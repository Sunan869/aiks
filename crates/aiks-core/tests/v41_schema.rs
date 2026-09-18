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
fn siyuan_sync_target_updates_canonical_session_doc_binding() {
    let dir = tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("v41.db")).unwrap();
    let conn = db.conn();
    conn.execute(
        "INSERT INTO source_session
         (source, external_session_id, last_seen_at, created_at, updated_at)
         VALUES ('opencode', 'session-1', 'now', 'now', 'now')",
        [],
    )
    .unwrap();
    let session_id = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO sync_target (session_id, sink, target_id, status)
         VALUES (?1, 'siyuan', 'doc-1', 'SYNCED')",
        [session_id],
    )
    .unwrap();
    let first: Option<String> = conn
        .query_row(
            "SELECT siyuan_doc_id FROM source_session WHERE id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(first.as_deref(), Some("doc-1"));

    conn.execute(
        "UPDATE sync_target SET target_id = 'doc-2' WHERE session_id = ?1 AND sink = 'siyuan'",
        [session_id],
    )
    .unwrap();
    let updated: Option<String> = conn
        .query_row(
            "SELECT siyuan_doc_id FROM source_session WHERE id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(updated.as_deref(), Some("doc-2"));
}

#[test]
fn v41_schema_upgrade_is_idempotent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("v41.db");
    StateDb::open(&path).unwrap();
    StateDb::open(&path).unwrap();
}
