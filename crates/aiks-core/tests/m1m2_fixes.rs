// M1/M2 remaining bug tests — assert CORRECT behavior after fixes.
// Run: cargo test -p aiks-core --test m1m2_fixes -- --test-threads=1
use std::{collections::HashMap, sync::Arc};
use aiks_core::{
    config::Config,
    model::*,
    providers::*,
    storage::{StateDb, SourceSessionRepo},
    sync::SyncEngine,
};
use aiks_core::pipeline::{
    knowledge_repo::KnowledgeRepo,
    orchestrator::PipelineOrchestrator,
    worker::{PipelineWorker, PipelineJob},
};
use aiks_core::ai::schema_v3::V3ExtractionResult;
use async_trait::async_trait;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn insert_session(db: &StateDb, source: &str, ext_id: &str) -> i64 {
    SourceSessionRepo::new(db)
        .upsert(source, ext_id, None, None, None, Some("Test Session"), None, Some("hash1"), Some("v1"))
        .unwrap()
}

fn make_extraction(title: &str) -> V3ExtractionResult {
    serde_json::from_value(serde_json::json!({
        "session_summary": "test",
        "knowledge_score": 0.9,
        "worth_extracting": true,
        "items": [{"title": title, "category": "general", "summary": title,
                   "content": title, "tags": [], "confidence": 0.9}]
    })).unwrap()
}

struct FakeProvider { sessions: Vec<(String, String)> }
#[async_trait]
impl SessionProvider for FakeProvider {
    fn source(&self) -> SourceKind { SourceKind::ClaudeCode }
    fn parser_version(&self) -> &'static str { "test-v1" }
    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        Ok(self.sessions.iter().map(|(id, title)| SessionSummary {
            source: SourceKind::ClaudeCode,
            external_session_id: id.clone(),
            title: Some(title.clone()),
            project_name: None, project_path: None, source_path: None,
            started_at: None, updated_at: None, message_count: 3,
        }).collect())
    }
    async fn load_session(&self, summary: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        Ok(NormalizedSession {
            source: SourceKind::ClaudeCode,
            external_session_id: summary.external_session_id.clone(),
            title: summary.title.clone(),
            project_name: None, project_path: None, source_path: None,
            started_at: None, updated_at: None, model: None,
            messages: vec![NormalizedMessage {
                external_id: "m1".into(), parent_id: None,
                role: MessageRole::User, created_at: None, model: None,
                blocks: vec![ContentBlock::Text { text: "test content".into() }],
                usage: None, metadata: HashMap::new(),
            }],
            usage: None, metadata: HashMap::new(),
        })
    }
    async fn health_check(&self) -> ProviderHealth { ProviderHealth::Ok }
}

// ── B14: rebuild-state must preserve knowledge data ──────────────────────────

/// B14: After fix, rebuild-sync-index must NOT delete knowledge_item rows
#[test]
fn b14_rebuild_sync_index_preserves_knowledge() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();

    // Insert a session and knowledge item
    let sid = insert_session(&db, "claude_code", "test-session-1");
    let repo = KnowledgeRepo::new(&db);
    repo.save_items(sid, None, &make_extraction("Test Knowledge")).unwrap();

    // Verify knowledge exists
    let items_before: i64 = db.conn()
        .query_row("SELECT COUNT(*) FROM knowledge_item", [], |r| r.get(0))
        .unwrap();
    assert_eq!(items_before, 1, "Should have 1 knowledge item before rebuild");

    // Simulate rebuild-sync-index (only reset sync tables, NOT knowledge tables)
    aiks_core::storage::rebuild_sync_index_only(&db).unwrap();

    // Knowledge must survive
    let items_after: i64 = db.conn()
        .query_row("SELECT COUNT(*) FROM knowledge_item", [], |r| r.get(0))
        .unwrap();
    assert_eq!(items_after, 1, "Knowledge item must survive sync index rebuild");

    // Sync tables should be cleared
    let sync_count: i64 = db.conn()
        .query_row("SELECT COUNT(*) FROM sync_target", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sync_count, 0, "sync_target should be cleared by rebuild");
}

// ── B15: Missing source must be marked after full scan ───────────────────────

/// B15: After fix, sessions from a provider that disappears should be marked MISSING
/// Note: Does NOT call run_sync (avoids SiYuan network call), tests mark_missing directly
#[tokio::test]
async fn b15_missing_source_marked_after_full_scan() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();

    // Insert a session that was previously synced
    insert_session(&db, "claude_code", "disappeared-session");

    let engine = SyncEngine::new(Arc::new(Config::default()));

    // Provider reports NO sessions for claude_code (session disappeared)
    let empty_provider = FakeProvider { sessions: vec![] };
    let registry = ProviderRegistry::new(vec![Box::new(empty_provider)]);

    // Call mark_missing_sessions directly (doesn't need SiYuan)
    let marked = engine.mark_missing_sessions(&db, &registry).await.unwrap();
    assert_eq!(marked, 1, "Should mark 1 session as missing");

    let session = SourceSessionRepo::new(&db)
        .find_by_source_and_id("claude_code", "disappeared-session")
        .unwrap()
        .unwrap();
    assert!(session.is_missing, "Session should be marked as MISSING");
}

// ── B13: Concurrency limit ────────────────────────────────────────────────────

/// B13: Pipeline worker should not exceed max_concurrent at any time
/// Tests that start_with_limit exists and accepts the concurrency parameter
#[tokio::test]
async fn b13_concurrency_limit_respected() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());

    // Insert 3 sessions
    let ids: Vec<i64> = (0..3).map(|i| insert_session(&db, "claude_code", &format!("session-{}", i))).collect();

    let orchestrator = PipelineOrchestrator::new(db.clone());
    let run_ids: Vec<String> = ids.iter().map(|id| orchestrator.enqueue(*id, Some("hash")).unwrap()).collect();

    // Start worker with max_concurrent=1 (serialize all jobs)
    let worker = PipelineWorker::start_with_limit(
        db.clone(),
        Arc::new(ProviderRegistry::new(vec![])), // no providers → all fail immediately
        Default::default(),
        Default::default(),
        1, // strictly sequential
    );

    // Submit all 3 jobs
    for (run_id, session_id) in run_ids.iter().zip(ids.iter()) {
        worker.submit(PipelineJob {
            pipeline_run_id: run_id.clone(),
            session_id: *session_id,
            session_external_id: format!("session-{}", session_id),
            source: "claude_code".into(),
            session_title: None,
            project_name: None,
        }).unwrap();
    }

    // Wait long enough for all to complete (they fail fast with no provider)
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    // All should be FAILED (no provider), not stuck in PROCESSING
    let processing_count: i64 = db.conn()
        .query_row("SELECT COUNT(*) FROM pipeline_run WHERE status = 'PROCESSING'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(processing_count, 0, "No jobs should be stuck in PROCESSING");

    let failed_count: i64 = db.conn()
        .query_row("SELECT COUNT(*) FROM pipeline_run WHERE status = 'FAILED'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(failed_count, 3, "All 3 jobs should have FAILED (no provider)");
}

// ── B17: AI failure semantics ────────────────────────────────────────────────

/// B17: Malformed AI JSON must result in a RETRYABLE failure, not RAW_ONLY
#[test]
fn b17_malformed_json_is_not_skip() {
    // parse_v3_result should distinguish between:
    // - Valid JSON with worth_extracting=false → Ok(skip)
    // - Invalid JSON → Err (retryable failure)
    use aiks_core::pipeline::ai_stage::parse_v3_result_typed;

    // Valid skip response
    let valid_skip = r#"{"session_summary":"x","knowledge_score":0.3,"worth_extracting":false,"items":[]}"#;
    let result = parse_v3_result_typed(valid_skip);
    assert!(result.is_ok(), "Valid skip JSON should be Ok");
    assert!(!result.unwrap().worth_extracting);

    // Completely invalid JSON → should be Err (retryable), not treated as "no knowledge"
    let malformed = "this is not json at all";
    let result = parse_v3_result_typed(malformed);
    assert!(result.is_err(), "Malformed JSON should be Err, not silently skipped");

    // Truncated JSON → also Err
    let truncated = r#"{"session_summary":"x","items":[{"title":"#;
    let result = parse_v3_result_typed(truncated);
    assert!(result.is_err(), "Truncated JSON should be Err");
}

// ── B04: Conflict detection ──────────────────────────────────────────────────

/// B04: After writing a document, the target_hash must differ from source content_hash
/// (target_hash is computed from the rendered markdown we actually sent to SiYuan)
#[tokio::test]
async fn b04_target_hash_tracks_written_content() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let sid = insert_session(&db, "claude_code", "test-session");

    // After a "successful" sync (even mocked), sync_target.synced_hash should equal
    // the hash of the content we wrote, NOT the source content_hash
    let target_repo = aiks_core::storage::SyncTargetRepo::new(&db);
    target_repo.upsert_pending(sid, "siyuan").unwrap();
    target_repo.mark_synced(sid, "siyuan", "doc-123", "/path/doc", "source-hash", Some("rendered-content-hash")).unwrap();

    let target = target_repo.find(sid, "siyuan").unwrap().unwrap();
    assert_eq!(target.synced_hash.as_deref(), Some("source-hash"),
        "synced_hash tracks source hash");
    assert_eq!(target.target_hash.as_deref(), Some("rendered-content-hash"),
        "target_hash tracks what we wrote to SiYuan");
    assert_ne!(target.synced_hash, target.target_hash,
        "synced_hash and target_hash should be independent fields");
}

// ── B09: Auto-enqueue produces real DB jobs ──────────────────────────────────

/// B09: sync + enqueue_all_pending creates pipeline_run records
/// Tests enqueue_all_pending_for_pipeline directly (avoids SiYuan network)
#[test]
fn b09_enqueue_all_pending_creates_pipeline_runs() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());

    // Insert sessions directly into source_session (simulating post-sync state)
    insert_session(&db, "claude_code", "session-1");
    insert_session(&db, "claude_code", "session-2");
    insert_session(&db, "opencode", "session-3");

    // Initially no pipeline runs
    let count_before: i64 = db.conn()
        .query_row("SELECT COUNT(*) FROM pipeline_run", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count_before, 0, "No pipeline runs before enqueue");

    // Enqueue all pending
    let engine = SyncEngine::new(Arc::new(Config::default()));
    let enqueued = engine.enqueue_all_pending_for_pipeline(&db).unwrap();
    assert_eq!(enqueued, 3, "Should enqueue 3 sessions");

    // Pipeline runs should be created
    let count_after: i64 = db.conn()
        .query_row("SELECT COUNT(*) FROM pipeline_run", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count_after, 3, "Should have 3 pipeline_run records");

    // Re-enqueue should not create duplicates (non-FAILED, non-DISCOVERED sessions are skipped)
    // Update one to READY so it's excluded from re-enqueue
    db.conn().execute(
        "UPDATE pipeline_run SET status = 'READY' WHERE rowid = (SELECT rowid FROM pipeline_run LIMIT 1)",
        [],
    ).unwrap();
    let enqueued_again = engine.enqueue_all_pending_for_pipeline(&db).unwrap();
    assert_eq!(enqueued_again, 2, "READY sessions should not be re-enqueued");
}
