//! Contract tests for OpenAI-compatible embedding responses.
//! These use a one-shot loopback gateway and never call a real model service.

use aiks_core::pipeline::embedding_client::EmbeddingClient;
use aiks_core::EmbeddingConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn embed_from_mock(
    status: &str,
    response_body: &str,
    content_length_override: Option<usize>,
    dimensions: Option<usize>,
    input_count: usize,
) -> anyhow::Result<Vec<Vec<f32>>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base_url = format!("http://{}/v1", listener.local_addr()?);
    let wire_response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
        content_length_override.unwrap_or(response_body.len())
    );
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0_u8; 4096];
        let _ = stream.read(&mut buf).await;
        stream.write_all(wire_response.as_bytes()).await.unwrap();
    });
    let client = EmbeddingClient::new(EmbeddingConfig {
        enabled: true,
        base_url,
        dimensions,
        ..EmbeddingConfig::default()
    })?;
    let result = client
        .embed_batch((0..input_count).map(|n| format!("text {n}")).collect())
        .await;
    server.await?;
    result
}

#[tokio::test]
async fn reordered_indexes_preserve_input_order() {
    let body = r#"{"data":[{"index":1,"embedding":[0.0,1.0]},{"index":0,"embedding":[1.0,0.0]}]}"#;
    let vectors = embed_from_mock("200 OK", body, None, Some(2), 2)
        .await
        .unwrap();
    assert_eq!(vectors, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
}

#[tokio::test]
async fn incomplete_ambiguous_and_out_of_range_indexes_are_errors() {
    let replies = [
        r#"{"data":[{"index":0,"embedding":[1.0,0.0]}]}"#,
        r#"{"data":[{"index":0,"embedding":[1.0,0.0]},{"index":0,"embedding":[0.0,1.0]}]}"#,
        r#"{"data":[{"index":0,"embedding":[1.0,0.0]},{"index":2,"embedding":[0.0,1.0]}]}"#,
    ];
    for body in replies {
        assert!(embed_from_mock("200 OK", body, None, Some(2), 2)
            .await
            .is_err());
    }
}

#[tokio::test]
async fn invalid_dimensions_and_non_normalizable_vectors_are_errors() {
    let invalid = [
        (r#"{"data":[{"index":0,"embedding":[1.0,2.0]}]}"#, Some(3)),
        (r#"{"data":[{"index":0,"embedding":[]}]}"#, None),
        (r#"{"data":[{"index":0,"embedding":[0.0,0.0]}]}"#, None),
        (r#"{"data":[{"index":0,"embedding":[1e100,1.0]}]}"#, None),
        (
            r#"{"data":[{"index":0,"embedding":[1.0,2.0]},{"index":1,"embedding":[1.0,2.0,3.0]}]}"#,
            None,
        ),
    ];
    for (body, dimensions) in invalid {
        let count = if body.contains(r#""index":1"#) { 2 } else { 1 };
        assert!(embed_from_mock("200 OK", body, None, dimensions, count)
            .await
            .is_err());
    }
}

#[tokio::test]
async fn oversized_declared_body_is_rejected_before_json_read() {
    let error = embed_from_mock("200 OK", "", Some(16 * 1024 * 1024 + 1), Some(2), 1)
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("Content-Length"));
}

#[tokio::test]
async fn unicode_error_body_is_truncated_without_panicking() {
    let body = format!("{}中{}", "x".repeat(199), "文".repeat(20));
    let error = embed_from_mock("429 Too Many Requests", &body, None, Some(2), 1)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Embedding API error 429"));
}

#[test]
fn invalid_client_configuration_fails_fast() {
    let mut config = EmbeddingConfig {
        batch_size: 0,
        ..EmbeddingConfig::default()
    };
    assert!(EmbeddingClient::new(config.clone()).is_err());
    config.batch_size = 16;
    config.dimensions = Some(0);
    assert!(EmbeddingClient::new(config.clone()).is_err());
    config.dimensions = Some(16_385);
    assert!(EmbeddingClient::new(config).is_err());
}
