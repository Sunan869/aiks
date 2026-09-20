use std::{collections::HashMap, sync::Arc};

use aiks_core::{
    config::Config,
    model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind},
    providers::{ProviderHealth, ProviderRegistry, SessionProvider, SessionSummary},
    sink::SiYuanSink,
    storage::{SourceSessionRepo, StateDb, SyncStatus, SyncTargetRepo},
    sync::{SyncEngine, SyncOptions},
};
use async_trait::async_trait;

struct OversizedProvider;

#[async_trait]
impl SessionProvider for OversizedProvider {
    fn source(&self) -> SourceKind {
        SourceKind::ClaudeCode
    }

    fn parser_version(&self) -> &'static str {
        "oversized-v1"
    }

    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Ok
    }

    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        Ok(vec![SessionSummary {
            source: SourceKind::ClaudeCode,
            external_session_id: "oversized-session".into(),
            title: Some("Oversized session".into()),
            project_name: Some("AIKS".into()),
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
            external_session_id: "oversized-session".into(),
            title: Some("Oversized session".into()),
            project_name: Some("AIKS".into()),
            project_path: None,
            source_path: None,
            started_at: None,
            updated_at: None,
            model: None,
            messages: vec![NormalizedMessage {
                external_id: "oversized-message".into(),
                parent_id: None,
                role: MessageRole::User,
                created_at: None,
                model: None,
                blocks: vec![ContentBlock::Text {
                    text: "x".repeat(6 * 1024 * 1024),
                }],
                usage: None,
                metadata: HashMap::new(),
            }],
            usage: None,
            metadata: HashMap::new(),
        })
    }
}

async fn mock_siyuan() -> (
    String,
    Arc<std::sync::Mutex<Vec<String>>>,
    tokio::task::JoinHandle<()>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_copy = Arc::clone(&seen);

    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let mut request = Vec::new();
            loop {
                let mut chunk = [0u8; 4096];
                let Ok(n) = socket.read(&mut chunk).await else {
                    break;
                };
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

            let body = if path.contains("lsNotebooks") {
                serde_json::json!({
                    "code": 0,
                    "msg": "",
                    "data": {"notebooks": [{"id": "box-1", "name": "AI Knowledge"}]}
                })
                .to_string()
            } else {
                serde_json::json!({"code": 0, "msg": "", "data": null}).to_string()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    (base_url, seen, task)
}

#[tokio::test]
async fn oversized_session_is_permanent_and_unchanged_source_is_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let mut config = Config::default();
    config.content.minimum_messages = 0;
    let registry = ProviderRegistry::new(vec![Box::new(OversizedProvider)]);
    let (base_url, seen, server) = mock_siyuan().await;
    let sink = SiYuanSink::embedded(base_url, "AI Knowledge").unwrap();
    let engine = SyncEngine::new(Arc::new(config));

    let first = engine
        .run_sync(&db, &registry, &sink, &SyncOptions::default())
        .await
        .unwrap();
    assert_eq!(first.failed_count, 1);

    let source = SourceSessionRepo::new(&db)
        .find_by_source_and_id("claude_code", "oversized-session")
        .unwrap()
        .unwrap();
    let target = SyncTargetRepo::new(&db)
        .find(source.id, "siyuan")
        .unwrap()
        .unwrap();
    assert_eq!(target.status, SyncStatus::FailedPermanent);
    assert_eq!(target.retry_count, 1);
    assert!(
        target
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("too large")
    );

    let second = engine
        .run_sync(&db, &registry, &sink, &SyncOptions::default())
        .await
        .unwrap();
    server.abort();

    assert_eq!(second.skipped_count, 1);
    assert_eq!(second.failed_count, 0);
    let target = SyncTargetRepo::new(&db)
        .find(source.id, "siyuan")
        .unwrap()
        .unwrap();
    assert_eq!(target.status, SyncStatus::FailedPermanent);
    assert_eq!(target.retry_count, 1);

    let paths = seen.lock().unwrap();
    assert!(
        !paths.iter().any(|path| path.contains("createDocWithMd")),
        "oversized sessions must be rejected before any document write: {paths:?}"
    );
}
