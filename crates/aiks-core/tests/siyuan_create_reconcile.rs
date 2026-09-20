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
async fn create_reconciled_refuses_to_adopt_existing_document_with_different_content() {
    let path = "/10 AI Sessions/OpenCode/2026/03/session [ses_30a9]";
    let existing = r#"{"code":0,"msg":"","data":[{"id":"doc-existing"}]}"#;
    let remote = r##"{"code":0,"msg":"","data":{"kramdown":"# user edited\n"}}"##;
    let (base_url, requests) = spawn_sequence_server(vec![existing, remote]).await;
    let sink = SiYuanSink::embedded(base_url, "AI Knowledge").unwrap();

    let err = sink
        .create_document_reconciled("box-1", path, "# payload")
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("content mismatch"), "unexpected error: {err}");
    assert_eq!(requests.load(Ordering::SeqCst), 2);
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

#[tokio::test]
async fn mapped_document_update_failure_must_not_fall_back_to_create() {
    let update_failed = r#"{"code":-1,"msg":"database busy","data":null}"#;
    let still_exists = r#"{"code":0,"msg":"","data":[{"box":"box-1"}]}"#;
    let (base_url, requests) = spawn_sequence_server(vec![update_failed, still_exists]).await;
    let sink = SiYuanSink::embedded(base_url, "AI Knowledge").unwrap();

    assert!(sink
        .update_document("doc-existing", "# replacement")
        .await
        .is_err());
    let err = sink
        .create_document("box-1", "/10 AI Sessions/existing", "# replacement")
        .await
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("refusing to recreate"),
        "unexpected error: {err}"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn oversized_document_is_rejected_before_any_siyuan_request() {
    let sink = SiYuanSink::embedded("http://127.0.0.1:1", "AI Knowledge").unwrap();
    let payload = "x".repeat(6 * 1024 * 1024);

    let err = sink
        .create_document_reconciled("box-1", "/10 AI Sessions/huge", &payload)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("too large"), "unexpected error: {err}");
}
