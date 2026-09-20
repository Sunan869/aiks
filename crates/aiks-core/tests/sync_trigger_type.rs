use std::sync::Arc;

use aiks_core::{
    config::Config,
    providers::ProviderRegistry,
    sink::SiYuanSink,
    storage::{StateDb, SyncRunRepo},
    sync::{SyncEngine, SyncOptions},
};

async fn mock_siyuan_notebook() -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let mut request = [0u8; 4096];
        let _ = socket.read(&mut request).await;
        let body = serde_json::json!({
            "code": 0,
            "msg": "",
            "data": {"notebooks": [{"id": "box-1", "name": "AI Knowledge"}]}
        })
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        );
        let _ = socket.write_all(response.as_bytes()).await;
    });
    (base_url, task)
}

#[tokio::test]
async fn sync_run_records_explicit_trigger_type() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let registry = ProviderRegistry::new(vec![]);
    let (base_url, server) = mock_siyuan_notebook().await;
    let sink = SiYuanSink::embedded(base_url, "AI Knowledge").unwrap();
    let engine = SyncEngine::new(Arc::new(Config::default()));

    let opts = SyncOptions {
        trigger_type: Some("startup".to_string()),
        ..Default::default()
    };
    engine.run_sync(&db, &registry, &sink, &opts).await.unwrap();
    server.abort();

    let run = SyncRunRepo::new(&db).last_run().unwrap().unwrap();
    assert_eq!(run.trigger_type.as_deref(), Some("startup"));
}
