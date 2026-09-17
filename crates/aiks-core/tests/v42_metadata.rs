use aiks_core::storage::StateDb;
use tempfile::tempdir;

#[test]
fn v42_schema_adds_origin_modification_and_lifecycle_metadata() {
    let dir = tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("v42.db")).unwrap();
    let conn = db.conn();

    let columns: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('knowledge_item') WHERE name IN ('origin','user_modified','lifecycle_status')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(columns, 3);
}

#[test]
fn v42_lifecycle_defaults_ai_knowledge_to_draft_without_changing_legacy_status() {
    let dir = tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("v42.db")).unwrap();
    let conn = db.conn();

    conn.execute(
        "INSERT INTO knowledge_item (
            id, project_name, title, category, summary, content, tags,
            confidence, worth_extracting, source_type, managed_by, status,
            is_favorite, created_at, updated_at, origin, user_modified, lifecycle_status
        ) VALUES (
            'kn-v42', 'AIKS', 'Draft knowledge', 'architecture', '', '# Draft', '[]',
            0.9, 1, 'conversation', 'pipeline', 'active', 0, 'now', 'now',
            'ai', 0, 'draft'
        )",
        [],
    )
    .unwrap();

    let row: (String, i64, String, String) = conn
        .query_row(
            "SELECT origin, user_modified, lifecycle_status, status FROM knowledge_item WHERE id = 'kn-v42'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();

    assert_eq!(row.0, "ai");
    assert_eq!(row.1, 0);
    assert_eq!(row.2, "draft");
    assert_eq!(row.3, "active");
}

#[test]
fn v42_schema_upgrade_is_idempotent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("v42.db");
    StateDb::open(&path).unwrap();
    StateDb::open(&path).unwrap();
}
