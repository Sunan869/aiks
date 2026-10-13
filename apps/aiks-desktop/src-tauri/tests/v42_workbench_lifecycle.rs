const EVENTS_SOURCE: &str = include_str!("../src/workbench/events.rs");

#[test]
fn routes_siyuan_document_lifecycle_explicitly() {
    assert!(EVENTS_SOURCE.contains("\"documentCreated\" =>"));
    assert!(EVENTS_SOURCE.contains("\"documentChanged\" | \"knowledgeModified\" =>"));
    assert!(EVENTS_SOURCE.contains("\"documentDeleted\" =>"));
    assert!(EVENTS_SOURCE.contains("knowledge-index-status"));
}
