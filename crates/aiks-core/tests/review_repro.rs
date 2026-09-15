// Audit reproduction probes — these assert the BUG behavior exists at baseline.
// After each fix, the corresponding probe should FAIL (meaning the bug is gone).
// Replace bug-assertion probes with correct-behavior assertions after fixing.
//
// Run: cargo test -p aiks-core --test review_repro -- --test-threads=1
use aiks_core::pipeline::{knowledge_repo::KnowledgeRepo, session_chunker::chunk_for_llm};
use aiks_core::{
    config::{Config, ContentConfig},
    model::*,
    providers::*,
    renderer::MarkdownRenderer,
    sink::SiYuanSink,
    storage::{SourceSessionRepo, StateDb},
    sync::{SyncEngine, SyncOptions},
    util::SecretSanitizer,
};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};

fn msg(block: ContentBlock) -> NormalizedMessage {
    NormalizedMessage {
        external_id: "m1".into(),
        parent_id: None,
        role: MessageRole::User,
        created_at: None,
        model: None,
        blocks: vec![block],
        usage: None,
        metadata: HashMap::new(),
    }
}
fn session_with(block: ContentBlock) -> NormalizedSession {
    NormalizedSession {
        source: SourceKind::ClaudeCode,
        external_session_id: "audit-session".into(),
        title: Some("Audit".into()),
        project_name: None,
        project_path: None,
        source_path: None,
        started_at: None,
        updated_at: None,
        model: None,
        messages: vec![msg(block)],
        usage: None,
        metadata: HashMap::new(),
    }
}
fn summary() -> SessionSummary {
    SessionSummary {
        source: SourceKind::ClaudeCode,
        external_session_id: "audit-session".into(),
        title: None,
        project_name: None,
        project_path: None,
        source_path: None,
        started_at: None,
        updated_at: None,
        message_count: 1,
    }
}
struct FakeProvider;
#[async_trait]
impl SessionProvider for FakeProvider {
    fn source(&self) -> SourceKind {
        SourceKind::ClaudeCode
    }
    fn parser_version(&self) -> &'static str {
        "audit-v1"
    }
    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        Ok(vec![summary()])
    }
    async fn load_session(&self, _: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        Ok(session_with(ContentBlock::Text {
            text: "hello".into(),
        }))
    }
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Ok
    }
}
fn insert(db: &StateDb) -> i64 {
    SourceSessionRepo::new(db)
        .upsert(
            "claude_code",
            "audit-session",
            None,
            None,
            None,
            None,
            None,
            Some("hash"),
            Some("audit-v1"),
        )
        .unwrap()
}
fn extraction() -> aiks_core::ai::schema_v3::V3ExtractionResult {
    serde_json::from_value(serde_json::json!({
        "session_summary": "audit",
        "knowledge_score": 0.9,
        "worth_extracting": true,
        "items": [{
            "title": "auditneedle",
            "category": "general",
            "summary": "auditneedle",
            "content": "auditneedle",
            "tags": [],
            "confidence": 0.9
        }]
    }))
    .unwrap()
}

// ── M0: Unicode / Panic Safety ───────────────────────────────────────────────

/// B05: After fix, renderer must NOT panic on CJK tool result
#[test]
fn b05_renderer_cjk_no_panic() {
    let s = session_with(ContentBlock::ToolResult {
        id: None,
        content: "中".repeat(4000),
        is_error: false,
    });
    // Must not panic
    let _ = MarkdownRenderer::new(ContentConfig::default(), true).render(&s);
}

/// B05: After fix, V3 chunker must NOT panic on CJK
#[test]
fn b05_chunker_cjk_no_panic() {
    let m = msg(ContentBlock::ToolResult {
        id: None,
        content: "中".repeat(1000),
        is_error: false,
    });
    // Must not panic
    let _ = chunk_for_llm(1, &[m]);
}

/// B05: Emoji truncation must not panic
#[test]
fn b05_emoji_no_panic() {
    let emoji_text = "🔥".repeat(500);
    let m = msg(ContentBlock::Text { text: emoji_text });
    let _ = chunk_for_llm(1, &[m]);
}

// ── M0: Sanitizer Coverage ────────────────────────────────────────────────────

/// B06: After fix, Unknown JSON password must be redacted
#[test]
fn b06_unknown_json_password_redacted() {
    let s = session_with(ContentBlock::Unknown {
        raw: serde_json::json!({"password": "AUDIT_ONLY_PASSWORD"}),
    });
    let rendered = MarkdownRenderer::new(ContentConfig::default(), true).render(&s);
    assert!(
        !rendered.contains("AUDIT_ONLY_PASSWORD"),
        "password in Unknown JSON should be redacted"
    );
}

/// B06: After fix, all common secret patterns must be redacted
#[test]
fn b06_sanitizer_covers_common_patterns() {
    let san = SecretSanitizer::new();
    let cases = [
        (
            "AWS_SECRET_ACCESS_KEY=AUDIT_ONLY_123456",
            "AUDIT_ONLY_123456",
        ),
        ("password=\"AUDIT_ONLY_123456\"", "AUDIT_ONLY_123456"),
        ("SecretKey=AUDIT_ONLY_123456", "AUDIT_ONLY_123456"),
        ("token = AUDIT_ONLY_123456", "AUDIT_ONLY_123456"),
    ];
    for (input, secret) in cases {
        let output = san.sanitize(input);
        assert!(!output.contains(secret), "Pattern not sanitized: {input}");
    }
}

// ── M1: Sync correctness ──────────────────────────────────────────────────────

/// B03: After fix, failure then retry should re-attempt (not return UNCHANGED)
#[tokio::test]
async fn b03_failure_should_be_retried() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let engine = SyncEngine::new(Arc::new(Config::default()));
    let registry = ProviderRegistry::new(vec![Box::new(FakeProvider)]);
    let bad_sink = SiYuanSink::embedded("http://127.0.0.1:1", "audit").unwrap();
    let first = engine
        .run_sync(&db, &registry, &bad_sink, &SyncOptions::default())
        .await
        .unwrap();
    let second = engine
        .run_sync(&db, &registry, &bad_sink, &SyncOptions::default())
        .await
        .unwrap();
    // After fix: first should fail, second should retry (failed again), not UNCHANGED
    assert_eq!(first.failed_count, 1, "First sync should fail");
    assert_eq!(
        second.unchanged_count, 0,
        "Second sync should retry, not report UNCHANGED"
    );
    assert_eq!(
        second.failed_count, 1,
        "Second sync should fail again (still offline)"
    );
}

/// B03: After fix, dry-run must NOT poison the sync state
#[tokio::test]
async fn b03_dry_run_no_poisoning() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let engine = SyncEngine::new(Arc::new(Config::default()));
    let registry = ProviderRegistry::new(vec![Box::new(FakeProvider)]);
    let bad_sink = SiYuanSink::embedded("http://127.0.0.1:1", "audit").unwrap();
    let dry = engine
        .run_sync(
            &db,
            &registry,
            &bad_sink,
            &SyncOptions {
                dry_run: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let real = engine
        .run_sync(&db, &registry, &bad_sink, &SyncOptions::default())
        .await
        .unwrap();
    // dry-run discovered 1, real should attempt sync (fail, not UNCHANGED)
    assert_eq!(dry.new_count, 1, "Dry-run should report 1 new");
    assert_eq!(
        real.unchanged_count, 0,
        "Real sync after dry-run must not be UNCHANGED"
    );
    assert_eq!(
        real.failed_count, 1,
        "Real sync should actually try and fail"
    );
    let target_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM sync_target", [], |r| r.get(0))
        .unwrap();
    assert_eq!(target_count, 0, "Dry-run must not create sync_target rows");
}

// ── M1: SiYuan API ────────────────────────────────────────────────────────────

/// B02: After fix, SiYuan string ID response must be accepted
#[tokio::test]
async fn b02_siyuan_string_id_accepted() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0; 8192];
        let _ = stream.read(&mut buf).await.unwrap();
        let body = r#"{"code":0,"msg":"","data":"20260913000000-auditxx"}"#;
        stream.write_all(
            format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body).as_bytes()
        ).await.unwrap();
    });
    let result = SiYuanSink::embedded(format!("http://{}", addr), "audit")
        .unwrap()
        .create_document("notebook", "/audit", "hello")
        .await;
    server.await.unwrap();
    assert!(
        result.is_ok(),
        "String ID response should be accepted, got: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap(), "20260913000000-auditxx");
}

// ── M2: Knowledge Pipeline ────────────────────────────────────────────────────

/// B11: After fix, re-extraction should NOT fail with FK constraint
#[test]
fn b11_reextraction_no_fk_failure() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let sid = insert(&db);
    let repo = KnowledgeRepo::new(&db);
    let ids = repo.save_items(sid, None, &extraction()).unwrap();
    // Save embedding chunks
    repo.save_embedding_chunks(&ids[0], &[(None, "audit chunk".into())])
        .unwrap();
    // Re-extraction must succeed, not fail with FOREIGN KEY
    let result = repo.save_items(sid, None, &extraction());
    assert!(
        result.is_ok(),
        "Re-extraction should succeed, got: {:?}",
        result.err()
    );
}

/// B11: After fix, FTS should not keep orphans after re-extraction
#[tokio::test]
async fn b16_fts_no_orphans_after_reextraction() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let sid = insert(&db);
    let repo = KnowledgeRepo::new(&db);
    let _old = repo.save_items(sid, None, &extraction()).unwrap();
    let _new = repo.save_items(sid, None, &extraction()).unwrap();
    let hits = aiks_core::pipeline::hybrid_search(&db, "auditneedle", 20, None)
        .await
        .unwrap();
    // After fix: only items that exist should appear in results
    for hit in &hits {
        let detail = repo.get_by_id(&hit.knowledge_id).unwrap();
        assert!(
            detail.is_some(),
            "FTS hit {} has no corresponding knowledge item (orphan)",
            hit.knowledge_id
        );
    }
    // Exactly 1 item should exist (the new one)
    assert_eq!(
        hits.len(),
        1,
        "After re-extraction, should have exactly 1 hit (no orphans)"
    );
}

// ── M2: Worker Lifecycle ──────────────────────────────────────────────────────

/// B12: After fix, worker with invalid provider should mark FAILED, not PROCESSING
#[tokio::test]
async fn b12_worker_marks_failed_on_provider_error() {
    use aiks_core::pipeline::{
        orchestrator::PipelineOrchestrator,
        worker::{PipelineJob, PipelineWorker},
    };
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    let sid = insert(&db);
    let run = PipelineOrchestrator::new(db.clone())
        .enqueue(sid, Some("hash"))
        .unwrap();
    let worker = PipelineWorker::start(
        db.clone(),
        Arc::new(ProviderRegistry::new(vec![])), // no providers
        Default::default(),
        Default::default(),
    );
    worker
        .submit(PipelineJob {
            pipeline_run_id: run.clone(),
            session_id: sid,
            session_external_id: "audit-session".into(),
            source: "claude_code".into(),
            session_title: None,
            project_name: None,
        })
        .unwrap();
    // Wait for worker to process
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let status: String = db
        .conn()
        .query_row("SELECT status FROM pipeline_run WHERE id=?1", [&run], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        status, "FAILED",
        "Worker with no provider should mark run as FAILED, not PROCESSING"
    );
}

// ── Hash correctness ──────────────────────────────────────────────────────────

/// B19: After fix, title change must change hash
#[test]
fn b19_hash_includes_title() {
    let mut s = session_with(ContentBlock::Image {
        source: "a".repeat(130),
        media_type: None,
    });
    let original = aiks_core::model::hash::compute_session_hash(&s);
    s.title = Some("changed title".into());
    let new_hash = aiks_core::model::hash::compute_session_hash(&s);
    assert_ne!(
        original, new_hash,
        "Title change should change session hash"
    );
}

/// B19: After fix, image tail change must change hash
#[test]
fn b19_hash_includes_full_image() {
    let s1 = session_with(ContentBlock::Image {
        source: "a".repeat(130),
        media_type: None,
    });
    let s2 = session_with(ContentBlock::Image {
        source: format!("{}bb", "a".repeat(128)),
        media_type: None,
    });
    let h1 = aiks_core::model::hash::compute_session_hash(&s1);
    let h2 = aiks_core::model::hash::compute_session_hash(&s2);
    assert_ne!(
        h1, h2,
        "Image content change (beyond byte 128) should change hash"
    );
}

// ── CLI correctness ───────────────────────────────────────────────────────────

/// B22: After fix, resync with session ID must reset the hash
#[test]
fn b22_resync_by_id_resets_hash() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    insert(&db);
    // Reset hash for this session (with proper source context)
    db.reset_hashes_for_resync(Some("claude_code"), &["audit-session".into()])
        .unwrap();
    let session = SourceSessionRepo::new(&db)
        .find_by_source_and_id("claude_code", "audit-session")
        .unwrap()
        .unwrap();
    assert!(
        session.content_hash.is_none(),
        "resync should reset content_hash to NULL, got {:?}",
        session.content_hash
    );
}

// ── Chunk Token Limit ─────────────────────────────────────────────────────────

/// B23: After fix, single very long message must be handled without exceeding limit
#[test]
fn b23_single_long_message_within_limit() {
    let c = chunk_for_llm(
        1,
        &[msg(ContentBlock::Text {
            text: "x".repeat(100_000),
        })],
    );
    // After fix: should either be one chunk within limit, or split into multiple chunks
    // but total estimated tokens per individual chunk must be reasonable
    let max_tokens_per_chunk = 25_000; // allow some headroom over 20k
    for chunk in &c.chunks {
        assert!(
            chunk.token_count <= max_tokens_per_chunk,
            "Chunk {} has {} tokens, exceeding limit",
            chunk.chunk_index,
            chunk.token_count
        );
    }
}
