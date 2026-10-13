use aiks_core::storage::StateDb;
use aiks_core::{CreateKnowledgeInput, KnowledgeService};
use tempfile::tempdir;

#[test]
fn native_siyuan_document_binds_without_generated_hash() {
    let dir = tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let record = KnowledgeService::new(&db)
        .create_native_siyuan(
            CreateKnowledgeInput {
                title: "Native note".into(),
                category: Some("general".into()),
                project_name: None,
                summary: Some(String::new()),
                content: "user authored".into(),
                tags: vec![],
            },
            "doc-native-1",
            Some("remote-hash-1"),
        )
        .unwrap();

    assert_eq!(record.siyuan_doc_id.as_deref(), Some("doc-native-1"));
    assert_eq!(record.generated_hash, None);
    assert_eq!(record.index_status, "pending");
}
