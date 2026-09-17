use aiks_core::storage::StateDb;
use tempfile::tempdir;

const INDEX_COLUMNS: [&str; 7] = [
    "index_status",
    "indexed_hash",
    "indexed_at",
    "embedding_model",
    "embedding_dimensions",
    "index_chunk_count",
    "last_index_error",
];

fn assert_index_columns(db: &StateDb) {
    let conn = db.conn();
    for column in INDEX_COLUMNS {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('knowledge_item') WHERE name = ?1",
                [column],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "missing knowledge_item.{column}");
    }

    let defaults: (String, i64) = conn
        .query_row(
            "SELECT index_status, index_chunk_count
             FROM knowledge_item
             LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or_else(|_| ("pending".to_string(), 0));
    assert_eq!(defaults.0, "pending");
    assert_eq!(defaults.1, 0);
}

#[test]
fn knowledge_index_lifecycle_columns_survive_reopen() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("state.db");

    {
        let db = StateDb::open(&db_path).unwrap();
        assert_index_columns(&db);
    }

    {
        let db = StateDb::open(&db_path).unwrap();
        assert_index_columns(&db);
    }
}
