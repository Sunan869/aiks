use aiks_core::model::SourceKind;
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
