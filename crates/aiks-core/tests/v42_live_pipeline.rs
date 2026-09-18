use std::{collections::HashMap, sync::{Arc, Mutex}};

use aiks_core::{
    config::Config,
    model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind},
    providers::{ProviderHealth, ProviderRegistry, SessionProvider, SessionSummary},
    sink::SiYuanSink,
    storage::StateDb,
    sync::{SyncEngine, SyncOptions},
};
use async_trait::async_trait;

struct OrderedProvider {
    events: Arc<Mutex<Vec<String>>>,
}

impl OrderedProvider {
    fn summary(id: &str) -> SessionSummary {
        SessionSummary {
            source: SourceKind::ClaudeCode,
            external_session_id: id.to_string(),
            title: Some(id.to_string()),
            project_name: None,
            project_path: None,
            source_path: None,
            started_at: None,
            updated_at: None,
            message_count: 1,
        }
    }

    fn session(id: &str) -> NormalizedSession {
        NormalizedSession {
            source: SourceKind::ClaudeCode,
            external_session_id: id.to_string(),
            title: Some(id.to_string()),
            project_name: None,
            project_path: None,
            source_path: None,
            started_at: None,
            updated_at: None,
            model: None,
            messages: vec![NormalizedMessage {
                external_id: format!("m-{id}"),
                parent_id: None,
                role: MessageRole::User,
                created_at: None,
                model: None,
                blocks: vec![ContentBlock::Text {
                    text: format!("content for {id}"),
                }],
                usage: None,
                metadata: HashMap::new(),
            }],
            usage: None,
            metadata: HashMap::new(),
        }
    }
}

#[async_trait]
impl SessionProvider for OrderedProvider {
    fn source(&self) -> SourceKind {
        SourceKind::ClaudeCode
    }

    fn parser_version(&self) -> &'static str {
        "v42-live-pipeline"
    }

    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        Ok(vec![Self::summary("first"), Self::summary("second")])
    }

    async fn load_session(&self, summary: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        self.events
            .lock()
            .unwrap()
            .push(format!("load:{}", summary.external_session_id));
        Ok(Self::session(&summary.external_session_id))
    }

    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Ok
    }
}

async fn siyuan_server() -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let handle = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            loop {
                let mut chunk = [0u8; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(header_end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&buf[..header_end]);
                    let len = header
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if buf.len() >= header_end + 4 + len {
                        break;
                    }
                }
            }

            let request = String::from_utf8_lossy(&buf);
            let path = request.split_whitespace().nth(1).unwrap_or("");
            let body = if path.contains("lsNotebooks") {
                serde_json::json!({"code":0,"msg":"","data":{"notebooks":[
                    {"id":"knowledge","name":"audit"},
                    {"id":"archive","name":"AI Session Archive"}
                ]}})
            } else if path.contains("createDocWithMd") {
                let id = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                serde_json::json!({"code":0,"msg":"","data":format!("doc-{id}")})
            } else if path.contains("getBlockKramdown") {
                serde_json::json!({"code":0,"msg":"","data":{"id":"doc","kramdown":"baseline"}})
            } else if path.contains("getBlockAttrs") {
                serde_json::json!({"code":0,"msg":"","data":{}})
            } else {
                serde_json::json!({"code":0,"msg":"","data":null})
            };
            let body = body.to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(), body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });

    (url, handle)
}

#[tokio::test]
async fn extraction_candidate_is_emitted_before_next_session_is_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let registry = ProviderRegistry::new(vec![Box::new(OrderedProvider {
        events: Arc::clone(&events),
    })]);
    let (url, server) = siyuan_server().await;
    let sink = SiYuanSink::embedded(url, "audit").unwrap();
    let callback_events = Arc::clone(&events);

    let stats = SyncEngine::new(Arc::new(Config::default()))
        .run_sync_with_candidate_handler(
            &db,
            &registry,
            &sink,
            &SyncOptions::default(),
            move |candidate| {
                callback_events
                    .lock()
                    .unwrap()
                    .push(format!("candidate:{}", candidate.external_session_id));
            },
        )
        .await
        .unwrap();
    server.abort();

    assert_eq!(stats.new_count, 2);
    assert_eq!(stats.extraction_candidates.len(), 2);
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            "load:first",
            "candidate:first",
            "load:second",
            "candidate:second",
        ]
    );
}

#[tokio::test]
async fn dry_run_never_emits_extraction_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let registry = ProviderRegistry::new(vec![Box::new(OrderedProvider {
        events: Arc::clone(&events),
    })]);
    let sink = SiYuanSink::embedded("http://127.0.0.1:1", "audit").unwrap();
    let emitted = Arc::new(Mutex::new(Vec::<String>::new()));
    let callback_emitted = Arc::clone(&emitted);

    SyncEngine::new(Arc::new(Config::default()))
        .run_sync_with_candidate_handler(
            &db,
            &registry,
            &sink,
            &SyncOptions {
                dry_run: true,
                ..Default::default()
            },
            move |candidate| {
                callback_emitted
                    .lock()
                    .unwrap()
                    .push(candidate.external_session_id.clone());
            },
        )
        .await
        .unwrap();

    assert!(emitted.lock().unwrap().is_empty());
}
