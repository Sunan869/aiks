use aiks_core::storage::StateDb;
use aiks_core::{CreateKnowledgeInput, KnowledgeService};
use tempfile::tempdir;

#[test]
fn knowledge_record_exposes_index_lifecycle_defaults() {
    let dir = tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let record = KnowledgeService::new(&db)
        .create_manual(CreateKnowledgeInput {
            title: "Lifecycle".into(),
            category: None,
            project_name: None,
            summary: None,
            content: "canonical body".into(),
            tags: vec![],
        })
        .unwrap();

    assert_eq!(record.index_status, "pending");
    assert_eq!(record.indexed_hash, None);
    assert_eq!(record.indexed_at, None);
    assert_eq!(record.embedding_model, None);
    assert_eq!(record.embedding_dimensions, None);
    assert_eq!(record.index_chunk_count, 0);
    assert_eq!(record.last_index_error, None);
}
