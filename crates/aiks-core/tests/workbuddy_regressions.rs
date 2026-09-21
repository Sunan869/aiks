use aiks_core::model::ContentBlock;
use aiks_core::providers::{workbuddy::WorkBuddyProvider, SessionProvider};
use rusqlite::Connection;
use serde_json::json;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("projects")).unwrap();
    let conn = Connection::open(root.path().join("workbuddy.db")).unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions (
            id TEXT PRIMARY KEY, cwd TEXT NOT NULL, title TEXT, custom_title TEXT,
            created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
            deleted_at INTEGER, last_activity_at INTEGER
        );
        INSERT INTO sessions VALUES ('s1', '/example/project', 'Title', NULL,
                                     1000, 5000, NULL, 2000);",
    )
    .unwrap();
    root
}

fn write_events(root: &TempDir, events: &[serde_json::Value]) -> std::path::PathBuf {
    let path = root.path().join("projects").join("session.jsonl");
    let content = events
        .iter()
        .map(serde_json::Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, content).unwrap();
    path
}

fn message(session_id: &str) -> serde_json::Value {
    json!({"id": "u1", "type": "message", "role": "user",
           "sessionId": session_id, "content": "Synthetic question"})
}

fn provider(root: &TempDir) -> WorkBuddyProvider {
    WorkBuddyProvider::new(Some(root.path().to_path_buf())).unwrap()
}

#[tokio::test]
async fn metadata_update_is_not_hidden_by_older_activity() {
    let root = fixture();
    write_events(&root, &[message("s1")]);
    let provider = provider(&root);
    let summaries = provider.discover_sessions().await.unwrap();
    assert_eq!(summaries[0].updated_at.unwrap().timestamp_millis(), 5000);
    let session = provider.load_session(&summaries[0]).await.unwrap();
    assert_eq!(session.updated_at.unwrap().timestamp_millis(), 5000);
}

#[tokio::test]
async fn discovery_orders_by_latest_metadata_or_activity_update() {
    let root = fixture();
    let conn = Connection::open(root.path().join("workbuddy.db")).unwrap();
    conn.execute_batch(
        "INSERT INTO sessions VALUES ('s2', '/example/other', 'Other', NULL,
                                     1000, 4000, NULL, 3000);",
    )
    .unwrap();
    let summaries = provider(&root).discover_sessions().await.unwrap();
    assert_eq!(summaries[0].external_session_id, "s1");
}

#[tokio::test]
async fn structured_reasoning_is_preserved_instead_of_becoming_empty_text() {
    let root = fixture();
    let event = json!({"id": "r1", "type": "reasoning", "sessionId": "s1",
                       "content": {"summary": "Unrecognized structured reasoning"}});
    write_events(&root, std::slice::from_ref(&event));
    let provider = provider(&root);
    let summaries = provider.discover_sessions().await.unwrap();
    let session = provider.load_session(&summaries[0]).await.unwrap();
    match session.messages[0].blocks.as_slice() {
        [ContentBlock::Unknown { raw }] => assert_eq!(raw, &event),
        blocks => panic!("unsupported reasoning was lost: {blocks:?}"),
    }
}

#[tokio::test]
async fn null_camel_case_session_id_does_not_mask_valid_snake_case_id() {
    let root = fixture();
    let mut event = message("s1");
    event["sessionId"] = serde_json::Value::Null;
    event["session_id"] = json!("s1");
    write_events(&root, &[event]);
    let provider = provider(&root);
    let summaries = provider.discover_sessions().await.unwrap();
    let session = provider.load_session(&summaries[0]).await.unwrap();
    assert_eq!(session.messages.len(), 1);
}

#[tokio::test]
async fn a_transcript_for_another_session_is_an_error_not_an_empty_session() {
    let root = fixture();
    let path = write_events(&root, &[message("other-session")]);
    let provider = provider(&root);
    let mut summaries = provider.discover_sessions().await.unwrap();
    summaries[0].source_path = Some(path);
    assert!(provider.load_session(&summaries[0]).await.is_err());
}

#[tokio::test]
async fn loading_a_summary_cannot_read_outside_projects() {
    let root = fixture();
    let forbidden_dir = root.path().join("file-history");
    std::fs::create_dir(&forbidden_dir).unwrap();
    let forbidden = forbidden_dir.join("synthetic.jsonl");
    std::fs::write(&forbidden, message("s1").to_string()).unwrap();
    let provider = provider(&root);
    let mut summaries = provider.discover_sessions().await.unwrap();
    summaries[0].source_path = Some(forbidden);
    assert!(provider.load_session(&summaries[0]).await.is_err());
}

#[tokio::test]
async fn malformed_lines_are_reported_without_copying_their_content() {
    let root = fixture();
    let path = write_events(&root, &[message("s1")]);
    let valid = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{valid}\nsynthetic-invalid-json\n")).unwrap();
    let provider = provider(&root);
    let summaries = provider.discover_sessions().await.unwrap();
    let session = provider.load_session(&summaries[0]).await.unwrap();
    assert_eq!(session.messages.len(), 1);
    let warnings = session.metadata.get("parse_warnings").unwrap();
    assert_eq!(warnings["malformed_lines"], 1);
    assert!(!warnings.to_string().contains("synthetic-invalid-json"));
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_projects_root_is_not_scanned() {
    let root = fixture();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("synthetic.jsonl"), message("s1").to_string()).unwrap();
    std::fs::remove_dir(root.path().join("projects")).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("projects")).unwrap();
    let provider = provider(&root);
    let summaries = provider.discover_sessions().await.unwrap();
    assert!(summaries[0].source_path.is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_transcript_cannot_escape_projects_on_load() {
    let root = fixture();
    let outside = tempfile::tempdir().unwrap();
    let external_path = outside.path().join("synthetic.jsonl");
    std::fs::write(&external_path, message("s1").to_string()).unwrap();
    let link = root.path().join("projects").join("link.jsonl");
    std::os::unix::fs::symlink(&external_path, &link).unwrap();
    let provider = provider(&root);
    let mut summaries = provider.discover_sessions().await.unwrap();
    summaries[0].source_path = Some(link);
    assert!(provider.load_session(&summaries[0]).await.is_err());
}
