use aiks_core::storage::StateDb;
use tempfile::tempdir;

#[test]
fn v41_schema_adds_siyuan_content_bindings_and_migration_state() {
    let dir = tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("v41.db")).unwrap();
    let conn = db.conn();

    let knowledge_columns: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('knowledge_item') WHERE name IN ('siyuan_doc_id','generated_hash','current_remote_hash','migration_status')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(knowledge_columns, 4);

    let source_session_doc_id: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('source_session') WHERE name = 'siyuan_doc_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source_session_doc_id, 1);

    let migration_table: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='content_migration'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(migration_table, 1);
}

#[test]
fn v41_schema_upgrade_is_idempotent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("v41.db");
    StateDb::open(&path).unwrap();
    StateDb::open(&path).unwrap();
}
