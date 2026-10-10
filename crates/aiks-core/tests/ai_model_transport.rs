//! End-to-end HTTP contract tests for the local OpenAI-compatible AI client.
//! Uses a loopback scripted server; no installed model or network account needed.

use aiks_core::ai::{AiClient, AiModelConfig};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

fn scripted_server(statuses: &[u16]) -> (String, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let scripted = statuses.to_vec();
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        for status in scripted {
            let (mut connection, _) = listener.accept().unwrap();
            connection
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut raw = Vec::new();
            let mut chunk = [0u8; 8192];
            let (header_end, body_len) = loop {
                let n = connection.read(&mut chunk).unwrap();
                assert!(n > 0, "client closed before sending request");
                raw.extend_from_slice(&chunk[..n]);
                assert!(raw.len() < 128 * 1024, "unexpectedly large mock request");
                if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header_end = end + 4;
                    let header = String::from_utf8_lossy(&raw[..header_end]);
                    let body_len = header
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                        .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                        .expect("client request must have Content-Length");
                    if raw.len() >= header_end + body_len {
                        break (header_end, body_len);
                    }
                }
            };
            let request: Value =
                serde_json::from_slice(&raw[header_end..header_end + body_len]).unwrap();
            requests.push(request);

            let (reason, body) = match status {
                200 => (
                    "OK",
                    r#"{"choices":[{"message":{"content":"Recovered answer"}}],"usage":{"prompt_tokens":11,"completion_tokens":2,"total_tokens":13}}"#,
                ),
                401 => ("Unauthorized", r#"{"error":{"message":"bad credentials"}}"#),
                429 => ("Too Many Requests", r#"{"error":{"message":"busy"}}"#),
                503 => (
                    "Service Unavailable",
                    r#"{"error":{"message":"overloaded"}}"#,
                ),
                _ => panic!("unsupported scripted status: {status}"),
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            connection.write_all(response.as_bytes()).unwrap();
        }
        requests
    });
    (url, server)
}

fn client(url: String) -> AiClient {
    let config = AiModelConfig {
        enabled: true,
        base_url: url,
        model: "fixture-model".to_string(),
        timeout_seconds: 10,
        ..AiModelConfig::default()
    };
    AiClient::new(config).unwrap()
}

#[tokio::test]
async fn overloaded_completion_recovers_after_two_bounded_retries() {
    let (url, server) = scripted_server(&[429, 503, 200]);
    let result = client(url)
        .chat_detailed("system", "question")
        .await
        .unwrap();
    assert_eq!(result.content, "Recovered answer");
    let usage = result.usage.unwrap();
    assert_eq!(usage.prompt_tokens, Some(11));
    assert_eq!(usage.completion_tokens, Some(2));
    assert_eq!(usage.total_tokens, Some(13));

    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    for request in requests {
        assert_eq!(request["model"], "fixture-model");
        assert_eq!(request["messages"][1]["content"], "question");
        assert!(request.get("stream").is_none());
    }
}

#[tokio::test]
async fn invalid_credentials_fail_without_retrying() {
    let (url, server) = scripted_server(&[401]);
    let error = client(url).chat("system", "question").await.unwrap_err();
    assert!(error.to_string().contains("401"));
    assert_eq!(server.join().unwrap().len(), 1);
}

#[tokio::test]
async fn streaming_json_fallback_recovers_from_503_without_duplicate_text() {
    let (url, server) = scripted_server(&[503, 200]);
    let mut deltas = Vec::<String>::new();
    let answer = client(url)
        .chat_stream_with_max_tokens("system", "question", 200, |delta| {
            deltas.push(delta.to_string());
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(answer, "Recovered answer");
    assert_eq!(deltas, ["Recovered answer"]);
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| request["stream"] == true));
}
