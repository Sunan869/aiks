use std::{collections::HashMap, sync::Arc, time::Duration};

use aiks_core::{
    ai::config::AiModelConfig,
    model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind},
    pipeline::{EmbeddingConfig, PipelineJob, PipelineOrchestrator, PipelineWorker},
    providers::{ProviderHealth, ProviderRegistry, SessionProvider, SessionSummary},
    storage::{SourceSessionRepo, StateDb},
};
use async_trait::async_trait;

fn message(text: &str) -> NormalizedMessage {
    NormalizedMessage {
        external_id: "p0-message".into(),
        parent_id: None,
        role: MessageRole::User,
        created_at: None,
        model: None,
        blocks: vec![ContentBlock::Text { text: text.into() }],
        usage: None,
        metadata: HashMap::new(),
    }
}

fn session() -> NormalizedSession {
    NormalizedSession {
        source: SourceKind::ClaudeCode,
        external_session_id: "p0-session".into(),
        title: Some("P0 regression".into()),
        project_name: Some("aiks".into()),
        project_path: None,
        source_path: None,
        started_at: None,
        updated_at: None,
        model: None,
        messages: vec![message("A useful session that should produce one knowledge item.")],
        usage: None,
        metadata: HashMap::new(),
    }
}

struct FakeProvider;

#[async_trait]
impl SessionProvider for FakeProvider {
    fn source(&self) -> SourceKind {
        SourceKind::ClaudeCode
    }

    fn parser_version(&self) -> &'static str {
        "p0-v1"
    }

    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Ok
    }

    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        Ok(vec![SessionSummary {
            source: SourceKind::ClaudeCode,
            external_session_id: "p0-session".into(),
            title: Some("P0 regression".into()),
            project_name: Some("aiks".into()),
            project_path: None,
            source_path: None,
            started_at: None,
            updated_at: None,
            message_count: 1,
        }])
    }

    async fn load_session(&self, _: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        Ok(session())
    }
}

fn insert_session(db: &StateDb) -> i64 {
    SourceSessionRepo::new(db)
        .upsert(
            "claude_code",
            "p0-session",
            Some("P0 regression"),
            Some("aiks"),
            None,
            None,
            None,
            Some("p0-hash"),
            Some("p0-v1"),
        )
        .unwrap()
}

async fn mock_ai_and_embedding_server(
    embedding_status: u16,
) -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());

    let handle = tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            let mut buf = Vec::new();
            loop {
                let mut chunk = [0u8; 4096];
                let n = match socket.read(&mut chunk).await {
                    Ok(n) => n,
                    Err(_) => break,
                };
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(header_end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&buf[..header_end]);
                    let content_len = header
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if buf.len() >= header_end + 4 + content_len {
                        break;
                    }
                }
            }

            let request = String::from_utf8_lossy(&buf);
            let path = request.split_whitespace().nth(1).unwrap_or("");

            let (status_line, body) = if path.ends_with("/embeddings") {
                if embedding_status == 200 {
                    (
                        "HTTP/1.1 200 OK",
                        serde_json::json!({
                            "data": [{"embedding": [0.1, 0.2, 0.3], "index": 0}]
                        })
                        .to_string(),
                    )
                } else {
                    (
                        "HTTP/1.1 500 Internal Server Error",
                        serde_json::json!({"error": "forced embedding failure"}).to_string(),
                    )
                }
            } else {
                let extracted = serde_json::json!({
                    "session_summary": "p0 summary",
                    "knowledge_score": 0.9,
                    "worth_extracting": true,
                    "items": [{
                        "title": "P0 knowledge",
                        "category": "general",
                        "summary": "P0 knowledge summary",
                        "content": "P0 knowledge content",
                        "problem": null,
                        "root_causes": null,
                        "solutions": null,
                        "key_commands": null,
                        "key_files": null,
                        "decisions": null,
                        "tags": ["p0"],
                        "confidence": 0.95
                    }]
                })
                .to_string();
                (
                    "HTTP/1.1 200 OK",
                    serde_json::json!({
                        "choices": [{"message": {"content": extracted}}]
                    })
                    .to_string(),
                )
            };

            let response = format!(
                "{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status_line,
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    (url, handle)
}

async fn wait_for_terminal(db: &StateDb, run_id: &str) -> (String, Option<String>) {
    for _ in 0..200 {
        let row = db.conn().query_row(
            "SELECT status, error_stage FROM pipeline_run WHERE id = ?1",
            [run_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        );
        let (status, error_stage) = row.unwrap();
        if status != "DISCOVERED" && status != "PROCESSING" {
            return (status, error_stage);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("pipeline run did not reach a terminal state");
}

fn worker_configs(base_url: &str) -> (AiModelConfig, EmbeddingConfig) {
    let mut ai = AiModelConfig::default();
    ai.enabled = true;
    ai.base_url = base_url.to_string();
    ai.model = "p0-ai".into();
    ai.max_concurrent = 1;

    let mut embedding = EmbeddingConfig::default();
    embedding.enabled = true;
    embedding.base_url = base_url.to_string();
    embedding.model = "p0-embedding".into();
    embedding.batch_size = 16;
    (ai, embedding)
}

#[tokio::test]
async fn configured_embedding_http_failure_must_fail_pipeline_before_indexed_ready() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    let session_id = insert_session(&db);
    let run_id = PipelineOrchestrator::new(db.clone())
        .enqueue(session_id, Some("p0-hash"))
        .unwrap();
    let (url, server) = mock_ai_and_embedding_server(500).await;
    let (ai, embedding) = worker_configs(&url);
    let worker = PipelineWorker::start_with_limit(
        db.clone(),
        Arc::new(ProviderRegistry::new(vec![Box::new(FakeProvider)])),
        ai,
        embedding,
        1,
    );

    worker.submit(PipelineJob {
        pipeline_run_id: run_id.clone(),
        session_id,
        session_external_id: "p0-session".into(),
        source: "claude_code".into(),
        session_title: Some("P0 regression".into()),
        project_name: Some("aiks".into()),
    }).unwrap();

    let (status, error_stage) = wait_for_terminal(&db, &run_id).await;
    server.abort();

    assert_eq!(status, "FAILED");
    assert_eq!(error_stage.as_deref(), Some("EMBEDDED"));
    let indexed_success: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM pipeline_stage_run WHERE pipeline_run_id = ?1 AND stage = 'INDEXED' AND status = 'SUCCESS'",
            [&run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(indexed_success, 0, "INDEXED must not succeed after embedding failure");
}

#[tokio::test]
async fn configured_embedding_chunk_failure_must_fail_at_embed_chunked() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    let session_id = insert_session(&db);
    db.conn()
        .execute_batch(
            "CREATE TRIGGER force_p0_chunk_failure
             BEFORE INSERT ON knowledge_chunk
             BEGIN
               SELECT RAISE(FAIL, 'forced P0 knowledge chunk failure');
             END;",
        )
        .unwrap();
    let run_id = PipelineOrchestrator::new(db.clone())
        .enqueue(session_id, Some("p0-hash"))
        .unwrap();
    let (url, server) = mock_ai_and_embedding_server(200).await;
    let (ai, embedding) = worker_configs(&url);
    let worker = PipelineWorker::start_with_limit(
        db.clone(),
        Arc::new(ProviderRegistry::new(vec![Box::new(FakeProvider)])),
        ai,
        embedding,
        1,
    );

    worker.submit(PipelineJob {
        pipeline_run_id: run_id.clone(),
        session_id,
        session_external_id: "p0-session".into(),
        source: "claude_code".into(),
        session_title: Some("P0 regression".into()),
        project_name: Some("aiks".into()),
    }).unwrap();

    let (status, error_stage) = wait_for_terminal(&db, &run_id).await;
    server.abort();

    assert_eq!(status, "FAILED");
    assert_eq!(error_stage.as_deref(), Some("EMBED_CHUNKED"));
    let later_successes: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM pipeline_stage_run WHERE pipeline_run_id = ?1 AND stage IN ('EMBEDDED', 'INDEXED') AND status = 'SUCCESS'",
            [&run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(later_successes, 0, "later vector stages must not succeed after chunk failure");
}

#[test]
fn ai_defaults_are_safe_for_a_public_repository() {
    let config = AiModelConfig::default();
    assert!(!config.enabled, "AI must be opt-in for a fresh public install");
    let private_ip = ["10", "10", "23", "16"].join(".");
    assert!(
        !config.base_url.contains(&private_ip),
        "public defaults must not expose private infrastructure"
    );
    assert!(
        !config.display_name().contains("公司内部"),
        "public defaults must not encode company-internal topology"
    );
}
