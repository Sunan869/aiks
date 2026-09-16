use aiks_core::sink::v41::{KnowledgeBindingAttrs, SiYuanContentStore};

#[test]
fn content_store_uses_one_notebook_roots() {
    assert_eq!(SiYuanContentStore::knowledge_root(), "/20 Knowledge");
    assert_eq!(SiYuanContentStore::session_root(), "/10 AI Sessions");
}

#[test]
fn knowledge_binding_attrs_include_v41_management_metadata() {
    let attrs = KnowledgeBindingAttrs {
        knowledge_id: "knowledge-1".into(),
        source_type: "conversation".into(),
        managed_by: "pipeline".into(),
        session_id: Some("session-1".into()),
        project: Some("AIKS".into()),
        category: "architecture".into(),
        generated_hash: "abc123".into(),
    }
    .to_block_attrs();

    assert_eq!(attrs.get("custom-aiks-managed").map(String::as_str), Some("true"));
    assert_eq!(attrs.get("custom-aiks-kind").map(String::as_str), Some("knowledge"));
    assert_eq!(attrs.get("custom-aiks-id").map(String::as_str), Some("knowledge-1"));
    assert_eq!(attrs.get("custom-aiks-source-type").map(String::as_str), Some("conversation"));
    assert_eq!(attrs.get("custom-aiks-managed-by").map(String::as_str), Some("pipeline"));
    assert_eq!(attrs.get("custom-aiks-session-id").map(String::as_str), Some("session-1"));
    assert_eq!(attrs.get("custom-aiks-project").map(String::as_str), Some("AIKS"));
    assert_eq!(attrs.get("custom-aiks-category").map(String::as_str), Some("architecture"));
    assert_eq!(attrs.get("custom-aiks-generated-hash").map(String::as_str), Some("abc123"));
}
