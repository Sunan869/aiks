#[test]
fn pipeline_no_longer_writes_knowledge_embeddings_directly() {
    let worker = include_str!("../src/pipeline/worker.rs");
    assert!(!worker.contains("EmbeddingStage::chunk_knowledge"));
    assert!(!worker.contains("embed_knowledge(db"));
    assert!(worker.contains("canonical SiYuan"));
}

#[test]
fn manual_creation_enters_canonical_index_hook() {
    let commands = include_str!("../../../apps/aiks-desktop/src-tauri/src/knowledge_commands.rs");
    assert!(commands.contains("index_canonical_knowledge"));
}
