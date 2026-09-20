use aiks_core::model::{ContentBlock, MessageRole, SourceKind};
use aiks_core::providers::{workbuddy::WorkBuddyProvider, ProviderHealth, SessionProvider};
use rusqlite::Connection;
use tempfile::TempDir;

fn create_workbuddy_root() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("projects")).unwrap();
    dir
}

fn create_sessions_db(root: &TempDir) -> Connection {
    let conn = Connection::open(root.path().join("workbuddy.db")).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE sessions (
            id TEXT PRIMARY KEY,
            cwd TEXT NOT NULL,
            user_id TEXT NOT NULL,
            title TEXT,
            custom_title TEXT,
            status TEXT DEFAULT 'Pending',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER,
            mode TEXT,
            last_activity_at INTEGER,
            permission_mode TEXT,
            is_playground INTEGER DEFAULT 0,
            project_id TEXT,
            model TEXT
        );
        CREATE TABLE workspaces (
            path TEXT PRIMARY KEY,
            last_opened_at INTEGER NOT NULL
        );
        "#,
    )
    .unwrap();
    conn
}

#[tokio::test]
async fn workbuddy_discovers_active_sessions_and_prefers_custom_title() {
    let root = create_workbuddy_root();
    let conn = create_sessions_db(&root);
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, cwd, user_id, title, custom_title, status,
            created_at, updated_at, deleted_at, mode,
            last_activity_at, permission_mode, is_playground, model
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?10, ?11, ?12, ?13)
        "#,
        rusqlite::params![
            "active-session",
            r"C:\src\demo",
            "user-1",
            "AI title",
            "Custom title",
            "completed",
            1_782_828_714_330_i64,
            1_782_828_715_330_i64,
            "craft",
            1_782_828_716_330_i64,
            "fullAccess",
            0_i64,
            "test-model"
        ],
    )
    .unwrap();
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, cwd, user_id, title, created_at, updated_at, deleted_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        rusqlite::params![
            "deleted-session",
            "/tmp/deleted",
            "user-1",
            "Deleted",
            1_782_828_714_330_i64,
            1_782_828_715_330_i64,
            1_782_828_716_330_i64
        ],
    )
    .unwrap();
    drop(conn);

    let provider = WorkBuddyProvider::new(Some(root.path().to_path_buf())).unwrap();
    let sessions = provider.discover_sessions().await.unwrap();

    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.source, SourceKind::WorkBuddy);
    assert_eq!(session.external_session_id, "active-session");
    assert_eq!(session.title.as_deref(), Some("Custom title"));
    assert_eq!(session.project_path.as_deref(), Some(r"C:\src\demo"));
    assert_eq!(session.project_name.as_deref(), Some("demo"));
    assert_eq!(session.message_count, 0);
    assert!(session.started_at.is_some());
    assert!(session.updated_at.is_some());
}

#[tokio::test]
async fn healthy_workbuddy_database_with_zero_sessions_is_ok() {
    let root = create_workbuddy_root();
    let conn = create_sessions_db(&root);
    drop(conn);

    let provider = WorkBuddyProvider::new(Some(root.path().to_path_buf())).unwrap();

    assert!(matches!(provider.health_check().await, ProviderHealth::Ok));
    assert!(provider.discover_sessions().await.unwrap().is_empty());
}

#[tokio::test]
async fn transcript_loads_by_internal_session_id_and_maps_known_events() {
    let root = create_workbuddy_root();
    let conn = create_sessions_db(&root);
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, cwd, user_id, title, custom_title, status,
            created_at, updated_at, deleted_at, mode,
            last_activity_at, permission_mode, is_playground, model
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?10, ?11, ?12, ?13)
        "#,
        rusqlite::params![
            "s1",
            r"C:\src\demo",
            "user-1",
            "Database title",
            "Database custom title",
            "completed",
            1_782_828_714_330_i64,
            1_782_828_715_330_i64,
            "craft",
            1_782_828_716_330_i64,
            "fullAccess",
            0_i64,
            "workbuddy-model"
        ],
    )
    .unwrap();
    drop(conn);

    let transcript_dir = root.path().join("projects").join("nested");
    std::fs::create_dir_all(&transcript_dir).unwrap();
    let transcript_path = transcript_dir.join("wrong-name.jsonl");
    std::fs::write(
        &transcript_path,
        concat!(
            r#"{"id":"u1","timestamp":1782828714330,"type":"message","role":"user","content":[{"type":"input_text","text":"<system-reminder>x</system-reminder><user_query>真实问题</user_query>"}],"session_id":"s1","cwd":"C:\\src\\demo"}"#,
            "\n",
            r#"{"id":"a1","timestamp":1782828715330,"type":"message","role":"assistant","content":[{"type":"output_text","text":"回答"}],"sessionId":"s1"}"#,
            "\n",
            r#"{"id":"u2","timestamp":1782828716330,"type":"message","role":"user","content":[{"type":"text","text":"未加标签的问题"}],"sessionId":"s1"}"#,
            "\n",
            r#"{"id":"sys1","timestamp":1782828717330,"type":"message","role":"system","content":"系统说明","sessionId":"s1"}"#,
            "\n",
            r#"{"id":"r1","timestamp":1782828718330,"type":"reasoning","text":"思考","sessionId":"s1"}"#,
            "\n",
            r#"{"id":"call-1","timestamp":1782828719330,"type":"function_call","name":"read_file","arguments":{"path":"a.txt"},"sessionId":"s1"}"#,
            "\n",
            r#"{"id":"call-1","timestamp":1782828720330,"type":"function_call_result","content":"ok","sessionId":"s1"}"#,
            "\n",
            r#"{"id":"snapshot-1","timestamp":1782828721330,"type":"file-history-snapshot","content":"must-not-load-file-history","sessionId":"s1"}"#,
            "\n",
            r#"{"timestamp":1782828722330,"type":"ai-title","aiTitle":"Ignored JSONL title","sessionId":"s1"}"#,
            "\nnot-json\n"
        ),
    )
    .unwrap();

    let provider = WorkBuddyProvider::new(Some(root.path().to_path_buf())).unwrap();
    let sessions = provider.discover_sessions().await.unwrap();
    assert_eq!(sessions.len(), 1);
    let summary = &sessions[0];
    assert_eq!(summary.external_session_id, "s1");
    assert_eq!(summary.source_path.as_deref(), Some(transcript_path.as_path()));

    let session = provider.load_session(summary).await.unwrap();

    assert_eq!(session.source, SourceKind::WorkBuddy);
    assert_eq!(session.external_session_id, "s1");
    assert_eq!(session.title.as_deref(), Some("Database custom title"));
    assert_eq!(session.project_path.as_deref(), Some(r"C:\src\demo"));
    assert_eq!(session.project_name.as_deref(), Some("demo"));
    assert_eq!(session.source_path.as_deref(), Some(transcript_path.as_path()));
    assert_eq!(session.model.as_deref(), Some("workbuddy-model"));
    assert_eq!(session.metadata.get("status").and_then(|v| v.as_str()), Some("completed"));
    assert_eq!(session.metadata.get("mode").and_then(|v| v.as_str()), Some("craft"));
    assert_eq!(
        session
            .metadata
            .get("permission_mode")
            .and_then(|v| v.as_str()),
        Some("fullAccess")
    );
    assert_eq!(session.messages.len(), 8);

    assert_eq!(session.messages[0].external_id, "u1");
    assert_eq!(session.messages[0].role, MessageRole::User);
    match session.messages[0].blocks.as_slice() {
        [ContentBlock::Text { text }] => assert_eq!(text, "真实问题"),
        blocks => panic!("unexpected user blocks: {blocks:?}"),
    }

    assert_eq!(session.messages[1].role, MessageRole::Assistant);
    match session.messages[1].blocks.as_slice() {
        [ContentBlock::Text { text }] => assert_eq!(text, "回答"),
        blocks => panic!("unexpected assistant blocks: {blocks:?}"),
    }

    match session.messages[2].blocks.as_slice() {
        [ContentBlock::Text { text }] => assert_eq!(text, "未加标签的问题"),
        blocks => panic!("unexpected fallback user blocks: {blocks:?}"),
    }

    assert_eq!(session.messages[3].role, MessageRole::System);
    match session.messages[3].blocks.as_slice() {
        [ContentBlock::Text { text }] => assert_eq!(text, "系统说明"),
        blocks => panic!("unexpected string-content blocks: {blocks:?}"),
    }

    match session.messages[4].blocks.as_slice() {
        [ContentBlock::Thinking { text }] => assert_eq!(text, "思考"),
        blocks => panic!("unexpected reasoning blocks: {blocks:?}"),
    }

    match session.messages[5].blocks.as_slice() {
        [ContentBlock::ToolCall { id, name, input }] => {
            assert_eq!(id.as_deref(), Some("call-1"));
            assert_eq!(name, "read_file");
            assert_eq!(input.get("path").and_then(|v| v.as_str()), Some("a.txt"));
        }
        blocks => panic!("unexpected function-call blocks: {blocks:?}"),
    }

    assert_eq!(session.messages[6].role, MessageRole::Tool);
    match session.messages[6].blocks.as_slice() {
        [ContentBlock::ToolResult {
            id,
            content,
            is_error,
        }] => {
            assert_eq!(id.as_deref(), Some("call-1"));
            assert_eq!(content, "ok");
            assert!(!is_error);
        }
        blocks => panic!("unexpected function-result blocks: {blocks:?}"),
    }

    assert_eq!(session.messages[7].role, MessageRole::Unknown);
    assert!(matches!(
        session.messages[7].blocks.as_slice(),
        [ContentBlock::Unknown { .. }]
    ));
}
