#[path = "../../aiks-desktop/src-tauri/src/service_client/mod.rs"]
pub mod service_client;
#[path = "../../aiks-desktop/src-tauri/src/service_client/supervisor.rs"]
mod supervisor;
use supervisor::ReadyHandshake;
use service_client::ClientError;
use serde_json::json;

#[test]
fn valid_owned_handshake_is_accepted_but_untrusted_identities_are_not() {
    let mut frame = json!({"api_version":1,"instance_id":"instance-a","space_id":"space-a","boot_nonce":uuid::Uuid::new_v4().to_string(),"address":"127.0.0.1:12345"});
    let line = format!("{frame}\n");
    assert_eq!(ReadyHandshake::parse(&line, Some("instance-a")).unwrap().space_id,"space-a");
    assert!(matches!(ReadyHandshake::parse(&line,Some("instance-b")),Err(ClientError::WrongInstance)));
    for bad in ["0.0.0.0:12345","localhost:12345","127.0.0.1:0","[::]:12345"] {
        frame["address"]=json!(bad);
        assert!(ReadyHandshake::parse(&format!("{frame}\n"),None).is_err());
    }
    assert!(ReadyHandshake::parse(&"x".repeat(4097),None).is_err());
}
