// Round 2 audit fixes — regression tests derived from
// docs/reviews/2026-09-13-audit-round2-reproduction.md.
//
// The original probes asserted BUG behavior (all 10 passed = defects present).
// This file asserts the FIXED behavior for R02..R15 items that live in
// aiks-core. Each test maps to an R-number in
// docs/reviews/2026-09-13-project-audit-round2.md.
//
// Run: cargo test -p aiks-core --test review_round2_fixes
use std::{collections::HashMap, sync::Arc};

use aiks_core::pipeline::{
    ai_stage::{parse_v3_result_typed, AiStage},
    cleaner::clean_messages,
    repo::PipelineRepo,
    session_chunker::{chunk_for_llm, save_chunks},
};
use aiks_core::{
    config::Config,
    model::*,
    providers::*,
    renderer::MarkdownRenderer,
    sink::SiYuanSink,
    storage::{SourceSessionRepo, StateDb, SyncStatus, SyncTargetRepo},
    sync::{SyncEngine, SyncOptions},
};
use async_trait::async_trait;

// ── Fixtures ──────────────────────────────────────────────────────────────────

fn msg(block: ContentBlock) -> NormalizedMessage {
    NormalizedMessage {
        external_id: "m".into(),
        parent_id: None,
        role: MessageRole::User,
        created_at: None,
        model: None,
        blocks: vec![block],
        usage: None,
        metadata: HashMap::new(),
    }
}

fn session(text: &str) -> NormalizedSession {
    NormalizedSession {
        source: SourceKind::ClaudeCode,
        external_session_id: "audit2".into(),
        title: Some("audit".into()),
        project_name: None,
        project_path: None,
        source_path: None,
        started_at: None,
        updated_at: None,
        model: None,
        messages: vec![msg(ContentBlock::Text { text: text.into() })],
        usage: None,
        metadata: HashMap::new(),
    }
}

struct Provider {
    fail: bool,
}

#[async_trait]
impl SessionProvider for Provider {
    fn source(&self) -> SourceKind {
        SourceKind::ClaudeCode
    }
    fn parser_version(&self) -> &'static str {
        "v1"
    }
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Ok
    }
    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        if self.fail {
            anyhow::bail!("audit provider unavailable");
        }
        Ok(vec![SessionSummary {
            source: SourceKind::ClaudeCode,
            external_session_id: "audit2".into(),
            title: None,
            project_name: None,
            project_path: None,
            source_path: None,
            started_at: None,
            updated_at: None,
            message_count: 1,
        }])
    }
    async fn load_session(&self, _: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        Ok(session("new"))
    }
}

fn insert(db: &StateDb, hash: &str) -> i64 {
    SourceSessionRepo::new(db)
        .upsert(
            "claude_code",
            "audit2",
            None,
            None,
            None,
            None,
            None,
            Some(hash),
            Some("v1"),
        )
        .unwrap()
}

/// Seed a session whose sync already succeeded against target "existing-doc".
fn seeded(db: &StateDb) -> i64 {
    let id = insert(db, "old");
    let repo = SyncTargetRepo::new(db);
    repo.upsert_pending(id, "siyuan").unwrap();
    // target_hash is the R05 baseline: hash of SiYuan's exported markdown
    // captured at the last successful sync. The mock exports "exported".
    repo.mark_synced(
        id,
        "siyuan",
        "existing-doc",
        "/old",
        "old",
        Some(&md_hash("exported")),
    )
    .unwrap();
    id
}

fn md_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("md:{}", hex::encode(Sha256::digest(content.as_bytes())))
}

/// Minimal HTTP mock: records request paths, serves canned SiYuan responses.
/// attr_fail makes setBlockAttrs return code=-1.
/// ai_garbage makes /completions return a non-JSON string.
async fn server(
    attr_fail: bool,
    ai_garbage: bool,
) -> (
    String,
    Arc<std::sync::Mutex<Vec<String>>>,
    tokio::task::JoinHandle<()>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(std::sync::Mutex::new(vec![]));
    let copy = seen.clone();
    let handle = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![];
            loop {
                let mut b = [0u8; 4096];
                let n = socket.read(&mut b).await.unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&b[..n]);
                if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&buf[..i]);
                    let len = header
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|s| s.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if buf.len() >= i + 4 + len {
                        break;
                    }
                }
            }
            let req = String::from_utf8_lossy(&buf);
            let path = req.split_whitespace().nth(1).unwrap_or("").to_string();
            copy.lock().unwrap().push(path.clone());

            let body = if path.contains("lsNotebooks") {
                // The sync engine writes session docs into the archive notebook
                // ("AI Session Archive") after the knowledge-sync change, so the
                // mock must expose it alongside the knowledge notebook "audit".
                serde_json::json!({"code":0,"msg":"","data":{"notebooks":[
                    {"id":"nb","name":"audit"},
                    {"id":"nba","name":"AI Session Archive"}
                ]}})
            } else if path.contains("createNotebook") {
                serde_json::json!({"code":0,"msg":"","data":{"notebook":{"id":"nbc","name":"created"}}})
            } else if path.contains("createDocWithMd") {
                serde_json::json!({"code":0,"msg":"","data":"new-doc"})
            } else if path.contains("getBlockKramdown") {
                // get_document_markdown uses /api/block/getBlockKramdown (the
                // embedded kernel lacks exportMdContent).
                serde_json::json!({"code":0,"msg":"","data":{"id":"doc","kramdown":"exported"}})
            } else if path.contains("getBlockAttrs") {
                serde_json::json!({"code":0,"msg":"","data":{"custom-aiks-content-hash":"old"}})
            } else if path.contains("setBlockAttrs") && attr_fail {
                serde_json::json!({"code":-1,"msg":"audit attr failure","data":null})
            } else if path.contains("completions") {
                if ai_garbage {
                    serde_json::json!({"choices":[{"message":{"content":"not valid JSON"}}]})
                } else {
                    serde_json::json!({"code":0,"msg":"","data":null})
                }
            } else {
                serde_json::json!({"code":0,"msg":"","data":null})
            };
            let body = body.to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    (url, seen, handle)
}

// ── R03: update uses the saved target_id, never re-creates ───────────────────

#[tokio::test]
async fn r03_update_uses_saved_target() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("db")).unwrap();
    let id = seeded(&db);
    let (url, seen, h) = server(false, false).await;
    let stats = SyncEngine::new(Arc::new(Config::default()))
        .run_sync(
            &db,
            &ProviderRegistry::new(vec![Box::new(Provider { fail: false })]),
            &SiYuanSink::embedded(url, "audit").unwrap(),
            &SyncOptions::default(),
        )
        .await
        .unwrap();
    h.abort();

    assert_eq!(stats.updated_count, 1);
    let paths = seen.lock().unwrap();
    // The saved target "existing-doc" must be updated, not re-created.
    assert!(
        paths.iter().any(|p| p.contains("updateBlock")),
        "expected updateBlock, got: {:?}",
        *paths
    );
    assert!(
        !paths.iter().any(|p| p.contains("createDocWithMd")),
        "must not create a duplicate document, got: {:?}",
        *paths
    );
    drop(paths);

    // State: synced_hash advanced to the new content hash, target_id unchanged.
    let target = SyncTargetRepo::new(&db)
        .find(id, "siyuan")
        .unwrap()
        .unwrap();
    assert_eq!(target.status, SyncStatus::Synced);
    assert_eq!(target.target_id.as_deref(), Some("existing-doc"));
    assert_ne!(target.synced_hash.as_deref(), Some("old"));
    // R05: baseline captured from the actual remote export.
    assert_eq!(
        target.target_hash.as_deref(),
        Some(md_hash("exported").as_str())
    );
}

// ── R04: dry-run must not swallow the subsequent real update ─────────────────

#[tokio::test]
async fn r04_dry_run_does_not_swallow_update() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("db")).unwrap();
    let id = seeded(&db);
    let engine = SyncEngine::new(Arc::new(Config::default()));
    let reg = ProviderRegistry::new(vec![Box::new(Provider { fail: false })]);
    let (url, seen, h) = server(false, false).await;
    let sink = SiYuanSink::embedded(url, "audit").unwrap();

    let dry = engine
        .run_sync(
            &db,
            &reg,
            &sink,
            &SyncOptions {
                dry_run: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let real = engine
        .run_sync(&db, &reg, &sink, &SyncOptions::default())
        .await
        .unwrap();
    h.abort();

    // Dry-run reports the update as intent, real run must still perform it.
    assert_eq!(dry.updated_count, 1);
    assert_eq!(
        real.updated_count, 1,
        "real sync after dry-run must not be UNCHANGED"
    );
    {
        let paths = seen.lock().unwrap();
        assert!(
            paths.iter().any(|p| p.contains("updateBlock")),
            "real sync must hit the API: {:?}",
            *paths
        );
    }
    let target = SyncTargetRepo::new(&db)
        .find(id, "siyuan")
        .unwrap()
        .unwrap();
    assert_ne!(
        target.synced_hash.as_deref(),
        Some("old"),
        "target hash must advance"
    );
}

// ── R05: attribute failure is a real failure, retry updates the same doc ─────

#[tokio::test]
async fn r05_attribute_failure_is_not_success_and_retry_reuses_doc() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("db")).unwrap();
    insert(&db, "hash");
    let (url, _seen, h) = server(true, false).await;

    // First run: create succeeds, attribute set FAILS.
    let stats = SyncEngine::new(Arc::new(Config::default()))
        .run_sync(
            &db,
            &ProviderRegistry::new(vec![Box::new(Provider { fail: false })]),
            &SiYuanSink::embedded(url.clone(), "audit").unwrap(),
            &SyncOptions::default(),
        )
        .await
        .unwrap();
    h.abort();

    assert_eq!(
        stats.failed_count, 1,
        "attr failure must surface as failure"
    );
    assert_eq!(stats.new_count, 0, "must not report success");
    let id = SourceSessionRepo::new(&db)
        .find_by_source_and_id("claude_code", "audit2")
        .unwrap()
        .unwrap()
        .id;
    let target = SyncTargetRepo::new(&db)
        .find(id, "siyuan")
        .unwrap()
        .unwrap();
    assert_ne!(
        target.status,
        SyncStatus::Synced,
        "must not be marked SYNCED"
    );
    assert_eq!(
        target.target_id.as_deref(),
        Some("new-doc"),
        "doc mapping must already be recorded so retry updates instead of duplicating"
    );

    // Second run with attrs working: retry must UPDATE the same document.
    let (url2, seen2, h2) = server(false, false).await;
    let stats2 = SyncEngine::new(Arc::new(Config::default()))
        .run_sync(
            &db,
            &ProviderRegistry::new(vec![Box::new(Provider { fail: false })]),
            &SiYuanSink::embedded(url2, "audit").unwrap(),
            &SyncOptions::default(),
        )
        .await
        .unwrap();
    h2.abort();

    assert_eq!(stats2.updated_count, 1);
    let creates = seen2
        .lock()
        .unwrap()
        .iter()
        .filter(|p| p.contains("createDocWithMd"))
        .count();
    assert_eq!(
        creates, 0,
        "retry must not create a second (orphan) document"
    );

    let target = SyncTargetRepo::new(&db)
        .find(id, "siyuan")
        .unwrap()
        .unwrap();
    assert_eq!(target.status, SyncStatus::Synced);
}

// ── R14: provider scan failure must not mark sessions missing ────────────────

#[tokio::test]
async fn r14_provider_failure_is_not_missing() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("db")).unwrap();
    insert(&db, "old");

    // Provider scan FAILS → must not mark the stored session as missing.
    let count = SyncEngine::new(Arc::new(Config::default()))
        .mark_missing_sessions(
            &db,
            &ProviderRegistry::new(vec![Box::new(Provider { fail: true })]),
        )
        .await
        .unwrap();
    assert_eq!(count, 0, "failed scan must never equal deletion");

    let stored = SourceSessionRepo::new(&db)
        .find_by_source_and_id("claude_code", "audit2")
        .unwrap()
        .unwrap();
    assert!(!stored.is_missing);
}

// ── R11: CJK content must not panic (cleaner + parse-error preview) ──────────

#[test]
fn r11_cleaner_cjk_no_panic() {
    let result = clean_messages(vec![msg(ContentBlock::ToolResult {
        id: None,
        content: "中".repeat(3000),
        is_error: false,
    })]);
    assert_eq!(result.cleaned_count, 1);
    // Content is truncated on char boundaries, still valid UTF-8.
    let text = match &result.messages[0].blocks[0] {
        ContentBlock::ToolResult { content, .. } => content.clone(),
        _ => panic!("wrong block"),
    };
    assert!(std::str::from_utf8(text.as_bytes()).is_ok());
}

#[test]
fn r11_parse_error_cjk_preview_no_panic() {
    let err = parse_v3_result_typed(&"中".repeat(40)).unwrap_err();
    assert!(!err.to_string().is_empty());
}

// ── R12: oversized CJK message stays in budget, tail preserved ───────────────

#[test]
fn r12_cjk_chunk_in_budget_and_tail_kept() {
    let chunks = chunk_for_llm(
        1,
        &[msg(ContentBlock::Text {
            text: format!("{}AUDIT_TAIL", "中".repeat(80_000)),
        })],
    );
    assert!(chunks.chunks.len() > 1, "oversized message must be split");
    for chunk in &chunks.chunks {
        assert!(
            chunk.token_count <= 20_000,
            "chunk exceeds token budget: {}",
            chunk.token_count
        );
    }
    let all: String = chunks
        .chunks
        .iter()
        .map(|c| c.content.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(all.contains("AUDIT_TAIL"), "tail must not be lost");
}

// ── R10: malformed AI response is a real failure, not a skip ─────────────────

#[tokio::test]
async fn r10_ai_stage_malformed_response_fails() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("db")).unwrap();
    let id = insert(&db, "hash");
    let run = PipelineRepo::new(&db)
        .upsert_pipeline_run(id, Some("hash"), "v3")
        .unwrap();
    save_chunks(&db, &chunk_for_llm(id, &session("hello").messages).chunks).unwrap();

    let (url, _, h) = server(false, true).await;
    let cfg = aiks_core::ai::config::AiModelConfig {
        base_url: url,
        ..Default::default()
    };
    let result = AiStage::new(cfg)
        .unwrap()
        .run(&db, &run, id, None, None)
        .await;
    h.abort();

    // R10: parse errors must surface as Err — not Ok(0) with a SUCCESS stage.
    assert!(result.is_err(), "malformed model output must be an error");
}

// ── R15: title secrets are sanitized in the final document ───────────────────

#[test]
fn r15_title_secret_sanitized() {
    let mut s = session("ordinary");
    s.title = Some("password=AUDITSECRET123".into());
    let md = MarkdownRenderer::new(Default::default(), true).render(&s);
    assert!(
        !md.contains("AUDITSECRET123"),
        "secret in title leaked into final document"
    );
}
