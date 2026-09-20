use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use aiks_core::sink::SiYuanSink;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn spawn_sequence_server(responses: Vec<&'static str>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let requests_bg = Arc::clone(&requests);

    tokio::spawn(async move {
        for body in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16 * 1024];
            let _ = socket.read(&mut buf).await.unwrap();
            requests_bg.fetch_add(1, Ordering::SeqCst);

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
        }
    });

    (format!("http://{}", addr), requests)
}

#[tokio::test]
async fn create_reconciled_adopts_existing_document_before_creating() {
    let path = "/10 AI Sessions/OpenCode/2026/03/session [ses_30a9]";
    let body = r#"{"code":0,"msg":"","data":[{"id":"doc-existing"}]}"#;
    let (base_url, requests) = spawn_sequence_server(vec![body]).await;
    let sink = SiYuanSink::embedded(base_url, "AI Knowledge").unwrap();

    let doc_id = sink
        .create_document_reconciled("box-1", path, "# payload")
        .await
        .unwrap();

    assert_eq!(doc_id, "doc-existing");
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn create_reconciled_recovers_document_after_ambiguous_create_failure() {
    let path = "/10 AI Sessions/OpenCode/2026/03/session [ses_30a9]";
    let empty = r#"{"code":0,"msg":"","data":[]}"#;
    let failed_create = r#"{"code":-1,"msg":"create timed out","data":null}"#;
    let created = r#"{"code":0,"msg":"","data":[{"id":"doc-created-server-side"}]}"#;
    let (base_url, requests) = spawn_sequence_server(vec![empty, failed_create, created]).await;
    let sink = SiYuanSink::embedded(base_url, "AI Knowledge").unwrap();

    let doc_id = sink
        .create_document_reconciled("box-1", path, "# huge payload")
        .await
        .unwrap();

    assert_eq!(doc_id, "doc-created-server-side");
    assert_eq!(requests.load(Ordering::SeqCst), 3);
}
