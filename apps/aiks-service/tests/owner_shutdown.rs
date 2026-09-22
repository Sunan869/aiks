use aiks_core::storage::StateDb;
use serde_json::{json, Value};
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::test]
async fn owned_pipe_shutdown_is_explicit_and_releases_the_writer_on_all_platforms() {
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("state.db");
    let config = root.path().join("service.toml");
    std::fs::write(
        &config,
        format!("database={}\n", serde_json::to_string(&db).unwrap()),
    )
    .unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_aiks-service"))
        .arg("--config")
        .arg(&config)
        .arg("--bootstrap-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(
            format!(
                "{}\n",
                json!({"token":"ab".repeat(32),"owner_control":true})
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(10), stdout.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    let ready: Value =
        serde_json::from_str(&line).expect("owned service must produce a ready handshake");
    assert!(StateDb::open_exclusive(&db).is_err());
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
    let exit = tokio::time::timeout(Duration::from_secs(12), child.wait())
        .await
        .expect("explicit owner shutdown must not require a forced kill")
        .unwrap();
    assert!(exit.success());
    assert!(StateDb::open_exclusive(&db).is_ok());
}
