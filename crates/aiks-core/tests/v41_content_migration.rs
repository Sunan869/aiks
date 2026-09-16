use aiks_core::knowledge::migration::{
    decide_migration, ContentMigrationService, MigrationDecision, MigrationSnapshot,
};
use aiks_core::knowledge::refresh_siyuan_document_read_model;
use aiks_core::storage::StateDb;
use aiks_core::{CreateKnowledgeInput, KnowledgeService};
use rusqlite::params;

fn snapshot(
    target_id: Option<&str>,
    synced_hash: Option<&str>,
    target_hash: Option<&str>,
    local_hash: &str,
    remote_hash: Option<&str>,
) -> MigrationSnapshot {
    MigrationSnapshot {
        target_id: target_id.map(str::to_string),
        synced_hash: synced_hash.map(str::to_string),
        target_hash: target_hash.map(str::to_string),
        local_hash: local_hash.to_string(),
        remote_hash: remote_hash.map(str::to_string),
    }
}

#[test]
fn unpublished_legacy_knowledge_is_created_in_siyuan() {
    let decision = decide_migration(&snapshot(None, None, None, "local-a", None));
    assert_eq!(decision, MigrationDecision::Create);
}

#[test]
fn unchanged_existing_document_is_reused() {
    let decision = decide_migration(&snapshot(
        Some("doc-1"),
        Some("local-a"),
        Some("remote-a"),
        "local-a",
        Some("remote-a"),
    ));
    assert_eq!(
        decision,
        MigrationDecision::Reuse {
            doc_id: "doc-1".into()
        }
    );
}

#[test]
fn locally_changed_only_document_is_updated_before_cutover() {
    let decision = decide_migration(&snapshot(
        Some("doc-1"),
        Some("local-a"),
        Some("remote-a"),
        "local-b",
        Some("remote-a"),
    ));
    assert_eq!(
        decision,
        MigrationDecision::Update {
            doc_id: "doc-1".into()
        }
    );
}

#[test]
fn remotely_changed_document_is_preserved_as_conflict() {
    let decision = decide_migration(&snapshot(
        Some("doc-1"),
        Some("local-a"),
        Some("remote-a"),
        "local-a",
        Some("remote-b"),
    ));
    assert_eq!(
        decision,
        MigrationDecision::Conflict {
            doc_id: "doc-1".into()
        }
    );
}

#[test]
fn both_sides_changed_document_is_preserved_as_conflict() {
    let decision = decide_migration(&snapshot(
        Some("doc-1"),
        Some("local-a"),
        Some("remote-a"),
        "local-b",
        Some("remote-b"),
    ));
    assert_eq!(
        decision,
        MigrationDecision::Conflict {
            doc_id: "doc-1".into()
        }
    );
}

#[test]
fn existing_document_without_a_baseline_is_reused_conservatively() {
    let decision = decide_migration(&snapshot(
        Some("doc-1"),
        None,
        None,
        "local-a",
        Some("remote-a"),
    ));
    assert_eq!(
        decision,
        MigrationDecision::Reuse {
            doc_id: "doc-1".into()
        }
    );
}

#[test]
fn persisted_migration_stats_are_aggregated_for_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("aiks.db")).unwrap();
    let conn = db.conn();
    for (entity_id, status) in [
        ("knowledge-1", "migrated"),
        ("knowledge-2", "reused"),
        ("knowledge-3", "conflict"),
        ("knowledge-4", "failed"),
        ("knowledge-5", "pending"),
    ] {
        conn.execute(
            "INSERT INTO content_migration
             (entity_type, entity_id, status, updated_at)
             VALUES ('knowledge', ?1, ?2, '2026-09-16T00:00:00Z')",
            params![entity_id, status],
        )
        .unwrap();
    }
    drop(conn);

    let stats = ContentMigrationService::new(&db).stats().unwrap();
    assert_eq!(stats.total, 5);
    assert_eq!(stats.pending, 1);
    assert_eq!(stats.migrated, 1);
    assert_eq!(stats.reused, 1);
    assert_eq!(stats.conflicts, 1);
    assert_eq!(stats.failed, 1);
}

#[test]
fn knowledge_without_a_migration_row_is_reported_as_pending() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("aiks.db")).unwrap();
    KnowledgeService::new(&db)
        .create_manual(CreateKnowledgeInput {
            title: "Not migrated yet".into(),
            category: Some("general".into()),
            project_name: Some("AIKS".into()),
            summary: Some("pending migration".into()),
            content: "pending body".into(),
            tags: vec!["v4.1".into()],
        })
        .unwrap();

    let stats = ContentMigrationService::new(&db).stats().unwrap();
    assert_eq!(stats.total, 1);
    assert_eq!(stats.pending, 1);
    assert_eq!(stats.migrated, 0);
    assert_eq!(stats.reused, 0);
    assert_eq!(stats.conflicts, 0);
    assert_eq!(stats.failed, 0);
}

#[test]
fn siyuan_refresh_rebuilds_read_model_and_fts_from_canonical_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("aiks.db")).unwrap();
    let service = KnowledgeService::new(&db);
    service
        .create_manual_bound(
            "knowledge-1",
            CreateKnowledgeInput {
                title: "Canonical knowledge".into(),
                category: Some("general".into()),
                project_name: Some("AIKS".into()),
                summary: Some("summary".into()),
                content: "old cached body".into(),
                tags: vec!["v4.1".into()],
            },
            "siyuan-doc-1",
            "generated-hash",
            Some("old-remote-hash"),
        )
        .unwrap();

    service.invalidate_siyuan_document("siyuan-doc-1").unwrap();
    let refreshed = refresh_siyuan_document_read_model(
        &db,
        "siyuan-doc-1",
        "# Canonical knowledge\n\nfresh text edited in SiYuan",
    )
    .unwrap();
    assert_eq!(refreshed.as_deref(), Some("knowledge-1"));

    let item = service.get("knowledge-1").unwrap().unwrap();
    assert_eq!(
        item.content,
        "# Canonical knowledge\n\nfresh text edited in SiYuan"
    );
    assert_eq!(item.managed_by, "user");

    let conn = db.conn();
    let fts_content: String = conn
        .query_row(
            "SELECT content FROM knowledge_fts WHERE knowledge_id = 'knowledge-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let remote_hash: Option<String> = conn
        .query_row(
            "SELECT current_remote_hash FROM knowledge_item WHERE id = 'knowledge-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(fts_content, item.content);
    assert!(remote_hash.is_some_and(|hash| !hash.is_empty()));
}

#[test]
fn siyuan_edit_invalidates_fts_chunks_and_embeddings_without_deleting_knowledge() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("aiks.db")).unwrap();
    let service = KnowledgeService::new(&db);
    let item = service
        .create_manual_bound(
            "knowledge-1",
            CreateKnowledgeInput {
                title: "Canonical knowledge".into(),
                category: Some("general".into()),
                project_name: Some("AIKS".into()),
                summary: Some("summary".into()),
                content: "old cached body".into(),
                tags: vec!["v4.1".into()],
            },
            "siyuan-doc-1",
            "generated-hash",
            Some("remote-hash"),
        )
        .unwrap();

    db.conn()
        .execute(
            "INSERT INTO knowledge_chunk
             (id, knowledge_id, heading, chunk_index, token_count, text, content_hash, created_at)
             VALUES ('chunk-1', ?1, NULL, 0, 3, 'old cached body', 'chunk-hash', '2026-09-16T00:00:00Z')",
            params![item.id],
        )
        .unwrap();
    db.conn()
        .execute(
            "INSERT INTO embedding_record
             (id, chunk_id, model, dimensions, vector, created_at)
             VALUES ('embedding-1', 'chunk-1', 'test-model', 1, X'00000000', '2026-09-16T00:00:00Z')",
            [],
        )
        .unwrap();

    let invalidated = service.invalidate_siyuan_document("siyuan-doc-1").unwrap();
    assert_eq!(invalidated.as_deref(), Some("knowledge-1"));

    let conn = db.conn();
    let fts_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_fts WHERE knowledge_id = 'knowledge-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let chunk_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_chunk WHERE knowledge_id = 'knowledge-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let embedding_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM embedding_record WHERE id = 'embedding-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let managed_by: String = conn
        .query_row(
            "SELECT managed_by FROM knowledge_item WHERE id = 'knowledge-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(fts_count, 0);
    assert_eq!(chunk_count, 0);
    assert_eq!(embedding_count, 0);
    assert_eq!(managed_by, "user");
    assert!(service.get("knowledge-1").unwrap().is_some());
}
