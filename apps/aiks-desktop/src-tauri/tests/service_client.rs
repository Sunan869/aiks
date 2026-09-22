//! These tests link the actual Tauri library, not a path-imported client copy.
use aiks_core::{service::SnapshotReceipt, storage::StateDb};
use aiks_desktop_lib::service_client::{
    ClientError, CollectorOutbox, EnqueueOutcome, PendingSubmission, ServiceConnection,
};
use std::path::PathBuf;

#[path = "../../../../crates/aiks-core/tests/support/service_fixture.rs"]
mod fixture;

struct TestRoot(PathBuf);
impl TestRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("aiks-native-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn service_client_native_outbox_replays_without_retargeting_or_rebasing() {
    let root = TestRoot::new();
    let path = root.0.join("collector.db");
    let input = fixture::submission("space-a", "instance-a", "reg-a", "one", 0, "NATIVE_QUEUE");
    let pending = PendingSubmission::new(input).unwrap();
    let outbox = CollectorOutbox::open(&path).unwrap();
    assert!(matches!(
        outbox.enqueue(&pending).unwrap(),
        EnqueueOutcome::Queued(_)
    ));
    assert!(outbox
        .next_for("instance-b", "space-a", 0)
        .unwrap()
        .is_none());
    let claim = outbox
        .next_for("instance-a", "space-a", 0)
        .unwrap()
        .unwrap();
    let payload = serde_json::to_value(claim.pending().submission()).unwrap();
    drop(claim);
    drop(outbox);

    let outbox = CollectorOutbox::open(&path).unwrap();
    let claim = outbox
        .next_for("instance-a", "space-a", 1)
        .unwrap()
        .unwrap();
    assert_eq!(claim.pending().payload_hash(), pending.payload_hash());
    assert_eq!(
        serde_json::to_value(claim.pending().submission()).unwrap(),
        payload
    );
    let receipt = SnapshotReceipt {
        receipt_id: "receipt-1".into(),
        session_id: "12".into(),
        snapshot_id: "snapshot-1".into(),
        revision: 1,
        job_id: "job-1".into(),
        pipeline_run_id: "run-1".into(),
        state: "accepted".into(),
    };
    outbox.record_receipt(&claim, &receipt).unwrap();
    let unchanged = fixture::submission("space-a", "instance-a", "reg-a", "two", 1, "NATIVE_QUEUE");
    assert_eq!(
        outbox
            .enqueue(&PendingSubmission::new(unchanged).unwrap())
            .unwrap(),
        EnqueueOutcome::Unchanged
    );
    assert_eq!(
        outbox.statuses("instance-a", "space-a").unwrap()[0].state,
        "acknowledged"
    );
}

#[test]
fn service_client_native_outbox_refuses_business_storage_and_unsafe_origins() {
    let root = TestRoot::new();
    let path = root.0.join("business.db");
    drop(StateDb::open(&path).unwrap());
    assert!(matches!(
        CollectorOutbox::open(&path),
        Err(ClientError::Storage)
    ));
    let credential = "ab".repeat(32);
    for origin in [
        "http://localhost:1234",
        "http://127.0.0.2:1234",
        "http://127.0.0.1:1234/prefix",
        "http://127.0.0.1:1234?token=unsafe",
        "http://user@127.0.0.1:1234",
    ] {
        assert!(matches!(
            ServiceConnection::local(origin, "instance-a", "space-a", &credential),
            Err(ClientError::InvalidInput)
        ));
    }
    assert!(ServiceConnection::local(
        "http://127.0.0.1:1234",
        "instance-a",
        "space-a",
        &credential
    )
    .is_ok());
}
