use aiks_core::config::Config;
use aiks_core::model::{hash::compute_session_hash, ContentBlock, NormalizedSession, SourceKind};
use aiks_core::providers::build_registry;
use serde_json::json;
use std::path::Path;

fn fixture(files: &[(&str, &str)]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for (relative, text) in files {
        let path = root.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

async fn load(key: &str, root: &Path) -> NormalizedSession {
    let config_text = format!(
        "[providers.{key}]\nenabled=true\npath={}\n",
        serde_json::to_string(&root.to_string_lossy()).unwrap()
    );
    let config: Config = toml::from_str(&config_text).unwrap();
    let kind = SourceKind::from_str(key).unwrap_or_else(|| panic!("missing source: {key}"));
    assert_eq!(kind.as_str(), key);
    let registry = build_registry(&config);
    let provider = registry.get(kind).unwrap_or_else(|| panic!("unregistered source: {key}"));
    assert!(provider.health_check().await.is_ok(), "health: {key}");
    let found = provider.discover_sessions().await.unwrap();
    assert_eq!(found.len(), 1, "discovery: {key}");
    let session = provider.load_session(&found[0]).await.unwrap();
    let second = provider.load_session(&found[0]).await.unwrap();
    assert_eq!(compute_session_hash(&session), compute_session_hash(&second));
    assert_eq!(session.source, kind);
    assert!(session.messages.len() >= 2, "real messages missing: {key}");
    session
}

fn text(session: &NormalizedSession) -> String {
    session.messages.iter().flat_map(|m| &m.blocks)
        .filter_map(ContentBlock::text_content).collect::<Vec<_>>().join("\n")
}

#[tokio::test]
async fn qwen_code_imports_internal_identity_and_parts() {
    let root = fixture(&[("projects/p/chats/arbitrary.jsonl", concat!(
        "{\"uuid\":\"u1\",\"sessionId\":\"s1\",\"type\":\"user\",\"cwd\":\"/example/p\",\"message\":{\"role\":\"user\",\"parts\":[{\"text\":\"QWEN_UNIQUE 问题\"}]}}\n",
        "{\"uuid\":\"a1\",\"parentUuid\":\"u1\",\"sessionId\":\"s1\",\"type\":\"assistant\",\"message\":{\"role\":\"model\",\"parts\":[{\"text\":\"answer\"}]}}\n"
    ))]);
    let s = load("qwen_code", root.path()).await;
    assert_eq!(s.external_session_id, "s1");
    assert!(text(&s).contains("QWEN_UNIQUE"));
    assert_eq!(s.messages[1].parent_id.as_deref(), Some("u1"));
}

#[tokio::test]
async fn continue_imports_history_not_index_or_context_items() {
    let root = fixture(&[
        ("sessions/arbitrary.json", r#"{"sessionId":"s1","title":"Continue title","workspaceDirectory":"C:\\example\\中文","history":[{"message":{"role":"user","content":"CONTINUE_UNIQUE"},"contextItems":[{"content":"context only"}]},{"message":{"role":"assistant","content":[{"type":"text","text":"answer"}]}}]}"#),
        ("sessions/sessions.json", r#"[{"sessionId":"index-only"}]"#),
    ]);
    let s = load("continue", root.path()).await;
    assert_eq!(s.external_session_id, "s1");
    assert_eq!(text(&s), "CONTINUE_UNIQUE\nanswer");
    assert!(s.messages.iter().all(|m| m.created_at.is_none()));
}

#[tokio::test]
async fn cursor_agent_imports_transcripts_and_unwraps_only_user_text() {
    let root = fixture(&[("projects/p/agent-transcripts/s1/s1.jsonl", concat!(
        "{\"role\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"<user_query>CURSOR_AGENT_UNIQUE</user_query><context>noise</context>\"}]}}\n",
        "{\"role\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"answer\"}]}}\n"
    ))]);
    let s = load("cursor_agent", root.path()).await;
    assert_eq!(text(&s), "CURSOR_AGENT_UNIQUE\nanswer");
    assert!(s.messages.iter().all(|m| m.created_at.is_none()));
}

async fn cline_family(key: &str, index: &str) {
    let root = fixture(&[
        ("tasks/t1/api_conversation_history.json", r#"[{"role":"user","content":[{"type":"text","text":"CLINE_FAMILY_UNIQUE"}]},{"role":"assistant","content":[{"type":"tool_use","id":"c1","name":"read_file","input":{"path":"example.txt"}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"c1","content":"synthetic output"}]}]"#),
        (index, r#"[{"id":"t1","task":"Task title","ts":1000,"cwdOnTaskInitialization":"/example/p","workspace":"/example/p"}]"#),
    ]);
    let s = load(key, root.path()).await;
    assert!(text(&s).contains("CLINE_FAMILY_UNIQUE"));
    assert!(s.messages.iter().flat_map(|m| &m.blocks).any(|b|
        matches!(b, ContentBlock::ToolCall { id: Some(id), .. } if id == "c1")));
    assert!(s.messages.iter().flat_map(|m| &m.blocks).any(|b|
        matches!(b, ContentBlock::ToolResult { id: Some(id), .. } if id == "c1")));
}

#[tokio::test]
async fn cline_imports_api_history() { cline_family("cline", "state/taskHistory.json").await; }
#[tokio::test]
async fn roo_code_imports_task_history() { cline_family("roo_code", "tasks/_index.json").await; }
#[tokio::test]
async fn kilo_code_imports_without_cline_index() { cline_family("kilo_code", "tasks/t1/task_metadata.json").await; }

#[tokio::test]
async fn aider_imports_explicit_project_without_splitting_fenced_headers() {
    let root = fixture(&[(".aider.chat.history.md", concat!(
        "# aider chat started at 2026-09-21 10:00:00\n\n#### AIDER_UNIQUE\n\n",
        "answer\n```md\n# aider chat started at 1900-01-01 00:00:00\n#### not-a-user\n```\n"
    ))]);
    let s = load("aider", root.path()).await;
    assert!(text(&s).contains("AIDER_UNIQUE"));
    assert!(text(&s).contains("#### not-a-user"));
}

#[tokio::test]
async fn kimi_code_replays_wire_messages() {
    let root = fixture(&[
        ("sessions/wd_test/session_s1/state.json", r#"{"id":"s1","version":2,"cwd":"/example/p","title":"Kimi title"}"#),
        ("sessions/wd_test/session_s1/agents/main/wire.jsonl", concat!(
            "{\"type\":\"context.append_message\",\"message\":{\"id\":\"u1\",\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"KIMI_UNIQUE\"}]},\"time\":1000}\n",
            "{\"type\":\"context.append_loop_event\",\"event\":{\"type\":\"step.begin\"},\"time\":2000}\n",
            "{\"type\":\"context.append_loop_event\",\"event\":{\"type\":\"content.part\",\"part\":{\"type\":\"text\",\"text\":\"answer\"}},\"time\":2100}\n",
            "{\"type\":\"context.append_loop_event\",\"event\":{\"type\":\"step.end\"},\"time\":2200}\n"
        )),
    ]);
    assert_eq!(text(&load("kimi_code", root.path()).await), "KIMI_UNIQUE\nanswer");
}

#[tokio::test]
async fn github_copilot_imports_real_cli_events() {
    let root = fixture(&[("session-state/s1/events.jsonl", concat!(
        "{\"id\":\"start\",\"type\":\"session.start\",\"data\":{\"sessionId\":\"s1\",\"context\":{\"cwd\":\"/example/p\"}}}\n",
        "{\"id\":\"u1\",\"type\":\"user.message\",\"data\":{\"content\":\"COPILOT_UNIQUE\",\"transformedContent\":\"duplicate wrapper\"}}\n",
        "{\"id\":\"a1\",\"type\":\"assistant.message\",\"data\":{\"content\":\"answer\"}}\n"
    ))]);
    assert_eq!(text(&load("github_copilot", root.path()).await), "COPILOT_UNIQUE\nanswer");
}

#[tokio::test]
async fn antigravity_imports_cli_transcript_not_token_stats() {
    let root = fixture(&[("brain/s1/.system_generated/logs/transcript_full.jsonl", concat!(
        "{\"step_index\":0,\"source\":\"USER_EXPLICIT\",\"type\":\"USER_INPUT\",\"content\":\"ANTIGRAVITY_UNIQUE\"}\n",
        "{\"step_index\":1,\"source\":\"MODEL\",\"type\":\"PLANNER_RESPONSE\",\"content\":\"answer\"}\n"
    ))]);
    assert_eq!(text(&load("antigravity", root.path()).await), "ANTIGRAVITY_UNIQUE\nanswer");
}

#[tokio::test]
async fn cursor_imports_sqlite_composer_messages_readonly() {
    let root = fixture(&[]);
    std::fs::create_dir(root.path().join("globalStorage")).unwrap();
    let path = root.path().join("globalStorage/state.vscdb");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE cursorDiskKV (key TEXT PRIMARY KEY, value TEXT);").unwrap();
    let composer = json!({"composerId":"s1","name":"Cursor title","conversation":[
        {"bubbleId":"u1","type":1,"text":"CURSOR_UNIQUE"},
        {"bubbleId":"a1","type":2,"text":"answer"}
    ]});
    conn.execute("INSERT INTO cursorDiskKV VALUES (?1, ?2)", rusqlite::params!["composerData:s1", composer.to_string()]).unwrap();
    drop(conn);
    let before = std::fs::read(&path).unwrap();
    assert_eq!(text(&load("cursor", root.path()).await), "CURSOR_UNIQUE\nanswer");
    assert_eq!(before, std::fs::read(path).unwrap());
}
