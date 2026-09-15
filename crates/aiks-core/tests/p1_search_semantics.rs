use aiks_core::{
    ai::schema_v3::V3ExtractionResult,
    pipeline::{
        embedding_client::EmbeddingConfig,
        hybrid_search,
        search::{search_with_status, SearchDegradationKind, VECTOR_CANDIDATE_CAP},
        KnowledgeRepo,
    },
    storage::{SourceSessionRepo, StateDb},
};
use rusqlite::params;

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

#[test]
fn ten_thousand_embeddings_are_reduced_to_a_bounded_candidate_set() {
    const EMBEDDING_COUNT: usize = 10_000;

    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let session_id = SourceSessionRepo::new(&db)
        .upsert(
            "claude_code",
            "scale-session",
            None,
            None,
            Some("scale"),
            Some("Scale Session"),
            None,
            Some("scale-hash"),
            Some("scale-v1"),
        )
        .unwrap();

    let vector: Vec<u8> = [1.0_f32, 0.0_f32]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let mut conn = db.conn();
    let tx = conn.transaction().unwrap();
    tx.execute(
        "INSERT INTO knowledge_item
         (id, source_session_id, project_name, title, category, summary, content, tags,
          confidence, worth_extracting, created_at, updated_at)
         VALUES (?1, ?2, 'scale', 'Scale Item', 'search', 'scale', 'scale', '[]',
                 1.0, 1, '2026-09-15T00:00:00Z', '2026-09-15T00:00:00Z')",
        params!["scale-knowledge", session_id],
    )
    .unwrap();

    {
        let mut chunk_stmt = tx
            .prepare(
                "INSERT INTO knowledge_chunk
                 (id, knowledge_id, heading, chunk_index, token_count, text, content_hash, created_at)
                 VALUES (?1, 'scale-knowledge', NULL, ?2, 2, ?3, ?4, '2026-09-15T00:00:00Z')",
            )
            .unwrap();
        let mut embedding_stmt = tx
            .prepare(
                "INSERT INTO embedding_record
                 (id, chunk_id, model, dimensions, vector, created_at)
                 VALUES (?1, ?2, 'scale-model', 2, ?3, '2026-09-15T00:00:00Z')",
            )
            .unwrap();

        for i in 0..EMBEDDING_COUNT {
            let chunk_id = format!("scale-chunk-{i}");
            chunk_stmt
                .execute(params![
                    &chunk_id,
                    i as i64,
                    format!("scale chunk {i}"),
                    format!("hash-{i}")
                ])
                .unwrap();
            embedding_stmt
                .execute(params![format!("scale-embedding-{i}"), &chunk_id, &vector])
                .unwrap();
        }
    }
    tx.commit().unwrap();
    drop(conn);

    let candidates = KnowledgeRepo::new(&db)
        .load_embedding_candidates("scale-model", &[], VECTOR_CANDIDATE_CAP)
        .expect("bounded candidate query should succeed");

    assert_eq!(candidates.len(), VECTOR_CANDIDATE_CAP);
    assert!(VECTOR_CANDIDATE_CAP < EMBEDDING_COUNT);
}
