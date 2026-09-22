use std::sync::Arc;

use aiks_core::service::{validate_submission, ServiceStore, SnapshotSubmission};
use aiks_core::storage::StateDb;
use aiks_core::SourceKind;

#[path = "support/service_fixture.rs"]
mod fixture;

fn setup() -> (tempfile::TempDir, Arc<StateDb>, Arc<ServiceStore>, String) {
    let root = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open_exclusive(&root.path().join("state.db")).unwrap());
    let store = Arc::new(ServiceStore::open(db.clone()).unwrap());
    let registration = store
        .register_source(&store.local_context(), SourceKind::Continue, "device-1")
        .unwrap();
    (root, db, store, registration)
}

fn request(store: &ServiceStore, registration: &str, id: &str, revision: u32, text: &str) -> SnapshotSubmission {
    let context = store.local_context();
    fixture::submission(context.space_id(), context.instance_id(), registration, id, revision, text)
}

fn count(db: &StateDb, table: &str) -> i64 {
    db.conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn registration_and_upload_retry_are_idempotent() {
    let (_root, db, store, registration) = setup();
    let ctx = store.local_context();
    assert_eq!(registration, store.register_source(&ctx, SourceKind::Continue, "device-1").unwrap());
    let validated = validate_submission(&request(&store, &registration, "u1", 0, "first")).unwrap();
    let (first, inserted) = store.accept(&ctx, &validated).unwrap();
    let (retry, inserted_retry) = store.accept(&ctx, &validated).unwrap();
    assert!(inserted);
    assert!(!inserted_retry);
    assert_eq!(first, retry);
    assert_eq!(first.state, "accepted");
    assert_eq!(first.revision, 1);
    for table in ["source_session", "service_session_binding", "service_session_snapshot", "pipeline_run", "pipeline_job", "service_job_input", "service_ingest_receipt"] {
        assert_eq!(count(&db, table), 1, "{table}");
    }
}

#[test]
fn same_submission_id_cannot_change_content_or_expected_revision() {
    let (_root, _db, store, registration) = setup();
    let ctx = store.local_context();
    let original = request(&store, &registration, "u1", 0, "first");
    store.accept(&ctx, &validate_submission(&original).unwrap()).unwrap();
    for (revision, text) in [(0, "different"), (1, "first")] {
        let changed = request(&store, &registration, "u1", revision, text);
        assert_eq!(store.accept(&ctx, &validate_submission(&changed).unwrap()).unwrap_err().code(), "conflict");
    }
}

#[test]
fn historical_receipt_survives_later_revisions_and_stale_updates_are_rejected() {
    let (_root, db, store, registration) = setup();
    let ctx = store.local_context();
    let first_request = validate_submission(&request(&store, &registration, "u1", 0, "first")).unwrap();
    let (first, _) = store.accept(&ctx, &first_request).unwrap();
    let newer = validate_submission(&request(&store, &registration, "u2", 1, "second")).unwrap();
    let (second, _) = store.accept(&ctx, &newer).unwrap();
    assert_eq!(second.session_id, first.session_id);
    assert_eq!(second.revision, 2);
    assert_ne!(second.snapshot_id, first.snapshot_id);
    assert_ne!(second.pipeline_run_id, first.pipeline_run_id);
    assert_eq!(store.accept(&ctx, &first_request).unwrap(), (first, false));
    let stale = validate_submission(&request(&store, &registration, "u3", 0, "older")).unwrap();
    assert_eq!(store.accept(&ctx, &stale).unwrap_err().code(), "conflict");
    assert_eq!(count(&db, "service_session_snapshot"), 2);
}

#[test]
fn unchanged_content_reuses_work_but_retains_each_submission_receipt() {
    let (_root, db, store, registration) = setup();
    let ctx = store.local_context();
    let (first, _) = store.accept(&ctx, &validate_submission(&request(&store, &registration, "u1", 0, "same")).unwrap()).unwrap();
    let (same, _) = store.accept(&ctx, &validate_submission(&request(&store, &registration, "u2", 1, "same")).unwrap()).unwrap();
    assert_eq!(same.snapshot_id, first.snapshot_id);
    assert_eq!(same.job_id, first.job_id);
    assert_eq!(same.revision, 1);
    assert_ne!(same.receipt_id, first.receipt_id);
    assert_eq!(count(&db, "service_ingest_receipt"), 2);
    assert_eq!(count(&db, "pipeline_job"), 1);
}

#[test]
fn failure_at_any_late_insert_rolls_back_the_whole_acceptance() {
    for table in ["pipeline_job", "service_ingest_receipt"] {
        let (_root, db, store, registration) = setup();
        db.conn().execute_batch(&format!("CREATE TRIGGER stop_accept BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT, 'synthetic failure'); END;")).unwrap();
        let input = validate_submission(&request(&store, &registration, "u1", 0, "first")).unwrap();
        assert!(store.accept(&store.local_context(), &input).is_err());
        for checked in ["source_session", "service_session_binding", "service_session_snapshot", "pipeline_run", "pipeline_job", "service_job_input", "service_ingest_receipt"] {
            assert_eq!(count(&db, checked), 0, "{checked} after {table} failure");
        }
        db.conn().execute_batch("DROP TRIGGER stop_accept").unwrap();
        assert!(store.accept(&store.local_context(), &input).is_ok());
    }
}

#[test]
fn registration_namespaces_and_service_context_are_not_caller_controlled() {
    let (_root, db, store, registration) = setup();
    let (_other_root, _other_db, other, _other_reg) = setup();
    let ctx = store.local_context();
    assert!(store.register_source(&other.local_context(), SourceKind::Continue, "device").is_err());
    let mut input = request(&store, &registration, "u1", 0, "first");
    input.space_id = other.local_context().space_id().to_owned();
    assert!(store.accept(&ctx, &validate_submission(&input).unwrap()).is_err());
    input.space_id = ctx.space_id().to_owned();
    input.session.source = SourceKind::Codex;
    assert!(store.accept(&ctx, &validate_submission(&input).unwrap()).is_err());
    input.session.source = SourceKind::Continue;
    let first = store.accept(&ctx, &validate_submission(&input).unwrap()).unwrap().0;
    let different = store.register_source(&ctx, SourceKind::Continue, "device-2").unwrap();
    input.source_registration_id = different;
    let second = store.accept(&ctx, &validate_submission(&input).unwrap()).unwrap().0;
    assert_ne!(first.session_id, second.session_id);
    assert_eq!(count(&db, "source_session"), 2);
}

#[test]
fn concurrent_retries_get_one_atomic_receipt() {
    let (_root, db, store, registration) = setup();
    let threads: Vec<_> = (0..4).map(|_| {
        let store = store.clone();
        let input = request(&store, &registration, "u1", 0, "first");
        std::thread::spawn(move || store.accept(&store.local_context(), &validate_submission(&input).unwrap()).unwrap())
    }).collect();
    let receipts: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(receipts.iter().filter(|(_, inserted)| *inserted).count(), 1);
    assert!(receipts.iter().all(|(receipt, _)| *receipt == receipts[0].0));
    assert_eq!(count(&db, "service_session_snapshot"), 1);
}

#[test]
fn aba_content_does_not_attach_a_new_revision_to_an_old_running_job() {
    let (_root, db, store, registration) = setup();
    let ctx = store.local_context();
    let first = store.accept(&ctx, &validate_submission(&request(&store, &registration, "u1", 0, "A")).unwrap()).unwrap().0;
    db.conn().execute("UPDATE pipeline_job SET status='RUNNING' WHERE id=?1", [&first.job_id]).unwrap();
    store.accept(&ctx, &validate_submission(&request(&store, &registration, "u2", 1, "B")).unwrap()).unwrap();
    let third = store.accept(&ctx, &validate_submission(&request(&store, &registration, "u3", 2, "A")).unwrap()).unwrap().0;
    assert_eq!(third.revision, 3);
    assert_ne!(first.job_id, third.job_id);
    assert_eq!(count(&db, "service_job_input"), 3);
}

#[test]
fn a_mutated_validated_object_cannot_persist_unchecked_bytes() {
    let (_root, db, store, registration) = setup();
    let mut input = validate_submission(&request(&store, &registration, "u1", 0, "first")).unwrap();
    input.canonical_json = b"{}".to_vec();
    assert_eq!(store.accept(&store.local_context(), &input).unwrap_err().code(), "invalid_input");
    assert_eq!(count(&db, "source_session"), 0);
}
