use std::sync::Arc;
use std::time::Duration;

use aiks_core::indexing::EmbeddingProvider;
use aiks_core::storage::StateDb;
use aiks_core::{
    CreateKnowledgeInput, KnowledgeService, SearchCorpus, UnifiedSearchFilter, UnifiedSearchService,
};
use async_trait::async_trait;

struct TestEmbedding {
    enabled: bool,
}

#[async_trait]
impl EmbeddingProvider for TestEmbedding {
    fn enabled(&self) -> bool {
        self.enabled
    }

    fn model_name(&self) -> &str {
        "synthetic-search-model"
    }

    fn dimensions(&self) -> Option<usize> {
        Some(3)
    }

    async fn embed(&self, _texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        std::future::pending().await
    }
}

fn fixture() -> (tempfile::TempDir, Arc<StateDb>) {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    KnowledgeService::new(&db)
        .create_manual_bound(
            "search-fixture",
            CreateKnowledgeInput {
                title: "LLM cache".into(),
                category: Some("general".into()),
                project_name: Some("synthetic".into()),
                summary: Some("Query cache".into()),
                content: "LLM cache and Chinese 磁盘空间不足".into(),
                tags: vec!["search".into()],
            },
            "synthetic-doc",
            "synthetic-hash",
            None,
        )
        .unwrap();
    (dir, db)
}

#[tokio::test]
async fn missing_knowledge_fts_preserves_hits_and_reports_fallback() {
    let (_dir, db) = fixture();
    db.conn().execute_batch("DROP TABLE knowledge_fts").unwrap();
    let service = UnifiedSearchService::new(db, Arc::new(TestEmbedding { enabled: false }));
    let outcome = service
        .search(
            "llm",
            10,
            UnifiedSearchFilter {
                corpora: vec![SearchCorpus::Knowledge],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(outcome.hits.len(), 1);
    assert!(outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("FTS")));
}

#[tokio::test]
async fn unavailable_session_index_does_not_discard_knowledge_hits() {
    let (_dir, db) = fixture();
    db.conn()
        .execute_batch("DROP TABLE session_search_fts")
        .unwrap();
    let service = UnifiedSearchService::new(db, Arc::new(TestEmbedding { enabled: false }));
    let outcome = service
        .search("llm", 10, UnifiedSearchFilter::default())
        .await
        .unwrap();
    assert!(outcome
        .hits
        .iter()
        .any(|hit| hit.entity_id == "search-fixture"));
    assert!(outcome.degraded);
}

#[tokio::test]
async fn stalled_embedding_returns_lexical_results_within_interactive_budget() {
    let (_dir, db) = fixture();
    let service = UnifiedSearchService::new(db, Arc::new(TestEmbedding { enabled: true }));
    let outcome = tokio::time::timeout(
        Duration::from_secs(12),
        service.search("llm", 10, UnifiedSearchFilter::default()),
    )
    .await
    .expect("search must not inherit an unbounded or 60-second embedding wait")
    .unwrap();
    assert!(outcome
        .hits
        .iter()
        .any(|hit| hit.entity_id == "search-fixture"));
    assert!(outcome.degraded);
    assert!(outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("timed out")));
}
