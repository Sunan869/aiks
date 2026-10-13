use std::{
    ffi::OsString,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use aiks_core::{
    ai::schema_v3::{V3ExtractionResult, V3KnowledgeItem},
    engine::{AiksEngine, AiksEngineConfig},
    pipeline::KnowledgeRepo,
    storage::{SourceSessionRepo, StateDb},
    sync::SyncOptions,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::oneshot,
};

fn env_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

struct DataDirGuard {
    previous: Option<OsString>,
}

impl DataDirGuard {
    fn set(path: &Path) -> Self {
        let previous = std::env::var_os("AIKS_DATA_DIR");
        std::env::set_var("AIKS_DATA_DIR", path);
        Self { previous }
    }
}

impl Drop for DataDirGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var("AIKS_DATA_DIR", value),
            None => std::env::remove_var("AIKS_DATA_DIR"),
        }
    }
}

fn write_config(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        r#"
[providers.claude]
enabled = false
[providers.codex]
enabled = false
[providers.gemini]
enabled = false
[providers.opencode]
enabled = false

[ai]
enabled = false
auto_extract = false

[embedding]
enabled = false
"#,
    )
    .unwrap();
    path
}

fn seed_knowledge(data_dir: &Path) {
    let db = StateDb::open(&data_dir.join("aiks.db")).unwrap();
    let session_id = SourceSessionRepo::new(&db)
        .upsert(
            "opencode",
            "lock-scope-session",
            None,
            None,
            Some("lock-scope"),
            Some("Lock Scope Session"),
            None,
            Some("hash-1"),
            Some("test-v1"),
        )
        .unwrap();

    let item = V3KnowledgeItem {
        title: "SiYuan lock scope".into(),
        category: "architecture".into(),
        summary: "knowledge sync must not block raw sync".into(),
        content: "concurrency regression fixture".into(),
        problem: None,
        root_causes: None,
        solutions: None,
        key_commands: None,
        key_files: None,
        decisions: None,
        tags: vec!["sync".into()],
        confidence: 0.95,
    };
    KnowledgeRepo::new(&db)
        .save_items(
            session_id,
            Some("lock-scope"),
            &V3ExtractionResult {
                session_summary: "fixture".into(),
                knowledge_score: 0.95,
                worth_extracting: true,
                items: vec![item],
            },
        )
        .unwrap();
}

async fn mock_siyuan(
    first_response_delay: Duration,
) -> (
    String,
    Arc<Mutex<Vec<String>>>,
    oneshot::Receiver<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_copy = Arc::clone(&seen);
    let (first_tx, first_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        let mut first_tx = Some(first_tx);
        let mut first = true;
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
                            line.to_lowercase()
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
            let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
            seen_copy.lock().unwrap().push(path.clone());

            if first {
                first = false;
                if let Some(tx) = first_tx.take() {
                    let _ = tx.send(());
                }
                if !first_response_delay.is_zero() {
                    tokio::time::sleep(first_response_delay).await;
                }
            }

            let body = if path.contains("lsNotebooks") {
                serde_json::json!({"code":0,"msg":"","data":{"notebooks":[
                    {"id":"knowledge-nb","name":"AI Knowledge"},
                    {"id":"session-nb","name":"AI Session Archive"}
                ]}})
            } else if path.contains("createDocWithMd") {
                serde_json::json!({"code":0,"msg":"","data":"knowledge-doc"})
            } else if path.contains("getBlockKramdown") {
                serde_json::json!({"code":0,"msg":"","data":{"id":"knowledge-doc","kramdown":"remote baseline"}})
            } else {
                serde_json::json!({"code":0,"msg":"","data":null})
            }
            .to_string();

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

    (url, seen, first_rx, handle)
}

fn make_engine(config_path: std::path::PathBuf, url: String) -> Arc<AiksEngine> {
    Arc::new(
        AiksEngine::initialize(AiksEngineConfig {
            config_path: Some(config_path),
            siyuan_base_url: Some(url),
            siyuan_token: None,
        })
        .unwrap(),
    )
}

#[tokio::test]
async fn slow_knowledge_push_does_not_block_unrelated_raw_sync() {
    let _env = env_lock().lock().await;
    let dir = tempfile::tempdir().unwrap();
    let _data_dir = DataDirGuard::set(dir.path());
    let config_path = write_config(dir.path());
    let (url, _seen, first_request, server) = mock_siyuan(Duration::from_millis(1200)).await;
    let engine = make_engine(config_path, url);
    seed_knowledge(dir.path());

    let knowledge_engine = Arc::clone(&engine);
    let knowledge =
        tokio::spawn(async move { knowledge_engine.sync_knowledge_to_siyuan(false).await });

    tokio::time::timeout(Duration::from_secs(2), first_request)
        .await
        .expect("knowledge sync never reached the slow SiYuan request")
        .expect("mock server dropped first-request signal");

    let raw = tokio::time::timeout(
        Duration::from_millis(400),
        engine.sync(SyncOptions {
            dry_run: true,
            ..Default::default()
        }),
    )
    .await;

    knowledge.abort();
    server.abort();

    assert!(
        raw.is_ok(),
        "raw sync was serialized behind slow knowledge network I/O"
    );
    raw.unwrap().expect("raw sync should succeed");
}

#[tokio::test]
async fn concurrent_knowledge_syncs_still_create_only_one_remote_document() {
    let _env = env_lock().lock().await;
    let dir = tempfile::tempdir().unwrap();
    let _data_dir = DataDirGuard::set(dir.path());
    let config_path = write_config(dir.path());
    let (url, seen, _first_request, server) = mock_siyuan(Duration::ZERO).await;
    let engine = make_engine(config_path, url);
    seed_knowledge(dir.path());

    let left_engine = Arc::clone(&engine);
    let right_engine = Arc::clone(&engine);
    let (left, right) = tokio::join!(
        async move { left_engine.sync_knowledge_to_siyuan(false).await },
        async move { right_engine.sync_knowledge_to_siyuan(false).await },
    );

    let left = left.expect("first knowledge sync failed");
    let right = right.expect("second knowledge sync failed");
    server.abort();

    let paths = seen.lock().unwrap();
    let creates = paths
        .iter()
        .filter(|path| path.contains("createDocWithMd"))
        .count();
    assert_eq!(
        creates, 1,
        "concurrent knowledge syncs created duplicate docs: {paths:?}"
    );
    assert_eq!(left.created + right.created, 1);
    assert_eq!(left.failed + right.failed, 0);
}
