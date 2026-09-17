use std::sync::{Arc, Mutex};

use aiks_core::indexing::{EmbeddingProvider, KnowledgeIndexInput, KnowledgeIndexService};
use aiks_core::storage::StateDb;
use aiks_core::{CreateKnowledgeInput, KnowledgeService};
use async_trait::async_trait;
use rusqlite::params;
use tempfile::tempdir;

struct FakeEmbedding {
    enabled: bool,
    model: String,
    dimensions: usize,
    fail: bool,
    calls: Mutex<usize>,
}

impl FakeEmbedding {
    fn ready(model: &str, dimensions: usize) -> Self {
        Self {
            enabled: true,
            model: model.into(),
            dimensions,
            fail: false,
            calls: Mutex::new(0),
        }
    }

    fn failing(model: &str, dimensions: usize) -> Self {
        Self {
            enabled: true,
            model: model.into(),
            dimensions,
            fail: true,
            calls: Mutex::new(0),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for FakeEmbedding {
    fn enabled(&self) -> bool {
        self.enabled
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn dimensions(&self) -> Option<usize> {
        Some(self.dimensions)
    }

    async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        *self.calls.lock().unwrap() += 1;
        if self.fail {
            anyhow::bail!("synthetic embedding failure");
        }
        Ok(texts
            .into_iter()
            .map(|_| vec![0.25; self.dimensions])
            .collect())
    }
}

fn create_bound_knowledge(db: &StateDb, id: &str, doc_id: &str) {
    KnowledgeService::new(db)
        .create_manual_bound(
            id,
            CreateKnowledgeInput {
                title: "Canonical title".into(),
                category: Some("general".into()),
                project_name: Some("AIKS".into()),
                summary: Some("summary".into()),
                content: "initial cache".into(),
                tags: vec!["index".into()],
            },
            doc_id,
            "generated-hash",
            None,
        )
        .unwrap();
}

#[tokio::test]
async fn indexing_is_idempotent_for_same_hash_and_model() {
    let dir = tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    create_bound_knowledge(&db, "knowledge-1", "doc-1");
    let embedding = Arc::new(FakeEmbedding::ready("embed-v4", 3));
    let service = KnowledgeIndexService::new(db.clone(), embedding.clone());

    let first = service
        .index_document(KnowledgeIndexInput {
            knowledge_id: "knowledge-1".into(),
            siyuan_doc_id: "doc-1".into(),
            markdown: "canonical markdown".into(),
        })
        .await
        .unwrap();
    assert!(!first.skipped);
    assert_eq!(first.chunk_count, 1);
    assert_eq!(first.embedded_count, 1);

    let second = service
        .index_document(KnowledgeIndexInput {
            knowledge_id: "knowledge-1".into(),
            siyuan_doc_id: "doc-1".into(),
            markdown: "canonical markdown".into(),
        })
        .await
        .unwrap();
    assert!(second.skipped);
    assert_eq!(*embedding.calls.lock().unwrap(), 1);

    let record = KnowledgeService::new(&db).get("knowledge-1").unwrap().unwrap();
    assert_eq!(record.index_status, "ready");
    assert_eq!(record.embedding_model.as_deref(), Some("embed-v4"));
    assert_eq!(record.embedding_dimensions, Some(3));
    assert_eq!(record.index_chunk_count, 1);

    let conn = db.conn();
    let fts_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_fts WHERE knowledge_id = 'knowledge-1' AND content = 'canonical markdown'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let chunk_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_chunk WHERE knowledge_id = 'knowledge-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let vector_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM embedding_record er JOIN knowledge_chunk kc ON kc.id = er.chunk_id WHERE kc.knowledge_id = 'knowledge-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(fts_count, 1);
    assert_eq!(chunk_count, 1);
    assert_eq!(vector_count, 1);
}

#[tokio::test]
async fn changed_content_replaces_stale_derivatives() {
    let dir = tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    create_bound_knowledge(&db, "knowledge-2", "doc-2");
    let embedding = Arc::new(FakeEmbedding::ready("embed-v4", 2));
    let service = KnowledgeIndexService::new(db.clone(), embedding);

    service
        .index_document(KnowledgeIndexInput {
            knowledge_id: "knowledge-2".into(),
            siyuan_doc_id: "doc-2".into(),
            markdown: "first canonical body".into(),
        })
        .await
        .unwrap();

    let old_chunk_id: String = db
        .conn()
        .query_row(
            "SELECT id FROM knowledge_chunk WHERE knowledge_id = 'knowledge-2' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();

    let result = service
        .index_document(KnowledgeIndexInput {
            knowledge_id: "knowledge-2".into(),
            siyuan_doc_id: "doc-2".into(),
            markdown: "second canonical body".into(),
        })
        .await
        .unwrap();
    assert!(!result.skipped);

    let conn = db.conn();
    let old_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_chunk WHERE id = ?1",
            params![old_chunk_id],
            |row| row.get(0),
        )
        .unwrap();
    let new_text: String = conn
        .query_row(
            "SELECT text FROM knowledge_chunk WHERE knowledge_id = 'knowledge-2' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(old_count, 0);
    assert_eq!(new_text, "second canonical body");
}

#[tokio::test]
async fn embedding_failure_marks_failed_and_removes_stale_vectors() {
    let dir = tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    create_bound_knowledge(&db, "knowledge-3", "doc-3");

    let good = KnowledgeIndexService::new(
        db.clone(),
        Arc::new(FakeEmbedding::ready("embed-v4", 2)),
    );
    good.index_document(KnowledgeIndexInput {
        knowledge_id: "knowledge-3".into(),
        siyuan_doc_id: "doc-3".into(),
        markdown: "old body".into(),
    })
    .await
    .unwrap();

    let failing = KnowledgeIndexService::new(
        db.clone(),
        Arc::new(FakeEmbedding::failing("embed-v4", 2)),
    );
    let error = failing
        .index_document(KnowledgeIndexInput {
            knowledge_id: "knowledge-3".into(),
            siyuan_doc_id: "doc-3".into(),
            markdown: "new body that cannot be embedded".into(),
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("synthetic embedding failure"));

    let record = KnowledgeService::new(&db).get("knowledge-3").unwrap().unwrap();
    assert_eq!(record.index_status, "failed");
    assert!(record
        .last_index_error
        .as_deref()
        .unwrap_or_default()
        .contains("synthetic embedding failure"));

    let conn = db.conn();
    let chunks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_chunk WHERE knowledge_id = 'knowledge-3'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let vectors: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM embedding_record er JOIN knowledge_chunk kc ON kc.id = er.chunk_id WHERE kc.knowledge_id = 'knowledge-3'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(chunks, 0);
    assert_eq!(vectors, 0);
}

#[tokio::test]
async fn mark_deleted_removes_all_search_derivatives() {
    let dir = tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    create_bound_knowledge(&db, "knowledge-4", "doc-4");
    let service = KnowledgeIndexService::new(
        db.clone(),
        Arc::new(FakeEmbedding::ready("embed-v4", 2)),
    );
    service
        .index_document(KnowledgeIndexInput {
            knowledge_id: "knowledge-4".into(),
            siyuan_doc_id: "doc-4".into(),
            markdown: "body to delete".into(),
        })
        .await
        .unwrap();

    assert_eq!(service.mark_deleted("doc-4").unwrap().as_deref(), Some("knowledge-4"));

    let record = KnowledgeService::new(&db).get("knowledge-4").unwrap().unwrap();
    assert_eq!(record.status, "deleted");

    let conn = db.conn();
    let fts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_fts WHERE knowledge_id = 'knowledge-4'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let chunks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_chunk WHERE knowledge_id = 'knowledge-4'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(fts, 0);
    assert_eq!(chunks, 0);
}
