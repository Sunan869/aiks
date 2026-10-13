use aiks_core::storage::StateDb;
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn legacy_v4_database_upgrades_to_allow_deleted_knowledge_status() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("legacy-v4.db");

    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE knowledge_item (
            id TEXT PRIMARY KEY,
            source_session_id INTEGER,
            project_name TEXT,
            title TEXT NOT NULL,
            category TEXT NOT NULL DEFAULT 'general',
            summary TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL,
            tags TEXT NOT NULL DEFAULT '[]',
            confidence REAL NOT NULL DEFAULT 0.0,
            worth_extracting INTEGER NOT NULL DEFAULT 1,
            source_type TEXT NOT NULL DEFAULT 'conversation'
                CHECK(source_type IN ('conversation', 'manual')),
            managed_by TEXT NOT NULL DEFAULT 'pipeline'
                CHECK(managed_by IN ('pipeline', 'user')),
            status TEXT NOT NULL DEFAULT 'active'
                CHECK(status IN ('active', 'archived')),
            is_favorite INTEGER NOT NULL DEFAULT 0
                CHECK(is_favorite IN (0, 1)),
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        INSERT INTO knowledge_item (
            id, title, content, created_at, updated_at
        ) VALUES (
            'legacy-knowledge', 'Legacy knowledge', 'legacy body',
            '2026-09-17T00:00:00Z', '2026-09-17T00:00:00Z'
        );
        "#,
    )
    .unwrap();
    drop(conn);

    let db = StateDb::open(&db_path).unwrap();
    let conn = db.conn();

    conn.execute(
        "UPDATE knowledge_item SET status = 'deleted' WHERE id = 'legacy-knowledge'",
        [],
    )
    .unwrap();

    let status: String = conn
        .query_row(
            "SELECT status FROM knowledge_item WHERE id = 'legacy-knowledge'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "deleted");
}
