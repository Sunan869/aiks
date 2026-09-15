use aiks_core::{
    ai::schema_v3::V3ExtractionResult,
    pipeline::{
        embedding_client::EmbeddingConfig,
        hybrid_search,
        search::{search_with_status, SearchDegradationKind},
        KnowledgeRepo,
    },
    storage::{SourceSessionRepo, StateDb},
};

fn extraction() -> V3ExtractionResult {
    serde_json::from_value(serde_json::json!({
        "session_summary": "search regression",
        "knowledge_score": 0.95,
        "worth_extracting": true,
        "items": [{
            "title": "alpha OR parser",
            "category": "search",
            "summary": "alpha OR parser behavior",
            "content": "literal user-entered search syntax should stay searchable",
            "tags": ["fts"],
            "confidence": 0.95
        }]
    }))
    .unwrap()
}

fn seeded_db() -> (tempfile::TempDir, StateDb) {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let session_id = SourceSessionRepo::new(&db)
        .upsert(
            "claude_code",
            "search-session",
            None,
            None,
            Some("search"),
            Some("Search Session"),
            None,
            Some("search-hash"),
            Some("search-v1"),
        )
        .unwrap();
    KnowledgeRepo::new(&db)
        .save_items(session_id, Some("search"), &extraction())
        .unwrap();
    (dir, db)
}

#[tokio::test]
async fn malformed_fts_syntax_does_not_turn_search_into_an_error() {
    let (_dir, db) = seeded_db();

    let hits = hybrid_search(&db, "alpha OR", 20, None)
        .await
        .expect("malformed user FTS syntax must degrade safely instead of failing");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].match_type, "fts");
}

#[tokio::test]
async fn unavailable_vector_search_keeps_text_hits_and_reports_degradation() {
    let (_dir, db) = seeded_db();
    let cfg = EmbeddingConfig {
        enabled: true,
        base_url: "http://127.0.0.1:1/v1".into(),
        model: "unavailable-test-model".into(),
        ..Default::default()
    };

    let outcome = search_with_status(&db, "alpha", 20, Some(&cfg))
        .await
        .expect("vector failure must degrade instead of failing text search");

    assert_eq!(outcome.hits.len(), 1, "text search should remain usable");
    assert!(outcome
        .degradations
        .iter()
        .any(|d| d.kind == SearchDegradationKind::VectorUnavailable));
}
