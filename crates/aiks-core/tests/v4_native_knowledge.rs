use aiks_core::ai::{V3ExtractionResult, V3KnowledgeItem};
use aiks_core::pipeline::search::search_with_status;
use aiks_core::storage::StateDb;
use aiks_core::{CreateKnowledgeInput, KnowledgeRepo, KnowledgeService, UpdateKnowledgeInput};
use tempfile::tempdir;

fn db() -> (tempfile::TempDir, StateDb) {
    let dir = tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("v4.db")).unwrap();
    (dir, db)
}

fn create_session(db: &StateDb) -> i64 {
    let now = chrono::Utc::now().to_rfc3339();
    let conn = db.conn();
    conn.execute(
        "INSERT INTO source_session
         (source, external_session_id, project_name, title, last_seen_at, created_at, updated_at)
         VALUES ('opencode', 'session-v4', 'AIKS', 'V4 test session', ?1, ?1, ?1)",
        rusqlite::params![now],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn extracted(title: &str, content: &str) -> V3ExtractionResult {
    V3ExtractionResult {
        session_summary: "summary".into(),
        knowledge_score: 0.9,
        worth_extracting: true,
        items: vec![V3KnowledgeItem {
            title: title.into(),
            category: "implementation".into(),
            summary: "AI summary".into(),
            content: content.into(),
            problem: None,
            root_causes: None,
            solutions: None,
            key_commands: None,
            key_files: None,
            decisions: None,
            tags: vec!["v4".into()],
            confidence: 0.9,
        }],
    }
}

#[test]
fn manual_knowledge_crud_is_native_and_invalidates_derived_content() {
    let (_dir, db) = db();
    let service = KnowledgeService::new(&db);
    let created = service
        .create_manual(CreateKnowledgeInput {
            title: "  手工知识  ".into(),
            category: Some("implementation".into()),
            project_name: Some(" AIKS ".into()),
            summary: Some("本地知识".into()),
            content: "初始正文".into(),
            tags: vec!["Rust".into(), " rust ".into(), "".into()],
        })
        .unwrap();

    assert!(created.source_session_id.is_none());
    assert_eq!(created.source_type, "manual");
    assert_eq!(created.managed_by, "user");
    assert_eq!(created.status, "active");
    assert!(!created.is_favorite);
    assert_eq!(created.tags, vec!["Rust"]);

    let repo = KnowledgeRepo::new(&db);
    let chunk_ids = repo
        .save_embedding_chunks(&created.id, &[(None, "初始正文".into())])
        .unwrap();
    repo.save_embedding(&chunk_ids[0], "test", 2, &[0.1, 0.2])
        .unwrap();

    let updated = service
        .update(
            &created.id,
            UpdateKnowledgeInput {
                title: "手工知识（已编辑）".into(),
                category: "implementation".into(),
                project_name: Some("AIKS".into()),
                summary: "更新后的摘要".into(),
                content: "更新后的正文".into(),
                tags: vec!["Rust".into(), "Desktop".into()],
            },
        )
        .unwrap();
    assert_eq!(updated.managed_by, "user");

    let conn = db.conn();
    let chunks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_chunk WHERE knowledge_id = ?1",
            rusqlite::params![created.id],
            |row| row.get(0),
        )
        .unwrap();
    let fts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_fts WHERE knowledge_id = ?1 AND title = ?2",
            rusqlite::params![created.id, "手工知识（已编辑）"],
            |row| row.get(0),
        )
        .unwrap();
    drop(conn);
    assert_eq!(chunks, 0);
    assert_eq!(fts, 1);

    assert!(service.set_favorite(&created.id, true).unwrap().is_favorite);
    assert_eq!(service.archive(&created.id).unwrap().status, "archived");
    assert_eq!(service.restore(&created.id).unwrap().status, "active");
}

#[test]
fn v41_manual_control_row_is_created_with_canonical_siyuan_binding() {
    let (_dir, db) = db();
    let service = KnowledgeService::new(&db);
    let created = service
        .create_manual_bound(
            "manual-v41-1",
            CreateKnowledgeInput {
                title: "SiYuan 主内容".into(),
                category: Some("implementation".into()),
                project_name: Some("AIKS".into()),
                summary: Some("控制元数据".into()),
                content: "兼容快照".into(),
                tags: vec!["SiYuan".into()],
            },
            "doc-v41-1",
            "generated-hash-1",
            Some("remote-hash-1"),
        )
        .unwrap();

    assert_eq!(created.id, "manual-v41-1");
    assert_eq!(created.siyuan_doc_id.as_deref(), Some("doc-v41-1"));
    assert_eq!(
        created.generated_hash.as_deref(),
        Some("generated-hash-1")
    );
    assert_eq!(created.source_type, "manual");
    assert_eq!(created.managed_by, "user");
}

#[test]
fn v41_existing_knowledge_can_record_canonical_siyuan_binding() {
    let (_dir, db) = db();
    let service = KnowledgeService::new(&db);
    let created = service
        .create_manual(CreateKnowledgeInput {
            title: "待绑定".into(),
            category: None,
            project_name: None,
            summary: None,
            content: "snapshot".into(),
            tags: vec![],
        })
        .unwrap();

    let bound = service
        .bind_siyuan_document(
            &created.id,
            "doc-existing-1",
            "generated-existing-1",
            Some("remote-existing-1"),
            "migrated",
        )
        .unwrap();

    assert_eq!(bound.siyuan_doc_id.as_deref(), Some("doc-existing-1"));
    assert_eq!(
        bound.generated_hash.as_deref(),
        Some("generated-existing-1")
    );
}

#[tokio::test]
async fn archived_knowledge_is_excluded_from_search() {
    let (_dir, db) = db();
    let service = KnowledgeService::new(&db);
    let item = service
        .create_manual(CreateKnowledgeInput {
            title: "独有搜索词 ZebraV4".into(),
            category: None,
            project_name: None,
            summary: None,
            content: "ZebraV4 only content".into(),
            tags: vec![],
        })
        .unwrap();

    let before = search_with_status(&db, "ZebraV4", 10, None).await.unwrap();
    assert_eq!(before.hits.len(), 1);

    service.archive(&item.id).unwrap();
    let after = search_with_status(&db, "ZebraV4", 10, None).await.unwrap();
    assert!(after.hits.is_empty());
}

#[test]
fn reextraction_never_overwrites_or_deletes_user_managed_session_knowledge() {
    let (_dir, db) = db();
    let session_id = create_session(&db);
    let repo = KnowledgeRepo::new(&db);

    let ids = repo
        .save_items(
            session_id,
            Some("AIKS"),
            &extracted("V4 identity", "AI original"),
        )
        .unwrap();
    let id = ids[0].clone();

    let service = KnowledgeService::new(&db);
    service
        .update(
            &id,
            UpdateKnowledgeInput {
                title: "V4 identity".into(),
                category: "implementation".into(),
                project_name: Some("AIKS".into()),
                summary: "用户摘要".into(),
                content: "USER CONTENT".into(),
                tags: vec!["user".into()],
            },
        )
        .unwrap();

    let ids = repo
        .save_items(
            session_id,
            Some("AIKS"),
            &extracted("V4 identity", "AI overwrite attempt"),
        )
        .unwrap();
    assert_eq!(ids, vec![id.clone()]);
    let preserved = service.get(&id).unwrap().unwrap();
    assert_eq!(preserved.content, "USER CONTENT");
    assert_eq!(preserved.managed_by, "user");

    let new_ids = repo
        .save_items(
            session_id,
            Some("AIKS"),
            &extracted("V4 new identity", "new AI item"),
        )
        .unwrap();
    assert_eq!(new_ids.len(), 1);
    assert_ne!(new_ids[0], id);
    assert!(service.get(&id).unwrap().is_some());
}
