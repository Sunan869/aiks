use aiks_core::service::SnapshotReceipt;
use aiks_desktop_lib::{
    service_client::{CollectorOutbox, EnqueueOutcome, PendingSubmission, TargetIdentity},
    team_client::{
        connection::{TeamEndpoint, TeamIdentity},
        login::{AuthFuture, AuthStart, AuthTokens, TeamAuthTransport},
        TeamClientManager,
    },
};
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

#[path = "../../../../crates/aiks-core/tests/support/service_fixture.rs"]
mod fixture;

struct TestRoot(PathBuf);
impl TestRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("aiks-team-native-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct FakeTransport {
    exchanges: AtomicUsize,
}
impl FakeTransport {
    fn new() -> Self {
        Self {
            exchanges: AtomicUsize::new(0),
        }
    }
    fn identity(index: usize) -> TeamIdentity {
        match index {
            0 => TeamIdentity {
                instance_id: "instance-1".into(),
                company_id: "company-1".into(),
                user_id: "user-a".into(),
                space_id: "space-a".into(),
                display_name: "User A".into(),
            },
            1 => TeamIdentity {
                instance_id: "instance-1".into(),
                company_id: "company-1".into(),
                user_id: "user-b".into(),
                space_id: "space-b".into(),
                display_name: "User B".into(),
            },
            _ => TeamIdentity {
                instance_id: "instance-2".into(),
                company_id: "company-1".into(),
                user_id: "user-a".into(),
                space_id: "space-c".into(),
                display_name: "User A".into(),
            },
        }
    }
}
impl TeamAuthTransport for FakeTransport {
    fn start<'a>(&'a self, endpoint: &'a TeamEndpoint, _: &'a str) -> AuthFuture<'a, AuthStart> {
        Box::pin(async move {
            let origin = endpoint.origin();
            Ok(AuthStart {
                attempt_id: uuid::Uuid::new_v4().to_string(),
                authorize_url: format!(
                    "{origin}/api/v1/auth/dingtalk/browser?attempt_id={}&launch={}",
                    uuid::Uuid::new_v4(),
                    "a1".repeat(32)
                ),
                expires_at: u64::MAX / 2,
            })
        })
    }
    fn exchange<'a>(
        &'a self,
        _: &'a TeamEndpoint,
        _: &'a str,
        _: &'a str,
    ) -> AuthFuture<'a, AuthTokens> {
        Box::pin(async move {
            let index = self.exchanges.fetch_add(1, Ordering::SeqCst);
            Ok(AuthTokens {
                access_token: format!("{:064x}", index + 1),
                refresh_token: format!("{:064x}", index + 101),
                expires_in: 900,
                identity: Self::identity(index),
            })
        })
    }
    fn refresh<'a>(&'a self, _: &'a TeamEndpoint, _: &'a str) -> AuthFuture<'a, AuthTokens> {
        Box::pin(async move {
            Ok(AuthTokens {
                access_token: "c1".repeat(32),
                refresh_token: "d1".repeat(32),
                expires_in: 900,
                identity: Self::identity(self.exchanges.load(Ordering::SeqCst).saturating_sub(1)),
            })
        })
    }
    fn logout<'a>(&'a self, _: &'a TeamEndpoint, _: &'a str) -> AuthFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn account_switch_never_retargets_pending_team_uploads() {
    let root = TestRoot::new();
    let manager =
        TeamClientManager::with_transport(root.0.join("team"), Arc::new(FakeTransport::new()))
            .unwrap();
    let connection = manager
        .add_connection("https://team.example.test")
        .await
        .unwrap();
    manager
        .begin_login(&connection.connection_id)
        .await
        .unwrap();
    manager
        .finish_login(&connection.connection_id)
        .await
        .unwrap();
    let target_a = manager
        .active_target(&connection.connection_id)
        .await
        .unwrap();
    assert_eq!(target_a.user_id(), "user-a");

    let outbox = CollectorOutbox::open(&root.0.join("collector.db")).unwrap();
    let pending_a = PendingSubmission::new(fixture::submission(
        "space-a",
        "instance-1",
        "reg-a",
        "same",
        0,
        "A_BODY",
    ))
    .unwrap();
    assert!(matches!(
        outbox.enqueue_for(&target_a, &pending_a).unwrap(),
        EnqueueOutcome::Queued(_)
    ));

    manager
        .begin_login(&connection.connection_id)
        .await
        .unwrap();
    manager
        .finish_login(&connection.connection_id)
        .await
        .unwrap();
    let target_b = manager
        .active_target(&connection.connection_id)
        .await
        .unwrap();
    assert_eq!(target_b.user_id(), "user-b");
    assert!(outbox.next_for_target(&target_b, 1).unwrap().is_none());

    let claim = outbox.next_for_target(&target_a, 1).unwrap().unwrap();
    assert_eq!(claim.pending().payload_hash(), pending_a.payload_hash());
    let receipt = SnapshotReceipt {
        receipt_id: "receipt-a".into(),
        session_id: "1".into(),
        snapshot_id: "snapshot-a".into(),
        revision: 1,
        job_id: "job-a".into(),
        pipeline_run_id: "run-a".into(),
        state: "accepted".into(),
    };
    outbox.record_receipt(&claim, &receipt).unwrap();
    assert_eq!(
        outbox
            .revision_for_target(&target_a, "reg-a", "same")
            .unwrap(),
        1
    );
    assert_eq!(
        outbox
            .revision_for_target(&target_b, "reg-a", "same")
            .unwrap(),
        0
    );

    manager.logout(&connection.connection_id).await.unwrap();
    assert_eq!(manager.statuses().await[0].state, "signed_out");
}

#[test]
fn legacy_v1_rows_migrate_as_personal_and_cannot_be_claimed_as_team() {
    let root = TestRoot::new();
    let path = root.0.join("collector-v1.db");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "PRAGMA application_id=1095322435; PRAGMA user_version=1;
         CREATE TABLE collector_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
         CREATE TABLE collector_upload(id TEXT PRIMARY KEY,instance_id TEXT NOT NULL,space_id TEXT NOT NULL,
         registration_id TEXT NOT NULL,upstream_id TEXT NOT NULL,submission_id TEXT NOT NULL,source TEXT NOT NULL,
         expected_revision INTEGER NOT NULL,body BLOB,request_hash TEXT NOT NULL,content_hash TEXT NOT NULL,
         state TEXT NOT NULL CHECK(state IN ('pending','inflight','blocked','acknowledged')),
         attempt INTEGER NOT NULL DEFAULT 0,due_ms INTEGER NOT NULL DEFAULT 0,lease_id TEXT,lease_until INTEGER,
         error_code TEXT,receipt_json TEXT,UNIQUE(instance_id,space_id,registration_id,upstream_id,submission_id));
         CREATE INDEX collector_pending ON collector_upload(instance_id,space_id,state,due_ms);
         CREATE UNIQUE INDEX collector_one_generation ON collector_upload(instance_id,space_id,registration_id,upstream_id) WHERE state IN ('pending','inflight','blocked');
         CREATE TABLE collector_cursor(instance_id TEXT NOT NULL,space_id TEXT NOT NULL,registration_id TEXT NOT NULL,
         upstream_id TEXT NOT NULL,revision INTEGER NOT NULL,content_hash TEXT NOT NULL,
         PRIMARY KEY(instance_id,space_id,registration_id,upstream_id));",
    )
    .unwrap();
    let pending = PendingSubmission::new(fixture::submission(
        "space-local",
        "instance-local",
        "reg",
        "same",
        0,
        "LOCAL",
    ))
    .unwrap();
    let body = serde_json::to_vec(pending.submission()).unwrap();
    db.execute(
        "INSERT INTO collector_upload(id,instance_id,space_id,registration_id,upstream_id,submission_id,source,expected_revision,body,request_hash,content_hash,state)
         VALUES ('legacy','instance-local','space-local','reg','same','sub','continue',0,?1,?2,?3,'pending')",
        rusqlite::params![body,pending.payload_hash(),pending.content_hash()],
    )
    .unwrap();
    drop(db);

    let outbox = CollectorOutbox::open(&path).unwrap();
    let team = TargetIdentity::team("instance-local", "company", "user", "space-local").unwrap();
    assert!(outbox.next_for_target(&team, 0).unwrap().is_none());
    assert!(outbox
        .next_for("instance-local", "space-local", 0)
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn unsafe_origins_and_cross_origin_browser_handoffs_are_rejected() {
    for value in [
        "http://team.example.test",
        "https://user@team.example.test",
        "https://team.example.test/path",
        "https://team.example.test?token=unsafe",
        "https://team.example.test/#fragment",
    ] {
        assert!(TeamEndpoint::parse(value).is_err(), "{value}");
    }
    let endpoint = TeamEndpoint::parse("https://team.example.test").unwrap();
    assert!(endpoint
        .validate_browser_handoff(
            "https://evil.example/api/v1/auth/dingtalk/browser?attempt_id=x&launch=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .is_err());
    assert!(endpoint
        .validate_browser_handoff(
            "https://team.example.test/api/v1/auth/dingtalk/browser?attempt_id=x&token=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .is_err());
}

#[tokio::test]
async fn persisted_team_metadata_never_contains_native_tokens() {
    let root = TestRoot::new();
    let manager =
        TeamClientManager::with_transport(root.0.join("team"), Arc::new(FakeTransport::new()))
            .unwrap();
    let connection = manager
        .add_connection("https://team.example.test")
        .await
        .unwrap();
    manager
        .begin_login(&connection.connection_id)
        .await
        .unwrap();
    manager
        .finish_login(&connection.connection_id)
        .await
        .unwrap();
    let forbidden = [format!("{:064x}", 1), format!("{:064x}", 101)];
    let mut stack = vec![root.0.clone()];
    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let text = String::from_utf8_lossy(&bytes);
                for secret in &forbidden {
                    assert!(
                        !text.contains(secret),
                        "secret persisted in {}",
                        path.display()
                    );
                }
            }
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
#[test]
fn native_credential_store_survives_reopen_without_plaintext() {
    use aiks_desktop_lib::team_client::credentials::{CredentialStore, StoredCredential};
    let root = TestRoot::new();
    let path = root.0.join("credentials-native");
    let value = StoredCredential {
        access_token: "e1".repeat(32),
        refresh_token: "f1".repeat(32),
        expires_at: u64::MAX / 2,
        identity: FakeTransport::identity(0),
    };
    {
        let store = CredentialStore::new(path.clone()).unwrap();
        store.store("connection-native", "user-a", &value).unwrap();
    }
    let reopened = CredentialStore::new(path.clone()).unwrap();
    let loaded = reopened
        .load("connection-native", "user-a")
        .unwrap()
        .unwrap();
    assert_eq!(loaded.identity, value.identity);
    assert_eq!(loaded.access_token, value.access_token);
    for entry in std::fs::read_dir(&path).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(&value.access_token));
        assert!(!text.contains(&value.refresh_token));
    }
    reopened.delete("connection-native", "user-a").unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn memory_fallback_never_creates_plaintext_credentials() {
    use aiks_desktop_lib::team_client::credentials::{CredentialStore, StoredCredential};
    let root = TestRoot::new();
    let path = root.0.join("credentials-linux");
    let value = StoredCredential {
        access_token: "e2".repeat(32),
        refresh_token: "f2".repeat(32),
        expires_at: u64::MAX / 2,
        identity: FakeTransport::identity(0),
    };
    let store = CredentialStore::new(path.clone()).unwrap();
    store.store("connection-linux", "user-a", &value).unwrap();
    for entry in std::fs::read_dir(&path).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(&value.access_token));
        assert!(!text.contains(&value.refresh_token));
    }
}
