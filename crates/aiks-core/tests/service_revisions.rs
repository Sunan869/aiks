//! Real snapshot worker regression: late model replies must not publish old data.
use std::{sync::Arc, time::Duration};

use aiks_core::{
    ai::config::AiModelConfig,
    model::SourceKind,
    pipeline::{EmbeddingConfig, KnowledgeRepo, PipelineWorker},
    service::{validate_submission, ServiceStore, SnapshotReceipt},
    storage::StateDb,
};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, Semaphore},
};

#[path = "support/service_fixture.rs"]
mod fixture;

struct GatedModel {
    base: String,
    seen: mpsc::Receiver<Value>,
    release: Arc<Semaphore>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for GatedModel {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl GatedModel {
    async fn next_request(&mut self) -> Value {
        tokio::time::timeout(Duration::from_secs(10), self.seen.recv())
            .await
            .expect("worker did not reach the loopback model")
            .expect("loopback fixture stopped")
    }

    fn reply(&self) {
        self.release.add_permits(1);
    }
}

async fn model() -> GatedModel {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let release = Arc::new(Semaphore::new(0));
    let gate = release.clone();
    let (sent, seen) = mpsc::channel(4);
    let task = tokio::spawn(async move {
        for title in ["OLD_REVISION_KNOWLEDGE", "CURRENT_REVISION_KNOWLEDGE"] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let payload = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let mut buffer = [0_u8; 8192];
                    let n = socket.read(&mut buffer).await.unwrap();
                    assert!(n > 0, "incomplete fixture request");
                    bytes.extend_from_slice(&buffer[..n]);
                    assert!(bytes.len() < 1024 * 1024);
                    if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]);
                        assert!(headers.starts_with("POST /v1/chat/completions "));
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|value| value.trim().parse::<usize>().ok())
                            })
                            .expect("fixture expects reqwest JSON content length");
                        if bytes.len() >= end + 4 + length {
                            break serde_json::from_slice::<Value>(
                                &bytes[end + 4..end + 4 + length],
                            )
                            .unwrap();
                        }
                    }
                }
            })
            .await
            .unwrap();
            if sent.send(payload).await.is_err() {
                return;
            }
            gate.acquire().await.unwrap().forget();
            let content = json!({
                "session_summary":"Synthetic revision test",
                "knowledge_score":0.99,
                "worth_extracting":true,
                "items":[{
                    "title":title,"category":"implementation","summary":title,
                    "content":format!("# {title}\nVerified synthetic revision content."),
                    "problem":null,"root_causes":null,"solutions":null,
                    "key_commands":null,"key_files":null,"decisions":null,
                    "tags":["revision-test"],"confidence":0.99
                }]
            })
            .to_string();
            let body = json!({"choices":[{"message":{"content":content}}]}).to_string();
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
    GatedModel {
        base,
        seen,
        release,
        task,
    }
}

fn job_status(db: &StateDb, receipt: &SnapshotReceipt) -> String {
    db.conn()
        .query_row(
            "SELECT status FROM pipeline_job WHERE id=?1",
            [&receipt.job_id],
            |row| row.get(0),
        )
        .unwrap()
}

async fn wait_done(db: &StateDb, receipt: &SnapshotReceipt) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let status = job_status(db, receipt);
            if status == "DONE" {
                break;
            }
            assert!(!matches!(status.as_str(), "FAILED" | "SUPERSEDED"));
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("current revision did not finish");
}

async fn late_result_case(title_only: bool) {
    let root = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open_exclusive(&root.path().join("state.db")).unwrap());
    let store = ServiceStore::open(db.clone()).unwrap();
    let ctx = store.local_context();
    let reg = store
        .register_source(&ctx, SourceKind::Continue, "synthetic-device")
        .unwrap();
    let mut input = fixture::submission(
        ctx.space_id(),
        ctx.instance_id(),
        &reg,
        "first",
        0,
        "REVISION_INPUT_ONE",
    );
    input.session.title = Some("Original title".into());
    let first = store
        .accept(&ctx, &validate_submission(&input).unwrap())
        .unwrap()
        .0;
    let mut remote = model().await;
    let ai = AiModelConfig {
        enabled: true,
        base_url: remote.base.clone(),
        model: "synthetic-revision-model".into(),
        ..Default::default()
    };
    let worker = PipelineWorker::start_from_snapshots(
        db.clone(),
        ai,
        EmbeddingConfig {
            enabled: false,
            ..Default::default()
        },
    );
    assert!(remote.next_request().await.to_string().contains("REVISION_INPUT_ONE"));
    input.submission_id = "second".into();
    input.expected_revision = 1;
    input.session.title = Some("Current title".into());
    if !title_only {
        input.session.messages[0].blocks[0] = aiks_core::ContentBlock::Text {
            text: "REVISION_INPUT_TWO".into(),
        };
    }
    let second = store
        .accept(&ctx, &validate_submission(&input).unwrap())
        .unwrap()
        .0;
    assert_eq!(second.revision, 2);
    remote.reply();
    let current_prompt = remote.next_request().await.to_string();
    assert!(current_prompt.contains(if title_only {
        "REVISION_INPUT_ONE"
    } else {
        "REVISION_INPUT_TWO"
    }));
    // The new model call is now blocked: inspect real state before its reply
    // could hide any stale writes made by the first response.
    assert_eq!(job_status(&db, &first), "SUPERSEDED");
    let session_id: i64 = second.session_id.parse().unwrap();
    assert!(KnowledgeRepo::new(&db)
        .get_by_session(session_id)
        .unwrap()
        .is_empty());
    let old_ready: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM pipeline_run WHERE id=?1 AND status='READY'",
            [&first.pipeline_run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(old_ready, 0);
    remote.reply();
    wait_done(&db, &second).await;
    let items = KnowledgeRepo::new(&db).get_by_session(session_id).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "CURRENT_REVISION_KNOWLEDGE");
    let title: String = db
        .conn()
        .query_row(
            "SELECT title FROM session_search_fts WHERE session_id=?1",
            [session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(title, "Current title");
    worker.shutdown(Duration::from_secs(2)).await.unwrap();
}

#[tokio::test]
async fn late_model_reply_cannot_write_old_knowledge_or_ready_state() {
    late_result_case(false).await;
}

#[tokio::test]
async fn title_only_revision_also_fences_old_model_results() {
    late_result_case(true).await;
}
