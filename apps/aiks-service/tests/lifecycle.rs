use std::{process::Stdio, time::Duration};
use aiks_core::storage::StateDb;
use aiks_service::{LocalAuth, ServiceConfig};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[test]
fn public_listen_and_team_mode_are_rejected_before_opening_a_database() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("must-not-exist.db");
    let mut config = ServiceConfig::personal(path.clone());
    for address in ["0.0.0.0:8080", "192.168.1.2:8080", "[::]:8080", "127.0.0.2:8080"] {
        config.listen = address.parse().unwrap();
        assert!(config.validate().is_err());
        assert!(LocalAuth::new(&"ab".repeat(32), "test-instance").unwrap()
            .with_authority(config.listen).is_err());
    }
    config.listen = "127.0.0.1:0".parse().unwrap();
    config.mode = "team".into();
    assert!(config.validate().is_err());
    assert!(!path.exists());
}

#[tokio::test]
async fn independent_process_survives_stdin_eof_and_restart_keeps_identity() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("business.db");
    let config_path = root.path().join("service.toml");
    std::fs::write(&config_path, format!("database={}\n", serde_json::to_string(&database).unwrap())).unwrap();
    let token = "cd".repeat(32);
    let mut previous = None;
    for _ in 0..2 {
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_aiks-service"))
            .arg("--config").arg(&config_path).arg("--bootstrap-stdin")
            .env("HOME", root.path().join("empty-home"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .kill_on_drop(true).spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(format!("{}\n", json!({"token":token})).as_bytes()).await.unwrap();
        drop(stdin);
        let mut line = String::new();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        tokio::time::timeout(Duration::from_secs(10), stdout.read_line(&mut line)).await.unwrap().unwrap();
        let ready: Value = serde_json::from_str(&line).expect("actual ready handshake");
        assert!(!line.contains(&token));
        assert_eq!(ready["api_version"], 1);
        let identity = ready["instance_id"].as_str().unwrap().to_owned();
        let nonce = ready["boot_nonce"].as_str().unwrap().to_owned();
        if let Some((old_id, old_nonce)) = previous { assert_eq!(identity, old_id); assert_ne!(nonce, old_nonce); }
        previous = Some((identity.clone(), nonce));
        let response = reqwest::Client::builder().no_proxy().build().unwrap()
            .get(format!("http://{}/api/v1/capabilities", ready["address"].as_str().unwrap()))
            .bearer_auth(&token).header("X-AIKS-Instance-ID", identity).send().await.unwrap();
        assert_eq!(response.status(), 200, "stdin EOF must not shut down the service");
        assert!(StateDb::open_exclusive(&database).is_err());
        child.kill().await.unwrap();
        child.wait().await.unwrap();
        assert!(StateDb::open_exclusive(&database).is_ok(), "OS must release the writer lock");
    }
}
