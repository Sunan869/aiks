use aiks_core::{
    ai::schema_v3::V3ExtractionResult,
    model::SourceKind,
    pipeline::KnowledgeRepo,
    service::{query, validate_submission, RevisionFence, ServiceStore},
    storage::StateDb,
};
use serde_json::json;
use std::sync::Arc;
#[path = "support/service_fixture.rs"]
mod fixture;

fn result(title: &str) -> V3ExtractionResult {
    serde_json::from_value(
        json!({"session_summary":"Synthetic","knowledge_score":1.0,"worth_extracting":true,
        "items":[{"title":title,"category":"implementation","summary":"Summary","content":"Body",
        "problem":null,"root_causes":null,"solutions":null,"key_commands":null,"key_files":null,
        "decisions":null,"tags":[],"confidence":1.0}]}),
    )
    .unwrap()
}

#[test]
fn a_new_extraction_does_not_relabel_preserved_user_knowledge_as_current() {
    let root = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open_exclusive(&root.path().join("state.db")).unwrap());
    let store = ServiceStore::open(db.clone()).unwrap();
    let ctx = store.local_context();
    let reg = store
        .register_source(&ctx, SourceKind::Continue, "device")
        .unwrap();
    let mut input = fixture::submission(
        ctx.space_id(),
        ctx.instance_id(),
        &reg,
        "one",
        0,
        "FIRST_SOURCE",
    );
    let first = store
        .accept(&ctx, &validate_submission(&input).unwrap())
        .unwrap()
        .0;
    let guard = RevisionFence {
        session_id: first.session_id.parse().unwrap(),
        snapshot_id: first.snapshot_id,
        revision: 1,
    };
    let old = KnowledgeRepo::new(&db)
        .save_items_guarded(
            guard.session_id,
            None,
            &result("Preserved old knowledge"),
            Some(&guard),
        )
        .unwrap()
        .remove(0);
    db.conn().execute("UPDATE knowledge_item SET managed_by='user',siyuan_doc_id='mapped-old-document' WHERE id=?1", [&old]).unwrap();
    input.submission_id = "two".into();
    input.expected_revision = 1;
    input.session.title = Some("New revision".into());
    let second = store
        .accept(&ctx, &validate_submission(&input).unwrap())
        .unwrap()
        .0;
    let guard = RevisionFence {
        session_id: guard.session_id,
        snapshot_id: second.snapshot_id,
        revision: 2,
    };
    let new = KnowledgeRepo::new(&db)
        .save_items_guarded(guard.session_id, None, &result("New draft"), Some(&guard))
        .unwrap()
        .remove(0);
    let old_view = query::knowledge(&db, &ctx, &old).unwrap();
    assert!(
        old_view.stale,
        "preserved data must not inherit another item's generation"
    );
    assert_eq!(old_view.revision, Some(1));
    assert_eq!(old_view.current_revision, 2);
    let new_view = query::knowledge(&db, &ctx, &new).unwrap();
    assert_eq!(new_view.revision, Some(2));
    assert!(!new_view.stale);
}
