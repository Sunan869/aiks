//! Persistent authentication contracts, with no external identity/network calls.
use std::sync::{Arc, Barrier};

use aiks_core::{
    storage::StateDb,
    team::{
        AuthPolicy, DirectorySnapshot, DirectoryUser, ExternalLogin, LoginStore, Membership,
        OrgRecord, SessionStore, TeamError, TeamStore,
    },
};
use sha2::{Digest, Sha256};

struct Fixture {
    _root: tempfile::TempDir,
    store: Arc<TeamStore>,
    login: Arc<LoginStore>,
    sessions: SessionStore,
}

fn directory(at: u64, active: bool) -> DirectorySnapshot {
    DirectorySnapshot {
        complete: true,
        scope: vec!["1".into()],
        orgs: vec![OrgRecord {
            id: "1".into(),
            parent_id: None,
            name: "Company".into(),
        }],
        users: vec![DirectoryUser {
            external_user_id: "employee-a".into(),
            union_id: "union-a".into(),
            display_name: "Alice".into(),
            active,
        }],
        memberships: vec![Membership {
            user_id: "employee-a".into(),
            org_id: "1".into(),
        }],
        observed_at: at,
    }
}
fn identity() -> ExternalLogin {
    ExternalLogin {
        corp_id: "synthetic-corp".into(),
        external_user_id: "employee-a".into(),
        union_id: "union-a".into(),
        display_name: "Alice".into(),
    }
}
fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(
            TeamStore::bind(
                Arc::new(StateDb::open_exclusive(&root.path().join("team.db")).unwrap()),
                "synthetic-corp",
                "synthetic-client",
            )
            .unwrap(),
        );
        store
            .publish_directory(directory(1000, true), 1000)
            .unwrap();
        let login = Arc::new(LoginStore::new(store.clone(), AuthPolicy::default()).unwrap());
        let sessions = SessionStore::new(store.clone(), AuthPolicy::default()).unwrap();
        Self {
            _root: root,
            store,
            login,
            sessions,
        }
    }
    fn authorize(&self, verifier: &str, now: u64) -> String {
        let start = self.login.start(&digest(verifier), now).unwrap();
        let browser = self
            .login
            .open_browser(&start.attempt_id, start.launch_token.expose(), now)
            .unwrap();
        let claim = self
            .login
            .claim_callback(
                browser.state.expose(),
                browser.nonce.expose(),
                Some(&format!("code-{}", start.attempt_id)),
                now,
            )
            .unwrap();
        self.login.finish_callback(&claim, identity(), now).unwrap();
        start.attempt_id
    }
}

#[test]
fn state_cookie_and_native_verifier_are_bound_and_consumed_exactly_once() {
    let f = Fixture::new();
    let verifier = "11".repeat(32);
    let start = f.login.start(&digest(&verifier), 1001).unwrap();
    assert!(matches!(
        f.login.exchange(&start.attempt_id, &verifier, 1001),
        Err(TeamError::LoginPending)
    ));
    let browser = f
        .login
        .open_browser(&start.attempt_id, start.launch_token.expose(), 1001)
        .unwrap();
    assert!(f
        .login
        .open_browser(&start.attempt_id, start.launch_token.expose(), 1001)
        .is_err());
    assert!(f
        .login
        .claim_callback(
            browser.state.expose(),
            &"22".repeat(32),
            Some("code-one"),
            1001
        )
        .is_err());
    let claim = f
        .login
        .claim_callback(
            browser.state.expose(),
            browser.nonce.expose(),
            Some("code-one"),
            1001,
        )
        .unwrap();
    assert!(f
        .login
        .claim_callback(
            browser.state.expose(),
            browser.nonce.expose(),
            Some("code-one"),
            1001
        )
        .is_err());
    f.login.finish_callback(&claim, identity(), 1001).unwrap();
    assert!(f
        .login
        .exchange(&start.attempt_id, &"33".repeat(32), 1001)
        .is_err());
    let tokens = f
        .login
        .exchange(&start.attempt_id, &verifier, 1001)
        .unwrap();
    let ctx = f
        .sessions
        .authenticate(tokens.access_token.expose(), 1002)
        .unwrap();
    assert_eq!(
        ctx.user_id(),
        f.store.user_by_union("union-a").unwrap().unwrap().id
    );
    assert_eq!(ctx.instance_id(), f.store.instance_id());
    assert_eq!(ctx.company_id(), f.store.company_id());
    assert!(f
        .login
        .exchange(&start.attempt_id, &verifier, 1002)
        .is_err());
    let encoded = format!("{tokens:?} {ctx:?} {start:?} {browser:?}");
    for secret in [
        tokens.access_token.expose(),
        tokens.refresh_token.expose(),
        start.launch_token.expose(),
        browser.nonce.expose(),
    ] {
        assert!(!encoded.contains(secret));
    }
    let conn = f.store.db().conn();
    let hashes: (String, String) = conn.query_row("SELECT access_hash,(SELECT token_hash FROM team_refresh_token LIMIT 1) FROM team_auth_session", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(hashes.0, digest(tokens.access_token.expose()));
    assert_eq!(hashes.1, digest(tokens.refresh_token.expose()));
}

#[test]
fn refresh_rotation_replay_revokes_the_entire_family_and_logout_is_effective() {
    let f = Fixture::new();
    let verifier = "44".repeat(32);
    let id = f.authorize(&verifier, 1001);
    let first = f.login.exchange(&id, &verifier, 1001).unwrap();
    let stale_ctx = f
        .sessions
        .authenticate(first.access_token.expose(), 1001)
        .unwrap();
    let next = f
        .sessions
        .refresh(first.refresh_token.expose(), 1002)
        .unwrap();
    assert_ne!(first.access_token.expose(), next.access_token.expose());
    assert!(f
        .sessions
        .authenticate(first.access_token.expose(), 1002)
        .is_err());
    assert!(f.sessions.identity(&stale_ctx, 1002).is_err());
    assert!(f
        .sessions
        .authenticate(next.access_token.expose(), 1002)
        .is_ok());
    assert!(f
        .sessions
        .refresh(first.refresh_token.expose(), 1003)
        .is_err());
    assert!(f
        .sessions
        .authenticate(next.access_token.expose(), 1003)
        .is_err());
    assert!(f
        .sessions
        .refresh(next.refresh_token.expose(), 1003)
        .is_err());
    let id = f.authorize(&verifier, 1004);
    let second = f.login.exchange(&id, &verifier, 1004).unwrap();
    f.sessions
        .logout(second.access_token.expose(), 1004)
        .unwrap();
    f.sessions
        .logout(second.access_token.expose(), 1004)
        .unwrap();
    assert!(f
        .sessions
        .authenticate(second.access_token.expose(), 1004)
        .is_err());
    assert!(f
        .sessions
        .refresh(second.refresh_token.expose(), 1004)
        .is_err());
}

#[test]
fn inactive_then_reactivated_members_never_revive_old_tokens_or_pending_logins() {
    let f = Fixture::new();
    let verifier = "55".repeat(32);
    let id = f.authorize(&verifier, 1001);
    let tokens = f.login.exchange(&id, &verifier, 1001).unwrap();
    let pending = f.authorize(&verifier, 1001);
    let user = tokens.identity.user_id.clone();
    f.store.mark_member_inactive(&user, 1002).unwrap();
    assert!(f
        .sessions
        .authenticate(tokens.access_token.expose(), 1002)
        .is_err());
    f.store
        .publish_directory(directory(1003, true), 1003)
        .unwrap();
    assert!(f
        .sessions
        .authenticate(tokens.access_token.expose(), 1003)
        .is_err());
    assert!(f
        .sessions
        .refresh(tokens.refresh_token.expose(), 1003)
        .is_err());
    assert!(f.login.exchange(&pending, &verifier, 1003).is_err());
    let fresh = f.authorize(&verifier, 1004);
    let renewed = f.login.exchange(&fresh, &verifier, 1004).unwrap();
    assert_eq!(renewed.identity.user_id, user);
    assert!(f
        .sessions
        .authenticate(renewed.access_token.expose(), 1004)
        .is_ok());
}

#[test]
fn expiry_stale_directory_cross_instance_and_wrong_company_fail_closed() {
    let f = Fixture::new();
    let verifier = "66".repeat(32);
    let expiring = f.login.start(&digest(&verifier), 1001).unwrap();
    assert!(f
        .login
        .open_browser(&expiring.attempt_id, expiring.launch_token.expose(), 1301)
        .is_err());
    let id = f.authorize(&verifier, 1002);
    let tokens = f.login.exchange(&id, &verifier, 1002).unwrap();
    assert!(matches!(
        f.sessions.authenticate(tokens.access_token.expose(), 1901),
        Err(TeamError::DirectoryUnavailable)
    ));
    let other = Fixture::new();
    assert!(other
        .sessions
        .authenticate(tokens.access_token.expose(), 1003)
        .is_err());
    assert!(other.login.exchange(&id, &verifier, 1003).is_err());
    let start = f.login.start(&digest(&verifier), 1003).unwrap();
    let browser = f
        .login
        .open_browser(&start.attempt_id, start.launch_token.expose(), 1003)
        .unwrap();
    let claim = f
        .login
        .claim_callback(
            browser.state.expose(),
            browser.nonce.expose(),
            Some("wrong-company-code"),
            1003,
        )
        .unwrap();
    let mut wrong = identity();
    wrong.corp_id = "another-corp".into();
    assert!(f.login.finish_callback(&claim, wrong, 1003).is_err());
    assert!(f
        .login
        .exchange(&start.attempt_id, &verifier, 1003)
        .is_err());
    assert!(f
        .sessions
        .authenticate(tokens.access_token.expose(), 999)
        .is_err());
}

#[test]
fn concurrent_native_exchanges_issue_only_one_session() {
    let f = Fixture::new();
    let verifier = "77".repeat(32);
    let id = f.authorize(&verifier, 1001);
    let barrier = Arc::new(Barrier::new(3));
    let mut tasks = Vec::new();
    for _ in 0..2 {
        let (login, barrier, verifier, id) = (
            f.login.clone(),
            barrier.clone(),
            verifier.clone(),
            id.clone(),
        );
        tasks.push(std::thread::spawn(move || {
            barrier.wait();
            login.exchange(&id, &verifier, 1002).is_ok()
        }));
    }
    barrier.wait();
    assert_eq!(
        tasks
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|ok| *ok)
            .count(),
        1
    );
    let count: i64 = f
        .store
        .db()
        .conn()
        .query_row("SELECT COUNT(*) FROM team_auth_session", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn session_write_failure_rolls_back_native_exchange_and_preserves_retry() {
    let f = Fixture::new();
    let verifier = "88".repeat(32);
    let id = f.authorize(&verifier, 1001);
    f.store.db().conn().execute_batch("CREATE TRIGGER fail_token BEFORE INSERT ON team_refresh_token BEGIN SELECT RAISE(ABORT,'synthetic'); END;").unwrap();
    assert!(matches!(
        f.login.exchange(&id, &verifier, 1002),
        Err(TeamError::Storage)
    ));
    let count: i64 = f
        .store
        .db()
        .conn()
        .query_row("SELECT COUNT(*) FROM team_auth_session", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    f.store
        .db()
        .conn()
        .execute_batch("DROP TRIGGER fail_token")
        .unwrap();
    assert!(f.login.exchange(&id, &verifier, 1002).is_ok());
}

#[test]
fn cancelled_attempt_and_reused_authorization_code_cannot_issue_tokens() {
    let f = Fixture::new();
    let verifier = "99".repeat(32);
    let start = f.login.start(&digest(&verifier), 1001).unwrap();
    let browser = f
        .login
        .open_browser(&start.attempt_id, start.launch_token.expose(), 1001)
        .unwrap();
    let claim = f
        .login
        .claim_callback(browser.state.expose(), browser.nonce.expose(), None, 1001)
        .unwrap();
    f.login.fail_callback(&claim, 1001).unwrap();
    assert!(f
        .login
        .exchange(&start.attempt_id, &verifier, 1001)
        .is_err());
    for n in 0..2 {
        let start = f.login.start(&digest(&verifier), 1001).unwrap();
        let browser = f
            .login
            .open_browser(&start.attempt_id, start.launch_token.expose(), 1001)
            .unwrap();
        let result = f.login.claim_callback(
            browser.state.expose(),
            browser.nonce.expose(),
            Some("one-use-code"),
            1001,
        );
        assert_eq!(result.is_ok(), n == 0);
    }
}

#[test]
fn workspace_ticket_is_short_lived_one_time_and_contains_only_server_identity() {
    let f = Fixture::new();
    let verifier = "aa".repeat(32);
    let id = f.authorize(&verifier, 1001);
    let tokens = f.login.exchange(&id, &verifier, 1001).unwrap();

    let handoff = f
        .sessions
        .issue_workspace_ticket(tokens.access_token.expose(), 1002)
        .unwrap();
    assert_eq!(handoff.expires_at, 1062);

    let principal = f
        .sessions
        .consume_workspace_ticket(handoff.ticket.expose(), 1002)
        .unwrap();
    assert_eq!(principal.instance_id, f.store.instance_id());
    assert_eq!(principal.company_id, f.store.company_id());
    assert_eq!(principal.user_id, tokens.identity.user_id);
    assert_eq!(principal.space_id, tokens.identity.space_id);
    assert!(!principal.session_id.is_empty());
    assert!(principal.auth_version > 0);

    assert!(f
        .sessions
        .consume_workspace_ticket(handoff.ticket.expose(), 1002)
        .is_err());

    let encoded = format!("{handoff:?} {principal:?}");
    assert!(!encoded.contains(handoff.ticket.expose()));

    let stored: String = f
        .store
        .db()
        .conn()
        .query_row(
            "SELECT token_hash FROM team_workspace_ticket LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, digest(handoff.ticket.expose()));
}

#[test]
fn workspace_ticket_rejects_expiry_logout_and_member_deactivation() {
    let f = Fixture::new();
    let verifier = "bb".repeat(32);

    let id = f.authorize(&verifier, 1001);
    let tokens = f.login.exchange(&id, &verifier, 1001).unwrap();
    let expired = f
        .sessions
        .issue_workspace_ticket(tokens.access_token.expose(), 1002)
        .unwrap();
    assert!(f
        .sessions
        .consume_workspace_ticket(expired.ticket.expose(), expired.expires_at)
        .is_err());

    let logout_ticket = f
        .sessions
        .issue_workspace_ticket(tokens.access_token.expose(), 1003)
        .unwrap();
    f.sessions
        .logout(tokens.access_token.expose(), 1003)
        .unwrap();
    assert!(f
        .sessions
        .consume_workspace_ticket(logout_ticket.ticket.expose(), 1003)
        .is_err());

    let id = f.authorize(&verifier, 1004);
    let renewed = f.login.exchange(&id, &verifier, 1004).unwrap();
    let inactive_ticket = f
        .sessions
        .issue_workspace_ticket(renewed.access_token.expose(), 1004)
        .unwrap();
    f.store
        .mark_member_inactive(&renewed.identity.user_id, 1005)
        .unwrap();
    assert!(f
        .sessions
        .consume_workspace_ticket(inactive_ticket.ticket.expose(), 1005)
        .is_err());
}

#[test]
fn workspace_principal_survives_access_refresh_but_not_session_revocation() {
    let f = Fixture::new();
    let verifier = "cc".repeat(32);
    let id = f.authorize(&verifier, 1001);
    let tokens = f.login.exchange(&id, &verifier, 1001).unwrap();
    let handoff = f
        .sessions
        .issue_workspace_ticket(tokens.access_token.expose(), 1002)
        .unwrap();
    let principal = f
        .sessions
        .consume_workspace_ticket(handoff.ticket.expose(), 1002)
        .unwrap();

    f.sessions
        .validate_workspace_principal(&principal, 1002)
        .unwrap();

    let refreshed = f
        .sessions
        .refresh(tokens.refresh_token.expose(), 1003)
        .unwrap();
    f.sessions
        .validate_workspace_principal(&principal, 1003)
        .unwrap();

    let other = Fixture::new();
    assert!(other
        .sessions
        .validate_workspace_principal(&principal, 1003)
        .is_err());

    f.sessions
        .logout(refreshed.access_token.expose(), 1004)
        .unwrap();
    assert!(f
        .sessions
        .validate_workspace_principal(&principal, 1004)
        .is_err());
}
