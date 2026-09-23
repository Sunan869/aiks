//! Actual binary bootstrap with synthetic environment secrets, not global env.
use std::{process::Stdio, time::Duration};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::test]
async fn model_keys_are_resolved_in_the_service_and_never_returned_in_capabilities() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("business.db");
    let config_path = root.path().join("service.toml");
    std::fs::write(
        &config_path,
        format!(
            "database={}\n[ai]\nenabled=true\nbase_url='http://127.0.0.1:1/v1'\nmodel='synthetic-text'\n[embedding]\nenabled=true\nbase_url='http://127.0.0.1:2/v1'\nmodel='synthetic-vector'\ndimensions=3\n[model_credentials]\nai_api_key_env='AIKS_TEST_AI_KEY'\nembedding_api_key_env='AIKS_TEST_VECTOR_KEY'\n",
            serde_json::to_string(&database).unwrap()
        ),
    )
    .unwrap();
    let token = "ef".repeat(32);
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_aiks-service"))
        .arg("--config")
        .arg(config_path)
        .arg("--bootstrap-stdin")
        .env("AIKS_TEST_AI_KEY", "synthetic-ai-private")
        .env("AIKS_TEST_VECTOR_KEY", "synthetic-vector-private")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(
            format!("{}\n", json!({"token":token,"owner_control":true})).as_bytes(),
        )
        .await
        .unwrap();
    let mut line = String::new();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    tokio::time::timeout(Duration::from_secs(10), stdout.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    let ready: Value = serde_json::from_str(&line).expect("real Service ready handshake");
    let response = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
        .get(format!(
            "http://{}/api/v1/capabilities",
            ready["address"].as_str().unwrap()
        ))
        .bearer_auth(&token)
        .header("X-AIKS-Instance-ID", ready["instance_id"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let text = response.text().await.unwrap();
    let caps: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(caps["ai_assist"], true);
    assert_eq!(caps["semantic_search"], true);
    assert_eq!(caps["team"], false);
    for secret in ["synthetic-ai-private", "synthetic-vector-private", &token] {
        assert!(!line.contains(secret));
        assert!(!text.contains(secret));
    }
    assert!(!text.contains("api_key"));
    assert!(!text.contains("model_credentials"));
    stdin
        .write_all(
            format!(
                "{}\n",
                json!({"command":"shutdown","boot_nonce":ready["boot_nonce"]})
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    drop(stdin);
    let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success());
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(!errors.contains("synthetic-ai-private"));
    assert!(!errors.contains("synthetic-vector-private"));
}
