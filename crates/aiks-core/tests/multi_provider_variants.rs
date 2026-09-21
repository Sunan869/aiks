use std::path::Path;
use aiks_core::config::ExternalProviderConfig;
use aiks_core::model::{hash::compute_session_hash, ContentBlock, NormalizedSession, SourceKind};
use aiks_core::providers::{native::NativeProvider, ProviderHealth, SessionProvider};
use serde_json::{json, Value};

fn put(root: &Path, relative: &str, value: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, value).unwrap();
}
fn provider(source: SourceKind, root: &Path) -> NativeProvider {
    NativeProvider::new(source, &ExternalProviderConfig { path: root.to_string_lossy().into_owned(), ..Default::default() }).unwrap()
}
async fn one(p: &NativeProvider) -> NormalizedSession {
    let found = p.discover_sessions().await.unwrap(); assert_eq!(found.len(), 1);
    p.load_session(&found[0]).await.unwrap()
}
fn texts(s: &NormalizedSession) -> String {
    s.messages.iter().flat_map(|m| &m.blocks).filter_map(ContentBlock::text_content).collect::<Vec<_>>().join("\n")
}
fn continue_session(text: &str) -> String {
    json!({"sessionId":"same-id","title":text,"history":[{"message":{"role":"user","content":text}},{"message":{"role":"assistant","content":"answer"}}]}).to_string()
}

#[tokio::test]
async fn namespaces_are_stable_when_roots_are_reordered_or_removed() {
    let a = tempfile::tempdir().unwrap(); let b = tempfile::tempdir().unwrap();
    put(a.path(), "sessions/s.json", &continue_session("A")); put(b.path(), "sessions/s.json", &continue_session("B"));
    let config = ExternalProviderConfig { paths: vec![a.path().to_string_lossy().into_owned(), b.path().to_string_lossy().into_owned(), a.path().to_string_lossy().into_owned()], ..Default::default() };
    let p = NativeProvider::new(SourceKind::Continue, &config).unwrap();
    let found = p.discover_sessions().await.unwrap(); assert_eq!(found.len(), 2); assert_ne!(found[0].external_session_id, found[1].external_session_id);
    let single = one(&provider(SourceKind::Continue, a.path())).await;
    assert!(found.iter().any(|s| s.external_session_id == single.external_session_id));
    let mut reverse = config; reverse.paths.reverse();
    let mut ids: Vec<_> = found.iter().map(|s| s.external_session_id.clone()).collect(); ids.sort();
    let mut other: Vec<_> = NativeProvider::new(SourceKind::Continue, &reverse).unwrap().discover_sessions().await.unwrap().into_iter().map(|s| s.external_session_id).collect(); other.sort();
    assert_eq!(ids, other);
}
#[tokio::test]
async fn corrupt_neighbor_is_diagnostic_not_a_successful_empty_scan() {
    let root = tempfile::tempdir().unwrap(); put(root.path(), "sessions/good.json", &continue_session("valid")); put(root.path(), "sessions/bad.json", "{broken");
    let p = provider(SourceKind::Continue, root.path());
    let report = p.discover_report().await.unwrap(); assert!(!report.complete); assert_eq!(report.sessions.len(), 1);
    assert!(!report.diagnostics.is_empty()); assert!(p.discover_sessions().await.is_err());
    assert!(texts(&p.load_session(&report.sessions[0]).await.unwrap()).contains("valid"));
}
#[tokio::test]
async fn loading_cannot_read_config_files_or_cross_provider_summaries() {
    let root = tempfile::tempdir().unwrap(); put(root.path(), "sessions/s.json", &continue_session("valid"));
    put(root.path(), "config.json", &continue_session("SYNTHETIC_CREDENTIAL_MARKER"));
    let p = provider(SourceKind::Continue, root.path()); let mut s = p.discover_sessions().await.unwrap().remove(0);
    let real = s.source_path.clone(); s.source_path = Some(root.path().join("config.json"));
    assert!(p.load_session(&s).await.is_err()); s.source_path = real; s.source = SourceKind::QwenCode;
    assert!(p.load_session(&s).await.is_err());
}
#[tokio::test]
async fn missing_aider_roots_are_not_configured_and_empty_continue_is_healthy() {
    let p = NativeProvider::new(SourceKind::Aider, &ExternalProviderConfig::default()).unwrap(); assert!(matches!(p.health_check().await, ProviderHealth::NotConfigured));
    let root = tempfile::tempdir().unwrap(); std::fs::create_dir(root.path().join("sessions")).unwrap();
    let p = provider(SourceKind::Continue, root.path()); assert!(p.health_check().await.is_ok()); assert!(p.discover_sessions().await.unwrap().is_empty());
}
#[tokio::test]
async fn antigravity_usage_only_never_produces_conversations() {
    let root = tempfile::tempdir().unwrap(); put(root.path(), ".token-monitor/rpc-cache/v1/s1/usage.jsonl", "{\"recordType\":\"usage\",\"inputTokens\":12}\n");
    let p = provider(SourceKind::Antigravity, root.path()); assert!(matches!(p.health_check().await, ProviderHealth::Unsupported { .. })); assert!(p.discover_sessions().await.is_err());
}
#[tokio::test]
async fn kilo_reads_its_exact_editor_index_and_does_not_modify_database() {
    let root = tempfile::tempdir().unwrap(); let base = "globalStorage/kilocode.kilo-code";
    put(root.path(), &format!("{base}/tasks/t1/api_conversation_history.json"), r#"[{"role":"user","content":"KILO"},{"role":"assistant","content":"answer"}]"#);
    let db = root.path().join("globalStorage/state.vscdb"); let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TABLE ItemTable(key TEXT PRIMARY KEY, value BLOB);").unwrap();
    conn.execute("INSERT INTO ItemTable VALUES(?1, ?2)", rusqlite::params!["kilocode.kilo-code", json!({"taskHistory":[{"id":"t1","task":"Indexed Kilo","workspace":"/example/kilo"}]}).to_string()]).unwrap();
    conn.execute("INSERT INTO ItemTable VALUES('unrelated.credentials', 'SYNTHETIC_CREDENTIAL_MARKER')", []).unwrap(); drop(conn);
    let before = std::fs::read(&db).unwrap(); let s = one(&provider(SourceKind::KiloCode, root.path())).await;
    assert_eq!(s.title.as_deref(), Some("Indexed Kilo")); assert_eq!(s.project_path.as_deref(), Some("/example/kilo")); assert!(!serde_json::to_string(&s).unwrap().contains("CREDENTIAL_MARKER")); assert_eq!(before, std::fs::read(db).unwrap());
}
#[tokio::test]
async fn copilot_desktop_tag_does_not_change_session_identity() {
    let root = tempfile::tempdir().unwrap(); put(root.path(), "session-state/s1/events.jsonl", "{\"type\":\"user.message\",\"data\":{\"content\":\"question\"}}\n{\"type\":\"assistant.message\",\"data\":{\"content\":\"answer\"}}\n");
    let p = provider(SourceKind::GithubCopilot, root.path()); let cli = one(&p).await;
    put(root.path(), "session-state/s1/workspace.yaml", "client_name: github/autopilot\n"); let desktop = one(&p).await;
    assert_eq!(cli.external_session_id, desktop.external_session_id); assert_eq!(desktop.metadata["entrypoint"], "copilot-desktop"); assert_eq!(compute_session_hash(&cli), compute_session_hash(&desktop));
}
#[tokio::test]
async fn vscode_replays_set_append_delete_and_rejects_huge_indices() {
    let root = tempfile::tempdir().unwrap(); let path = "workspaceStorage/ws/chatSessions/s1.jsonl";
    let events = [json!({"kind":0,"v":{"sessionId":"s1","requests":[{"message":{"text":"VS_CODE"},"response":[{"value":"old"},{"value":"remove"}]}]}}), json!({"kind":1,"k":["requests",0,"response",0,"value"],"v":"new"}), json!({"kind":2,"k":["requests",0,"response"],"v":[{"value":"tail"}]}), json!({"kind":3,"k":["requests",0,"response",1]})];
    let content = events.iter().map(Value::to_string).collect::<Vec<_>>().join("\n"); put(root.path(), path, &content);
    let p = provider(SourceKind::GithubCopilot, root.path()); assert_eq!(texts(&one(&p).await), "VS_CODE\nnew\ntail");
    put(root.path(), path, &format!("{content}\n{}", json!({"kind":1,"k":["requests",999999999],"v":null})));
    assert!(p.discover_sessions().await.is_err());
}
fn kimi(root: &Path, events: &[Value]) -> NativeProvider {
    put(root, "sessions/wd_x/session_s/state.json", r#"{"id":"s","version":2}"#);
    put(root, "sessions/wd_x/session_s/agents/main/wire.jsonl", &events.iter().map(Value::to_string).collect::<Vec<_>>().join("\n"));
    provider(SourceKind::KimiCode, root)
}
fn prompt(id: &str, text: &str) -> Value { json!({"type":"context.append_message","message":{"id":id,"role":"user","content":[{"type":"text","text":text}]}}) }
#[tokio::test]
async fn kimi_undo_and_legacy_compaction_keep_the_recorded_suffix() {
    let root = tempfile::tempdir().unwrap();
    let p = kimi(root.path(), &[prompt("u1","old"), prompt("u2","tail"), prompt("u3","retracted"), json!({"type":"context.undo","count":1}), json!({"type":"context.apply_compaction","compactedCount":1,"summary":"summary"})]);
    assert_eq!(texts(&one(&p).await), "summary\ntail");
    let p = kimi(root.path(), &[prompt("u1","old"), json!({"type":"context.clear"}), prompt("u2","new")]); assert_eq!(texts(&one(&p).await), "new");
}
#[tokio::test]
async fn kimi_legacy_context_and_unfinished_tool_steps_are_honest() {
    let root = tempfile::tempdir().unwrap(); put(root.path(), "sessions/p/s/context.jsonl", "{\"role\":\"user\",\"content\":\"legacy\"}\n{\"role\":\"assistant\",\"content\":\"answer\"}\n");
    assert_eq!(texts(&one(&provider(SourceKind::KimiCode, root.path())).await), "legacy\nanswer");
    let modern = tempfile::tempdir().unwrap();
    let p = kimi(modern.path(), &[prompt("u1","question"), json!({"type":"context.append_loop_event","event":{"type":"step.begin"}}), json!({"type":"context.append_loop_event","event":{"type":"tool.call","toolCallId":"t1","name":"read_file","args":{}}})]);
    let s = one(&p).await; assert!(s.messages.iter().all(|m| m.blocks.iter().all(|b| !matches!(b, ContentBlock::ToolResult { .. })))); assert_eq!(s.messages.last().unwrap().metadata["interrupted_tool_calls"], json!(["t1"]));
}
#[tokio::test]
async fn cursor_global_headers_wal_rename_and_workspace_fallback() {
    let root = tempfile::tempdir().unwrap(); std::fs::create_dir(root.path().join("globalStorage")).unwrap();
    let db = root.path().join("globalStorage/state.vscdb"); let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE cursorDiskKV(key TEXT PRIMARY KEY,value TEXT);").unwrap();
    for (key, value) in [("composerData:s1",json!({"composerId":"s1","name":"before","fullConversationHeadersOnly":[{"bubbleId":"u","type":1},{"bubbleId":"a","type":2}]})), ("bubbleId:s1:u",json!({"bubbleId":"u","type":1,"text":"cursor question"})), ("bubbleId:s1:a",json!({"bubbleId":"a","type":2,"text":"cursor answer"}))] { conn.execute("INSERT INTO cursorDiskKV VALUES(?1,?2)",rusqlite::params![key,value.to_string()]).unwrap(); }
    let p = provider(SourceKind::Cursor, root.path()); let a = one(&p).await; assert_eq!(texts(&a), "cursor question\ncursor answer");
    conn.execute("UPDATE cursorDiskKV SET value=json_set(value,'$.name','renamed') WHERE key='composerData:s1'", []).unwrap();
    let b = one(&p).await; assert_eq!(a.external_session_id, b.external_session_id); assert_ne!(compute_session_hash(&a), compute_session_hash(&b));
    put(root.path(), "workspaceStorage/ws/workspace.json", r#"{"folder":"file:///example/p"}"#);
    let ws = rusqlite::Connection::open(root.path().join("workspaceStorage/ws/state.vscdb")).unwrap(); ws.execute_batch("CREATE TABLE ItemTable(key TEXT PRIMARY KEY,value TEXT);").unwrap();
    ws.execute("INSERT INTO ItemTable VALUES('composer.composerData',?1)", [json!({"allComposers":[{"composerId":"s2","conversation":[{"type":1,"text":"legacy"},{"type":2,"text":"answer"}]}]}).to_string()]).unwrap();
    assert_eq!(p.discover_sessions().await.unwrap().len(), 2);
}
