use std::{collections::HashMap, sync::Arc};

use aiks_core::{
    config::Config,
    model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind},
    providers::{ProviderHealth, ProviderRegistry, SessionProvider, SessionSummary},
    renderer::knowledge::{render_knowledge_item_md, KnowledgeItemDoc},
    sink::SiYuanSink,
    storage::{SourceSessionRepo, StateDb, SyncTargetRepo},
    sync::{SyncEngine, SyncOptions},
};
use async_trait::async_trait;

#[test]
fn knowledge_document_keeps_native_session_provenance() {
    let markdown = render_knowledge_item_md(&KnowledgeItemDoc {
        knowledge_id: "knowledge-1",
        title: "V4.1 provenance",
        category: "architecture",
        project_name: Some("AIKS"),
        summary: "summary",
        content: "content",
        tags_json: r#"["V4.1"]"#,
        confidence: 0.95,
        source_display: "OpenCode",
        session_ext_id: "session-external-1",
        session_title: Some("source session"),
        session_doc_id: Some("20260916-session-doc"),
    });

    assert!(markdown.contains("查看原始 Session"));
    assert!(markdown.contains("((20260916-session-doc"));
    assert!(markdown.contains("Session ID: `session-external-1`"));
}

struct ChangedSessionProvider;

#[async_trait]
impl SessionProvider for ChangedSessionProvider {
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
        Ok(vec![SessionSummary {
            source: SourceKind::ClaudeCode,
            external_session_id: "raw-session-1".to_string(),
            title: Some("Raw session".to_string()),
            project_name: Some("AIKS".to_string()),
            project_path: None,
            source_path: None,
            started_at: None,
            updated_at: None,
            message_count: 1,
        }])
    }

    async fn load_session(&self, _: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        Ok(NormalizedSession {
            source: SourceKind::ClaudeCode,
            external_session_id: "raw-session-1".to_string(),
            title: Some("Raw session".to_string()),
            project_name: Some("AIKS".to_string()),
            project_path: None,
            source_path: None,
            started_at: None,
            updated_at: None,
            model: None,
            messages: vec![NormalizedMessage {
                external_id: "message-1".to_string(),
                parent_id: None,
                role: MessageRole::User,
                created_at: None,
                model: None,
                blocks: vec![ContentBlock::Text {
                    text: "source changed after the previous sync".to_string(),
                }],
                usage: None,
                metadata: HashMap::new(),
            }],
            usage: None,
            metadata: HashMap::new(),
        })
    }
}

fn markdown_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("md:{}", hex::encode(Sha256::digest(content.as_bytes())))
}

async fn remote_modified_server() -> (
    String,
    Arc<std::sync::Mutex<Vec<String>>>,
    tokio::task::JoinHandle<()>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_copy = seen.clone();

    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let mut request = Vec::new();
            loop {
                let mut chunk = [0u8; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..n]);
                if let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request[..header_end]);
                    let content_length = header
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= header_end + 4 + content_length {
                        break;
                    }
                }
            }

            let request_text = String::from_utf8_lossy(&request);
            let path = request_text
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .to_string();
            seen_copy.lock().unwrap().push(path.clone());

            let body = if path.contains("/api/query/sql") {
                serde_json::json!({
                    "code": 0,
                    "msg": "",
                    "data": [{"box": "box-1"}]
                })
            } else if path.contains("getBlockKramdown") {
                serde_json::json!({
                    "code": 0,
                    "msg": "",
                    "data": {"id": "raw-doc", "kramdown": "user edited remote raw session"}
                })
            } else if path.contains("getBlockAttrs") {
                serde_json::json!({
                    "code": 0,
                    "msg": "",
                    "data": {"custom-aiks-content-hash": "old-source-hash"}
                })
            } else {
                serde_json::json!({"code": 0, "msg": "", "data": null})
            };
            let body = body.to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });

    (base_url, seen, task)
}

fn seed_mapped_session(db: &StateDb, target_hash: Option<&str>) -> i64 {
    let session_id = SourceSessionRepo::new(db)
        .upsert(
            "claude_code",
            "raw-session-1",
            None,
            None,
            Some("AIKS"),
            Some("Raw session"),
            None,
            Some("old-source-hash"),
            Some("v1"),
        )
        .unwrap();

    let target_repo = SyncTargetRepo::new(db);
    target_repo.upsert_pending(session_id, "siyuan").unwrap();
    target_repo
        .mark_synced(
            session_id,
            "siyuan",
            "raw-doc",
            "/10 AI Sessions/ClaudeCode/raw-session-1",
            "old-source-hash",
            target_hash,
        )
        .unwrap();
    session_id
}

#[tokio::test]
async fn modified_raw_session_is_reported_as_conflict_and_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("aiks.db")).unwrap();
    seed_mapped_session(&db, Some(&markdown_hash("managed baseline")));

    let (base_url, seen, server) = remote_modified_server().await;
    let stats = SyncEngine::new(Arc::new(Config::default()))
        .run_sync(
            &db,
            &ProviderRegistry::new(vec![Box::new(ChangedSessionProvider)]),
            &SiYuanSink::embedded(base_url, "AI Knowledge").unwrap(),
            &SyncOptions::default(),
        )
        .await
        .unwrap();
    server.abort();

    assert_eq!(stats.conflict_count, 1);
    assert_eq!(stats.updated_count, 0);
    let paths = seen.lock().unwrap();
    assert!(paths.iter().any(|path| path.contains("/api/query/sql")));
    assert!(paths.iter().any(|path| path.contains("getBlockKramdown")));
    assert!(
        !paths.iter().any(|path| path.contains("updateBlock")),
        "modified raw Session must never be overwritten: {paths:?}"
    );
}

#[tokio::test]
async fn legacy_mapped_document_without_baseline_is_protected_from_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("aiks.db")).unwrap();
    seed_mapped_session(&db, None);

    let (base_url, seen, server) = remote_modified_server().await;
    let stats = SyncEngine::new(Arc::new(Config::default()))
        .run_sync(
            &db,
            &ProviderRegistry::new(vec![Box::new(ChangedSessionProvider)]),
            &SiYuanSink::embedded(base_url, "AI Knowledge").unwrap(),
            &SyncOptions::default(),
        )
        .await
        .unwrap();
    server.abort();

    assert_eq!(stats.conflict_count, 1);
    assert_eq!(stats.updated_count, 0);
    let paths = seen.lock().unwrap();
    assert!(paths.iter().any(|path| path.contains("/api/query/sql")));
    assert!(
        !paths.iter().any(|path| path.contains("getBlockKramdown")),
        "legacy baseline must fail closed before treating remote content as managed: {paths:?}"
    );
    assert!(
        !paths.iter().any(|path| path.contains("updateBlock")),
        "legacy mapped document without a baseline must fail closed: {paths:?}"
    );
}
