//! Compile the exact Desktop client module against the real Service, not a fork.
//! Full Tauri/workspace validation remains an additional desktop delivery gate.
#[path = "../../aiks-desktop/src-tauri/src/service_client/mod.rs"]
pub mod service_client;
mod support;
use aiks_core::{model::SourceKind, service::SnapshotReceipt, storage::StateDb};
use service_client::*;
use support::{fixture, RunningService};

fn pending(id: &str, revision: u32, text: &str) -> PendingSubmission {
    PendingSubmission::new(fixture::submission(
        "space-a",
        "instance-a",
        "reg-a",
        id,
        revision,
        text,
    ))
    .unwrap()
}
fn receipt(revision: u32) -> SnapshotReceipt {
    SnapshotReceipt {
        receipt_id: "receipt-1".into(),
        session_id: "12".into(),
        snapshot_id: "snapshot-1".into(),
        revision,
        job_id: "job-1".into(),
        pipeline_run_id: "run-1".into(),
        state: "accepted".into(),
    }
}

#[test]
fn outbox_persists_exact_submission_and_never_retargets_on_connection_switch() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("collector.db");
    let p = pending("one", 0, "PRIVATE_A_ONLY");
    let o = CollectorOutbox::open(&path).unwrap();
    let queued = o.enqueue(&p).unwrap();
    assert!(matches!(queued, EnqueueOutcome::Queued(_)));
    assert!(o.next_for("instance-b", "space-a", 0).unwrap().is_none());
    assert!(o.next_for("instance-a", "space-b", 0).unwrap().is_none());
    let first = o.next_for("instance-a", "space-a", 0).unwrap().unwrap();
    assert!(o.next_for("instance-a", "space-a", 0).unwrap().is_none());
    let encoded = serde_json::to_value(first.pending().submission()).unwrap();
    drop(first);
    drop(o);
    let o = CollectorOutbox::open(&path).unwrap();
    let recovered = o.next_for("instance-a", "space-a", 1).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(recovered.pending().submission()).unwrap(),
        encoded
    );
    assert_eq!(recovered.pending().payload_hash(), p.payload_hash());
    o.record_receipt(&recovered, &receipt(1)).unwrap();
    assert_eq!(
        o.revision_for("instance-a", "space-a", "reg-a", "synthetic-1")
            .unwrap(),
        1
    );
    assert!(o
        .next_for("instance-a", "space-a", 100_000)
        .unwrap()
        .is_none());
    assert_eq!(
        o.enqueue(&pending("unchanged", 1, "PRIVATE_A_ONLY"))
            .unwrap(),
        EnqueueOutcome::Unchanged
    );
}

#[test]
fn only_one_pending_generation_is_allowed_and_conflicts_are_not_blindly_rebased() {
    let root = tempfile::tempdir().unwrap();
    let o = CollectorOutbox::open(&root.path().join("collector.db")).unwrap();
    let original = pending("one", 0, "BODY_ONE");
    let EnqueueOutcome::Queued(id) = o.enqueue(&original).unwrap() else {
        panic!("new upload")
    };
    assert_eq!(
        o.enqueue(&original).unwrap(),
        EnqueueOutcome::Existing(id.clone())
    );
    assert_eq!(
        o.enqueue(&pending("another-id", 0, "BODY_ONE")).unwrap(),
        EnqueueOutcome::Existing(id)
    );
    assert_eq!(
        o.enqueue(&pending("one", 0, "MUTATED_BODY")).unwrap_err(),
        ClientError::Conflict
    );
    assert_eq!(
        o.enqueue(&pending("new-id", 0, "BODY_TWO")).unwrap_err(),
        ClientError::Busy
    );
    let claim = o.next_for("instance-a", "space-a", 0).unwrap().unwrap();
    o.record_failure(&claim, ClientError::Conflict, 0).unwrap();
    assert!(o
        .next_for("instance-a", "space-a", u64::MAX / 2)
        .unwrap()
        .is_none());
    assert_eq!(
        o.revision_for("instance-a", "space-a", "reg-a", "synthetic-1")
            .unwrap(),
        0
    );
    assert_eq!(
        o.record_receipt(&claim, &receipt(1)).unwrap_err(),
        ClientError::Conflict
    );
}

#[test]
fn retry_backoff_and_receipt_validation_preserve_the_original_payload() {
    let root = tempfile::tempdir().unwrap();
    let o = CollectorOutbox::open(&root.path().join("collector.db")).unwrap();
    let p = pending("one", 0, "RETRY_BODY");
    o.enqueue(&p).unwrap();
    let first = o.next_for("instance-a", "space-a", 0).unwrap().unwrap();
    assert_eq!(
        o.record_receipt(&first, &receipt(99)).unwrap_err(),
        ClientError::InvalidResponse
    );
    o.record_failure(&first, ClientError::Retryable, 0).unwrap();
    assert!(o.next_for("instance-a", "space-a", 1).unwrap().is_none());
    let retry = o
        .next_for("instance-a", "space-a", 1_000_000)
        .unwrap()
        .unwrap();
    assert_eq!(retry.pending().payload_hash(), p.payload_hash());
    assert_eq!(retry.pending().submission().expected_revision, 0);
    assert_eq!(retry.pending().submission().submission_id, "one");
    assert_eq!(
        o.record_receipt(&first, &receipt(1)).unwrap_err(),
        ClientError::Conflict
    );
    o.record_receipt(&retry, &receipt(1)).unwrap();
}

#[test]
fn outbox_refuses_business_databases_and_incomplete_or_mutated_targets() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("business.db");
    let db = StateDb::open(&path).unwrap();
    let before: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get(0))
        .unwrap();
    assert!(CollectorOutbox::open(&path).is_err());
    let after: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
    let mut input = fixture::submission("space-a", "instance-a", "reg-a", "one", 0, "PRIVATE");
    input.complete = false;
    assert!(PendingSubmission::new(input).is_err());
    for url in [
        "http://0.0.0.0:9",
        "http://example.invalid:9",
        "http://127.0.0.1:9/proxy",
        "http://user:pass@127.0.0.1:9",
        "http://127.0.0.1:9?token=secret",
    ] {
        assert!(ServiceConnection::local(url, "instance", "space", &"ab".repeat(32)).is_err());
    }
}

#[tokio::test]
async fn actual_http_retry_after_unrecorded_ack_creates_one_receipt_and_one_job() {
    let s = RunningService::start().await;
    let c = ServiceClient::new(
        ServiceConnection::local(&s.base, &s.instance_id, &s.space_id, &s.token).unwrap(),
    )
    .unwrap();
    assert_eq!(
        c.capabilities().await.unwrap()["instance_id"],
        s.instance_id
    );
    let reg = c
        .register_source(SourceKind::Continue, "outbox-device")
        .await
        .unwrap();
    let p = PendingSubmission::new(fixture::submission(
        &s.space_id,
        &s.instance_id,
        &reg,
        "outbox-one",
        0,
        "OUTBOX_UNIQUE_TEXT",
    ))
    .unwrap();
    let o = CollectorOutbox::open(&s.root.path().join("collector.db")).unwrap();
    o.enqueue(&p).unwrap();
    let first = o.next_for(&s.instance_id, &s.space_id, 0).unwrap().unwrap();
    // The server committed, but the uploader crashed before committing its ack.
    let accepted = c.submit_snapshot(first.pending()).await.unwrap();
    drop(first);
    drop(o);
    let o = CollectorOutbox::open(&s.root.path().join("collector.db")).unwrap();
    let again = o.next_for(&s.instance_id, &s.space_id, 1).unwrap().unwrap();
    let replay = c.submit_snapshot(again.pending()).await.unwrap();
    assert_eq!(accepted, replay);
    o.record_receipt(&again, &replay).unwrap();
    assert!(c.get_job(&replay.job_id).await.unwrap()["status"].is_string());
    let db = rusqlite::Connection::open(&s.path).unwrap();
    for table in [
        "source_session",
        "service_session_snapshot",
        "service_ingest_receipt",
        "pipeline_job",
    ] {
        let n: i64 = db
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }
    drop(db);
    drop(o);
    s.stop().await;
}

#[tokio::test]
async fn wrong_instance_at_same_address_cannot_receive_a_private_snapshot() {
    let s = RunningService::start().await;
    let c = ServiceClient::new(
        ServiceConnection::local(&s.base, "expected-other-instance", &s.space_id, &s.token)
            .unwrap(),
    )
    .unwrap();
    let p = PendingSubmission::new(fixture::submission(
        &s.space_id,
        "expected-other-instance",
        "reg",
        "one",
        0,
        "DO_NOT_UPLOAD",
    ))
    .unwrap();
    assert!(c.submit_snapshot(&p).await.is_err());
    let db = rusqlite::Connection::open(&s.path).unwrap();
    let n: i64 = db
        .query_row("SELECT COUNT(*) FROM service_session_snapshot", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 0);
    drop(db);
    s.stop().await;
}
