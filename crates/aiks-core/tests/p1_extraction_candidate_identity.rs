use aiks_core::{
    storage::{SourceSessionRepo, StateDb},
    sync::{ExtractionCandidate, SyncStats},
};

#[test]
fn extraction_candidate_preserves_exact_session_identity_across_provider_collision() {
    let dir = tempfile::tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let repo = SourceSessionRepo::new(&db);

    let claude_id = repo
        .upsert(
            "claude_code",
            "shared-external-id",
            Some("claude.jsonl"),
            None,
            Some("claude-project"),
            Some("Claude session"),
            None,
            Some("claude-hash"),
            Some("p1-v1"),
        )
        .unwrap();
    let codex_id = repo
        .upsert(
            "codex",
            "shared-external-id",
            Some("codex.jsonl"),
            None,
            Some("codex-project"),
            Some("Codex session"),
            None,
            Some("codex-hash"),
            Some("p1-v1"),
        )
        .unwrap();

    assert_ne!(claude_id, codex_id);

    let stats = SyncStats {
        extraction_candidates: vec![ExtractionCandidate {
            session_id: claude_id,
            source: "claude_code".to_string(),
            external_session_id: "shared-external-id".to_string(),
        }],
        ..Default::default()
    };

    assert_eq!(stats.extraction_candidates.len(), 1);
    let candidate = &stats.extraction_candidates[0];
    assert_eq!(candidate.session_id, claude_id);
    assert_ne!(candidate.session_id, codex_id);

    let (resolved_source, resolved_external_id): (String, String) = db
        .conn()
        .query_row(
            "SELECT source, external_session_id FROM source_session WHERE id = ?1",
            [candidate.session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(resolved_source, "claude_code");
    assert_eq!(resolved_external_id, "shared-external-id");
    assert_eq!(candidate.source, resolved_source);
    assert_eq!(candidate.external_session_id, resolved_external_id);

    let json = serde_json::to_value(&stats).unwrap();
    assert_eq!(
        json["extraction_candidates"][0]["session_id"],
        serde_json::json!(claude_id)
    );
    assert_eq!(
        json["extraction_candidates"][0]["source"],
        serde_json::json!("claude_code")
    );

    let decoded: SyncStats = serde_json::from_value(json).unwrap();
    assert_eq!(decoded.extraction_candidates[0], *candidate);
}
