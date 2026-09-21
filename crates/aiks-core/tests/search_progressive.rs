use std::sync::Arc;
use std::time::Duration;

use aiks_core::indexing::EmbeddingProvider;
use aiks_core::storage::StateDb;
use aiks_core::{CreateKnowledgeInput, KnowledgeService, UnifiedSearchFilter, UnifiedSearchService};
use async_trait::async_trait;

struct PendingEmbedding;

#[async_trait]
impl EmbeddingProvider for PendingEmbedding {
    fn enabled(&self) -> bool { true }
    fn model_name(&self) -> &str { "synthetic" }
    fn dimensions(&self) -> Option<usize> { Some(2) }
    async fn embed(&self, _texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        std::future::pending().await
    }
}

fn fixture() -> (tempfile::TempDir, Arc<StateDb>) {
    let root = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&root.path().join("state.db")).unwrap());
    KnowledgeService::new(&db).create_manual_bound(
        "fixture", CreateKnowledgeInput {
            title: "LLM cache".into(), category: None,
            project_name: Some("synthetic".into()), summary: None,
            content: "磁盘空间不足 /var/lib/kubelet/pods qwen3.8:27b C# cache".into(),
            tags: vec![],
        }, "synthetic-doc", "synthetic-hash", None,
    ).unwrap();
    (root, db)
}

#[tokio::test]
async fn lexical_callback_arrives_without_waiting_for_embedding() {
    let (_root, db) = fixture();
    let service = UnifiedSearchService::new(db, Arc::new(PendingEmbedding));
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let mut sender = Some(sender);
    let task = tokio::spawn(async move {
        service.search_with_progress("LLM", 10, UnifiedSearchFilter::default(), move |partial| {
            if let Some(sender) = sender.take() { let _ = sender.send(partial.clone()); }
        }).await
    });
    let partial = tokio::time::timeout(Duration::from_secs(2), receiver).await.unwrap().unwrap();
    assert!(partial.hits.iter().any(|hit| hit.entity_id == "fixture"));
    assert!(!task.is_finished());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn cjk_technical_identifiers_and_filters_survive_indexed_recall() {
    let (_root, db) = fixture();
    let service = UnifiedSearchService::new(db, Arc::new(PendingEmbedding));
    for query in ["空间不足", "/var/lib/kubelet/pods", "qwen3.8:27b", "C#", "LLM \"OR\" cache*"] {
        let mut seen = false;
        let search = service.search_with_progress(query, 10, UnifiedSearchFilter::default(), |partial| {
            seen = partial.hits.iter().any(|hit| hit.entity_id == "fixture");
            assert!(!partial.degraded, "{:?}", partial.warnings);
        });
        let _ = tokio::time::timeout(Duration::from_millis(100), search).await;
        assert!(seen, "failed query: {query}");
    }
    let mut count = None;
    let search = service.search_with_progress("LLM", 10, UnifiedSearchFilter {
        project: Some("another-project".into()), ..Default::default()
    }, |partial| count = Some(partial.hits.len()));
    let _ = tokio::time::timeout(Duration::from_millis(100), search).await;
    assert_eq!(count, Some(0));
}
