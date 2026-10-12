//! Shared bounded reader for untrusted OpenAI-compatible JSON and error bodies.
//! The cap is applied while receiving chunks, before constructing a String
//! or deserializing JSON. reqwest may still allocate one chunk internally.
use anyhow::Context;

#[derive(Default)]
struct BoundedHttpBody {
    bytes: Vec<u8>,
}

impl BoundedHttpBody {
    fn append(&mut self, chunk: &[u8], limit: usize) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.bytes
                .len()
                .checked_add(chunk.len())
                .is_some_and(|total| total <= limit),
            "HTTP response exceeded the safe body-size limit of {limit} bytes"
        );
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }

    fn into_text(self) -> anyhow::Result<String> {
        String::from_utf8(self.bytes).context("HTTP response body is not valid UTF-8")
    }
}

/// Download no more than max_bytes from an HTTP response. Applies to decoded
/// chunks regardless of Content-Length or server-side transfer encoding.
/// The request timeout remains the caller's responsibility.
pub async fn read_bounded_http_text(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> anyhow::Result<String> {
    if let Some(length) = response.content_length() {
        anyhow::ensure!(
            length <= max_bytes as u64,
            "HTTP response Content-Length exceeds the safe body-size limit of {max_bytes} bytes"
        );
    }
    let mut body = BoundedHttpBody::default();
    while let Some(chunk) = response.chunk().await.context("read HTTP response chunk")? {
        body.append(&chunk, max_bytes)?;
    }
    body.into_text()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_accepts_exact_budget_and_multi_chunk_utf8() {
        let mut body = BoundedHttpBody::default();
        body.append(&"中".as_bytes()[..1], 6).unwrap();
        body.append(&"中".as_bytes()[1..], 6).unwrap();
        body.append("文".as_bytes(), 6).unwrap();
        assert_eq!(body.into_text().unwrap(), "中文");
        let mut body = BoundedHttpBody::default();
        body.append(b"ok", 2).unwrap();
        assert_eq!(body.into_text().unwrap(), "ok");
    }

    #[test]
    fn bounded_reader_rejects_oversize_without_accepting_partial_chunk() {
        let mut body = BoundedHttpBody::default();
        body.append(b"ok", 3).unwrap();
        assert!(body.append(b"extra", 3).is_err());
        assert_eq!(body.into_text().unwrap(), "ok");
        let mut body = BoundedHttpBody::default();
        assert!(body.append(b"x", 0).is_err());
        assert!(body.append(b"x", usize::MAX).is_ok());
    }

    #[tokio::test]
    async fn chunked_http_without_content_length_is_bounded() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let bind = tokio::net::TcpListener::bind("127.0.0.1:0");
        let listener = bind.await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await;
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n3\r\ndef\r\n0\r\n\r\n",
                )
                .await
                .unwrap();
        });
        let get = reqwest::get(format!("http://{address}/"));
        let response = get.await.unwrap();
        assert!(read_bounded_http_text(response, 5).await.is_err());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn content_length_is_rejected_before_reading_a_large_body() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let bind = tokio::net::TcpListener::bind("127.0.0.1:0");
        let listener = bind.await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await;
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        });
        let get = reqwest::get(format!("http://{address}/"));
        let response = get.await.unwrap();
        let error = read_bounded_http_text(response, 5).await.unwrap_err();
        assert!(error.to_string().contains("Content-Length"));
        server.await.unwrap();
    }

    #[test]
    fn bounded_reader_does_not_silently_replace_invalid_utf8() {
        let mut body = BoundedHttpBody::default();
        body.append(&[0xE4, 0xB8], 10).unwrap();
        assert!(body.into_text().is_err());
    }
}
