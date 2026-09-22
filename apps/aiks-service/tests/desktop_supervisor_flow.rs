//! The same native supervisor used by the GUI, against the real Service binary.
#[path = "../../aiks-desktop/src-tauri/src/service_client/mod.rs"]
pub mod service_client;
use service_client::{supervisor::OwnedService,ClientError};
use aiks_core::storage::StateDb;
use std::{path::Path,time::Duration,process::Stdio};
use tokio::io::{AsyncBufReadExt,AsyncWriteExt,BufReader};
use serde_json::{json,Value};

#[tokio::test]
async fn owned_service_restarts_with_same_identity_and_cannot_stop_a_foreign_writer() {
    let root=tempfile::tempdir().unwrap();
    let database=root.path().join("state.db");
    let config=root.path().join("runtime.toml");
    std::fs::write(&config,format!("database={}\n",serde_json::to_string(&database).unwrap())).unwrap();
    let binary=Path::new(env!("CARGO_BIN_EXE_aiks-service"));
    let mut owner=OwnedService::start(binary,&config,None).await.unwrap();
    let client=owner.client();
    let identity=client.connection().instance_id().to_owned();
    assert!(StateDb::open_exclusive(&database).is_err());
    assert!(OwnedService::start(binary,&config,None).await.is_err());
    assert_eq!(client.capabilities().await.unwrap()["instance_id"],identity);
    assert!(!owner.shutdown().await.unwrap());
    drop(owner);
    drop(StateDb::open_exclusive(&database).unwrap());
    let wrong=OwnedService::start(binary,&config,Some("another-instance")).await;
    assert!(matches!(wrong,Err(ClientError::WrongInstance)));
    drop(StateDb::open_exclusive(&database).unwrap());
    let mut restarted=OwnedService::start(binary,&config,Some(&identity)).await.unwrap();
    assert_eq!(restarted.client().capabilities().await.unwrap()["instance_id"],identity);
    assert!(!restarted.shutdown().await.unwrap());
}

#[tokio::test]
async fn explicitly_managed_parent_pipe_eof_drains_but_independent_mode_stays_alive() {
    for managed in [true,false] {
        let root=tempfile::tempdir().unwrap();
        let database=root.path().join("state.db");
        let config=root.path().join("runtime.toml");
        std::fs::write(&config,format!("database={}\n",serde_json::to_string(&database).unwrap())).unwrap();
        let mut child=tokio::process::Command::new(env!("CARGO_BIN_EXE_aiks-service"))
            .arg("--config").arg(config).arg("--bootstrap-stdin")
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).spawn().unwrap();
        let mut pipe=child.stdin.take().unwrap();
        pipe.write_all(format!("{}\n",json!({"token":"ab".repeat(32),"owner_control":managed,"owner_lifetime":managed})).as_bytes()).await.unwrap();
        let mut output=BufReader::new(child.stdout.take().unwrap());
        let mut line=String::new();
        tokio::time::timeout(Duration::from_secs(10),output.read_line(&mut line)).await.unwrap().unwrap();
        let ready:Value=serde_json::from_str(&line).unwrap();
        assert!(ready["address"].is_string());
        drop(pipe);
        if managed {
            assert!(tokio::time::timeout(Duration::from_secs(12),child.wait()).await.unwrap().unwrap().success());
        } else {
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(child.try_wait().unwrap().is_none());
            child.kill().await.unwrap();
        }
        drop(StateDb::open_exclusive(&database).unwrap());
    }
}
