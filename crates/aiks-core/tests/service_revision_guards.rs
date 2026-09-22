use std::{sync::Arc, time::Duration};

use aiks_core::{
    ai::{config::AiModelConfig, schema_v3::V3ExtractionResult, ModelService},
    indexing::{EmbeddingProvider, SessionIndexInput, SessionIndexService},
    model::SourceKind,
    pipeline::{
        input::SnapshotInput,
        repo::PipelineRepo,
        session_chunker::{chunk_for_llm, save_chunks_guarded},
        EmbeddingConfig, KnowledgeRepo,
    },
    service::{
        validate_submission, RevisionFence, ServiceStore, SnapshotReceipt, SnapshotSubmission,
        SupersededRevision,
    },
    storage::StateDb,
};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Semaphore;

#[path = "support/service_fixture.rs"]
mod fixture;

struct Fixture {
    _root: tempfile::TempDir,
    db: Arc<StateDb>,
    store: ServiceStore,
    input: SnapshotSubmission,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let db = Arc::new(StateDb::open_exclusive(&root.path().join("state.db")).unwrap());
        let store = ServiceStore::open(db.clone()).unwrap();
        let ctx = store.local_context();
        let reg = store.register_source(&ctx, SourceKind::Continue, "device").unwrap();
        let input = fixture::submission(ctx.space_id(), ctx.instance_id(), &reg, "u1", 0, "OLD_TEXT");
        Self { _root: root, db, store, input }
    }

    fn accept(&self) -> SnapshotReceipt {
        self.store.accept(&self.store.local_context(), &validate_submission(&self.input).unwrap())
            .unwrap().0
    }

    fn advance(&mut self) -> SnapshotReceipt {
        self.input.submission_id = "u2".into();
        self.input.expected_revision = 1;
        self.input.session.title = Some("Current title".into());
        self.input.session.messages[0].blocks[0] = aiks_core::ContentBlock::Text { text: "NEW_TEXT".into() };
        self.accept()
    }

    fn index_input(&self, receipt: &SnapshotReceipt, text: &str) -> SessionIndexInput {
        let (session, _) = SnapshotInput::load_for_run(&self.db, &receipt.pipeline_run_id).unwrap();
        SessionIndexInput {
            session_id: receipt.session_id.parse().unwrap(),
            external_id: session.external_session_id,
            source: session.source.as_str().into(),
            title: session.title,
            normalized_text: text.into(),
        }
    }

    fn indexer(&self) -> SessionIndexService {
        SessionIndexService::new(self.db.clone(), Arc::new(ModelService::new(
            AiModelConfig { enabled: false, ..Default::default() },
            EmbeddingConfig { enabled: false, ..Default::default() },
        ).unwrap()))
    }
}

fn fence(receipt: &SnapshotReceipt) -> RevisionFence {
    RevisionFence {
        session_id: receipt.session_id.parse().unwrap(),
        snapshot_id: receipt.snapshot_id.clone(),
        revision: receipt.revision,
    }
}

fn superseded(error: anyhow::Error) {
    assert!(error.downcast_ref::<SupersededRevision>().is_some(), "{error:#}");
}

fn extraction() -> V3ExtractionResult {
    serde_json::from_value(json!({
        "session_summary":"synthetic","knowledge_score":1.0,"worth_extracting":true,
        "items":[{"title":"Guarded knowledge","category":"implementation","summary":"Synthetic summary",
            "content":"Synthetic content","problem":null,"root_causes":null,"solutions":null,
            "key_commands":null,"key_files":null,"decisions":null,"tags":[],"confidence":1.0}]
    })).unwrap()
}

#[test]
fn stale_fence_blocks_chunks_knowledge_and_success_in_their_write_transactions() {
    let mut f = Fixture::new();
    let old = f.accept();
    let old_fence = fence(&old);
    let chunks = chunk_for_llm(old_fence.session_id, &f.input.session.messages).chunks;
    f.advance();
    superseded(save_chunks_guarded(&f.db, &chunks, Some(&old_fence)).unwrap_err());
    superseded(KnowledgeRepo::new(&f.db)
        .save_items_guarded(old_fence.session_id, None, &extraction(), Some(&old_fence)).unwrap_err());
    superseded(PipelineRepo::new(&f.db)
        .mark_finished_guarded(&old.pipeline_run_id, "READY", Some(&old_fence)).unwrap_err());
    for table in ["session_chunk", "knowledge_item", "knowledge_fts"] {
        let n: i64 = f.db.conn().query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0)).unwrap();
        assert_eq!(n, 0, "{table}");
    }
}

#[tokio::test]
async fn stale_fence_blocks_first_index_build_and_unchanged_fts_shortcut() {
    for existing in [false, true] {
        let mut f = Fixture::new();
        let old = f.accept();
        let guard = fence(&old);
        let indexer = f.indexer();
        if existing {
            indexer.index_session_guarded(f.index_input(&old, "SAME_TEXT"), Some(&guard)).await.unwrap();
        }
        let current = f.advance();
        superseded(indexer.index_session_guarded(f.index_input(&old, "SAME_TEXT"), Some(&guard)).await.unwrap_err());
        let n: i64 = f.db.conn().query_row("SELECT COUNT(*) FROM session_search_fts", [], |row| row.get(0)).unwrap();
        assert_eq!(n, i64::from(existing));
        indexer.index_session_guarded(f.index_input(&current, "NEW_TEXT"), Some(&fence(&current))).await.unwrap();
        let text: String = f.db.conn().query_row("SELECT content FROM session_search_fts", [], |row| row.get(0)).unwrap();
        assert_eq!(text, "NEW_TEXT");
    }
}

struct GatedEmbedding {
    seen: Semaphore,
    release: Semaphore,
}

#[async_trait]
impl EmbeddingProvider for GatedEmbedding {
    fn enabled(&self) -> bool { true }
    fn model_name(&self) -> &str { "synthetic-vector" }
    fn dimensions(&self) -> Option<usize> { Some(3) }
    async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        self.seen.add_permits(1);
        self.release.acquire().await.unwrap().forget();
        Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
    }
}

#[tokio::test]
async fn late_embedding_cannot_replace_new_fts_vectors_or_index_version() {
    let mut f = Fixture::new();
    let old = f.accept();
    let embed = Arc::new(GatedEmbedding { seen: Semaphore::new(0), release: Semaphore::new(0) });
    let indexer = SessionIndexService::new(f.db.clone(), embed.clone());
    let old_input = f.index_input(&old, "OLD_TEXT");
    let old_guard = fence(&old);
    let task = tokio::spawn(async move { indexer.index_session_guarded(old_input, Some(&old_guard)).await });
    tokio::time::timeout(Duration::from_secs(5), embed.seen.acquire()).await.unwrap().unwrap().forget();
    let current = f.advance();
    f.indexer().index_session_guarded(f.index_input(&current, "NEW_TEXT"), Some(&fence(&current))).await.unwrap();
    embed.release.add_permits(1);
    superseded(tokio::time::timeout(Duration::from_secs(5), task).await.unwrap().unwrap().unwrap_err());
    let conn = f.db.conn();
    let text: String = conn.query_row("SELECT content FROM session_search_fts", [], |row| row.get(0)).unwrap();
    let vectors: i64 = conn.query_row("SELECT COUNT(*) FROM session_embedding_record", [], |row| row.get(0)).unwrap();
    let revision: u32 = conn.query_row("SELECT indexed_revision FROM service_derived_state", [], |row| row.get(0)).unwrap();
    assert_eq!(text, "NEW_TEXT");
    assert_eq!(vectors, 0);
    assert_eq!(revision, 2);
}

#[test]
fn invalid_fence_identity_and_database_errors_are_not_supersession() {
    let f = Fixture::new();
    let receipt = f.accept();
    let mut guard = fence(&receipt);
    let mut conn = f.db.conn();
    let tx = conn.transaction().unwrap();
    guard.check_in_tx(&tx).unwrap();
    guard.snapshot_id = "not-a-snapshot".into();
    assert!(guard.check_in_tx(&tx).unwrap_err().downcast_ref::<SupersededRevision>().is_none());
    tx.rollback().unwrap();
    conn.execute_batch("ALTER TABLE service_session_binding RENAME TO unavailable_bindings").unwrap();
    let tx = conn.transaction().unwrap();
    assert!(fence(&receipt).check_in_tx(&tx).unwrap_err().downcast_ref::<SupersededRevision>().is_none());
}

#[test]
fn current_revision_can_commit_and_failed_knowledge_write_rolls_back_freshness() {
    let mut f = Fixture::new();
    let old = f.accept();
    let old_guard = fence(&old);
    KnowledgeRepo::new(&f.db).save_items_guarded(old_guard.session_id, None, &extraction(), Some(&old_guard)).unwrap();
    let current = f.advance();
    f.db.conn().execute_batch("CREATE TRIGGER fail_knowledge BEFORE UPDATE ON knowledge_item BEGIN SELECT RAISE(ABORT, 'synthetic failure'); END;").unwrap();
    let error = KnowledgeRepo::new(&f.db).save_items_guarded(old_guard.session_id, None, &extraction(), Some(&fence(&current))).unwrap_err();
    assert!(error.downcast_ref::<SupersededRevision>().is_none());
    let revision: u32 = f.db.conn().query_row("SELECT knowledge_revision FROM service_derived_state", [], |row| row.get(0)).unwrap();
    assert_eq!(revision, 1);
    f.db.conn().execute_batch("DROP TRIGGER fail_knowledge").unwrap();
    KnowledgeRepo::new(&f.db).save_items_guarded(old_guard.session_id, None, &extraction(), Some(&fence(&current))).unwrap();
    PipelineRepo::new(&f.db).mark_finished_guarded(&current.pipeline_run_id, "READY", Some(&fence(&current))).unwrap();
    let revision: u32 = f.db.conn().query_row("SELECT knowledge_revision FROM service_derived_state", [], |row| row.get(0)).unwrap();
    assert_eq!(revision, 2);
}
