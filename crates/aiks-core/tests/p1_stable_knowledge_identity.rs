use aiks_core::{
    ai::schema_v3::{V3ExtractionResult, V3KnowledgeItem},
    pipeline::KnowledgeRepo,
    storage::{KnowledgeSyncRepo, SourceSessionRepo, StateDb, SyncStatus},
};

fn item(title: &str, category: &str, summary: &str, content: &str) -> V3KnowledgeItem {
    V3KnowledgeItem {
        title: title.to_string(),
        category: category.to_string(),
        summary: summary.to_string(),
        content: content.to_string(),
        problem: None,
        root_causes: None,
        solutions: None,
        key_commands: None,
        key_files: None,
        decisions: None,
        tags: vec!["stable-id".to_string()],
        confidence: 0.9,
    }
}

fn result(items: Vec<V3KnowledgeItem>) -> V3ExtractionResult {
    V3ExtractionResult {
        session_summary: "session".to_string(),
        knowledge_score: 0.9,
        worth_extracting: true,
        items,
    }
}

fn setup() -> (tempfile::TempDir, StateDb, i64) {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let session_id = SourceSessionRepo::new(&db)
        .upsert(
            "opencode",
            "session-stable-knowledge",
            None,
            None,
            Some("project"),
            Some("Session"),
            None,
            Some("source-hash"),
            Some("test-v1"),
        )
        .unwrap();
    (dir, db, session_id)
}

fn attach_mapping(db: &StateDb, knowledge_id: &str, target_id: &str) {
    let repo = KnowledgeSyncRepo::new(db);
    repo.record_target_doc(knowledge_id, "siyuan", target_id, "/20 Knowledge/test")
        .unwrap();
    repo.mark_synced(
        knowledge_id,
        "siyuan",
        target_id,
        "/20 Knowledge/test",
        "rendered-hash",
    )
    .unwrap();
}

#[test]
fn identical_reextraction_reuses_knowledge_id_and_siyuan_mapping() {
    let (_dir, db, session_id) = setup();
    let repo = KnowledgeRepo::new(&db);
    let extraction = result(vec![item("Durable Queue", "architecture", "summary", "content")]);

    let first = repo.save_items(session_id, Some("project"), &extraction).unwrap();
    attach_mapping(&db, &first[0], "doc-1");

    let second = repo.save_items(session_id, Some("project"), &extraction).unwrap();

    assert_eq!(second, first);
    let mapping = KnowledgeSyncRepo::new(&db)
        .find(&second[0], "siyuan")
        .unwrap()
        .expect("mapping must survive unchanged re-extraction");
    assert_eq!(mapping.target_id.as_deref(), Some("doc-1"));
}

#[test]
fn changed_content_with_same_identity_preserves_knowledge_id() {
    let (_dir, db, session_id) = setup();
    let repo = KnowledgeRepo::new(&db);

    let first = repo
        .save_items(
            session_id,
            Some("project"),
            &result(vec![item(
                "Queue Design",
                "architecture",
                "old summary",
                "old content",
            )]),
        )
        .unwrap();
    attach_mapping(&db, &first[0], "doc-2");

    let changed = repo
        .save_items(
            session_id,
            Some("project"),
            &result(vec![item(
                "  QUEUE   DESIGN  ",
                " Architecture ",
                "new summary",
                "new content",
            )]),
        )
        .unwrap();

    assert_eq!(changed[0], first[0]);
    let mapping = KnowledgeSyncRepo::new(&db)
        .find(&changed[0], "siyuan")
        .unwrap()
        .expect("mapping must remain attached to the normalized identity");
    assert_eq!(mapping.target_id.as_deref(), Some("doc-2"));
}

#[test]
fn different_knowledge_in_same_category_must_not_inherit_old_mapping() {
    let (_dir, db, session_id) = setup();
    let repo = KnowledgeRepo::new(&db);

    let first = repo
        .save_items(
            session_id,
            Some("project"),
            &result(vec![item(
                "Queue Design",
                "architecture",
                "queue summary",
                "queue content",
            )]),
        )
        .unwrap();
    attach_mapping(&db, &first[0], "doc-old-queue");

    let replacement = repo
        .save_items(
            session_id,
            Some("project"),
            &result(vec![item(
                "GPU Worker Topology",
                "architecture",
                "gpu summary",
                "gpu content",
            )]),
        )
        .unwrap();

    assert_ne!(replacement[0], first[0]);
    assert!(KnowledgeSyncRepo::new(&db)
        .find(&replacement[0], "siyuan")
        .unwrap()
        .is_none());

    let old_mapping = KnowledgeSyncRepo::new(&db)
        .find(&first[0], "siyuan")
        .unwrap()
        .expect("old mapping must remain as a tombstone instead of moving to new knowledge");
    assert_eq!(old_mapping.status, SyncStatus::Removed);
    assert_eq!(old_mapping.target_id.as_deref(), Some("doc-old-queue"));
}

#[test]
fn removed_knowledge_item_tombstones_mapping_and_cleans_dependents() {
    let (_dir, db, session_id) = setup();
    let repo = KnowledgeRepo::new(&db);

    let first = repo
        .save_items(
            session_id,
            Some("project"),
            &result(vec![
                item("Keep", "architecture", "keep", "keep"),
                item("Remove", "debugging", "remove", "remove"),
            ]),
        )
        .unwrap();
    attach_mapping(&db, &first[0], "doc-keep");
    attach_mapping(&db, &first[1], "doc-remove");

    let chunk_ids = repo
        .save_embedding_chunks(&first[1], &[(None, "obsolete embedding text".to_string())])
        .unwrap();
    repo.save_embedding(&chunk_ids[0], "test-model", 2, &[0.1, 0.2])
        .unwrap();

    let second = repo
        .save_items(
            session_id,
            Some("project"),
            &result(vec![item("Keep", "architecture", "keep", "keep updated")]),
        )
        .unwrap();

    assert_eq!(second, vec![first[0].clone()]);
    let removed_mapping = KnowledgeSyncRepo::new(&db)
        .find(&first[1], "siyuan")
        .unwrap()
        .expect("removed knowledge keeps an explicit tombstone mapping");
    assert_eq!(removed_mapping.status, SyncStatus::Removed);
    assert_eq!(removed_mapping.target_id.as_deref(), Some("doc-remove"));

    let conn = db.conn();
    let stale_items: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_item WHERE id = ?1",
            [&first[1]],
            |r| r.get(0),
        )
        .unwrap();
    let stale_chunks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_chunk WHERE knowledge_id = ?1",
            [&first[1]],
            |r| r.get(0),
        )
        .unwrap();
    let stale_embeddings: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM embedding_record WHERE chunk_id = ?1",
            [&chunk_ids[0]],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!((stale_items, stale_chunks, stale_embeddings), (0, 0, 0));
}
