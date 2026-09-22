#[cfg(unix)]
use serde_json::{json, Value};
use std::{process::Stdio, time::Duration};
#[cfg(unix)]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

#[tokio::test]
async fn binary_rejects_team_and_nonloopback_without_opening_the_database() {
    for extra in ["mode='team'", "listen='0.0.0.0:8080'"] {
        let root = tempfile::tempdir().unwrap();
        let database = root.path().join("must-not-exist.db");
        let config = root.path().join("service.toml");
        std::fs::write(
            &config,
            format!(
                "database={}\n{extra}\n",
                serde_json::to_string(&database).unwrap()
            ),
        )
        .unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_aiks-service"))
            .args(["--config", config.to_str().unwrap(), "--bootstrap-stdin"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let output = tokio::time::timeout(Duration::from_secs(5), child.wait_with_output())
            .await
            .unwrap()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!database.exists());
        assert_eq!(
            String::from_utf8(output.stderr).unwrap().trim(),
            "AIKS service startup or shutdown failed"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn graceful_stop_releases_writer_and_startup_does_not_create_provider_directories() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("empty-home");
    std::fs::create_dir(&home).unwrap();
    let database = root.path().join("state.db");
    let config = root.path().join("service.toml");
    std::fs::write(
        &config,
        format!("database={}\n", serde_json::to_string(&database).unwrap()),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_aiks-service"))
        .args(["--config", config.to_str().unwrap(), "--bootstrap-stdin"])
        .env("HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(format!("{}\n", json!({"token":"ef".repeat(32)})).as_bytes())
        .await
        .unwrap();
    drop(stdin);
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(5), stdout.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    let ready: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(ready["api_version"], 1);
    assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
    assert!(aiks_core::storage::StateDb::open_exclusive(&database).is_err());
    let signal = Command::new("kill")
        .args(["-TERM", &child.id().unwrap().to_string()])
        .status()
        .await
        .unwrap();
    assert!(signal.success());
    let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success());
    assert!(aiks_core::storage::StateDb::open_exclusive(&database).is_ok());
    assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
}
