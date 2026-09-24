//! Real HTTP auth boundary; only the external identity provider is synthetic.
use aiks_core::{
    storage::StateDb,
    team::{
        provider::ProviderFuture, DirectorySnapshot, DirectoryUser, ExternalLogin,
        IdentityProvider, Membership, OrgRecord, TeamError, TeamStore,
    },
};
use aiks_service::team::{
    auth_routes::{auth_router, AuthService},
    config::{validate_static, TeamSettings},
};
use reqwest::{Client, Response};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
struct Provider(AtomicUsize);
impl IdentityProvider for Provider {
    fn exchange_code<'a>(&'a self, code: &'a str) -> ProviderFuture<'a, ExternalLogin> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::SeqCst);
            if code.starts_with("invalid") {
                return Err(TeamError::Unauthorized);
            }
            if code.starts_with("unavailable") {
                return Err(TeamError::Unavailable);
            }
            Ok(ExternalLogin {
                corp_id: if code.starts_with("foreign") {
                    "different-corp"
                } else {
                    "synthetic-corp"
                }
                .into(),
                external_user_id: "employee-a".into(),
                union_id: "union-a".into(),
                display_name: "Synthetic user".into(),
            })
        })
    }
    fn directory<'a>(&'a self, _: &'a [String]) -> ProviderFuture<'a, DirectorySnapshot> {
        Box::pin(async { Err(TeamError::Unavailable) })
    }
}
struct Server {
    _root: tempfile::TempDir,
    store: Arc<TeamStore>,
    provider: Arc<Provider>,
    base: String,
    client: Client,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn start() -> Self {
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
            .publish_directory(
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
                        active: true,
                    }],
                    memberships: vec![Membership {
                        user_id: "employee-a".into(),
                        org_id: "1".into(),
                    }],
                    observed_at: now(),
                },
                now(),
            )
            .unwrap();
        let mut settings = TeamSettings {
            enabled: true,
            public_base_url: "https://team.example.test".into(),
            ..Default::default()
        };
        settings.dingtalk.enabled = true;
        settings.dingtalk.corp_id = "synthetic-corp".into();
        settings.dingtalk.client_id = "synthetic-client".into();
        settings.dingtalk.redirect_uri =
            "https://team.example.test/api/v1/auth/dingtalk/callback".into();
        settings.directory.root_department_ids = vec!["1".into()];
        let provider = Arc::new(Provider(AtomicUsize::new(0)));
        let auth = Arc::new(
            AuthService::new(
                store.clone(),
                provider.clone(),
                validate_static(&settings).unwrap(),
            )
            .unwrap(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                auth_router(auth).into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        Self {
            _root: root,
            store,
            provider,
            base,
            client,
            task,
        }
    }
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, self.url(path))
            .header("Host", "team.example.test")
    }
    async fn start_login(&self, verifier: &str) -> Value {
        let hash = hex::encode(Sha256::digest(verifier.as_bytes()));
        let response = self
            .request(reqwest::Method::POST, "/api/v1/auth/dingtalk/start")
            .json(&json!({"verifier_hash":hash}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        response.json().await.unwrap()
    }
    async fn browser(&self, start: &Value) -> (String, String) {
        let url = reqwest::Url::parse(start["authorize_url"].as_str().unwrap()).unwrap();
        assert_eq!(
            url.origin().ascii_serialization(),
            "https://team.example.test"
        );
        let response = self
            .request(
                reqwest::Method::GET,
                &format!("{}?{}", url.path(), url.query().unwrap()),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 303);
        let cookie = response.headers()["set-cookie"].to_str().unwrap();
        assert!(
            cookie.contains("HttpOnly")
                && cookie.contains("Secure")
                && cookie.contains("SameSite=Lax")
        );
        let cookie = cookie.split(';').next().unwrap().to_owned();
        let redirect =
            reqwest::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
        assert_eq!(redirect.host_str(), Some("login.dingtalk.com"));
        let state = redirect
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .to_string();
        (state, cookie)
    }
    async fn callback(&self, state: &str, cookie: Option<&str>, code: &str) -> Response {
        let mut request = self
            .request(reqwest::Method::GET, "/api/v1/auth/dingtalk/callback")
            .query(&[("state", state), ("authCode", code)]);
        if let Some(cookie) = cookie {
            request = request.header("Cookie", cookie);
        }
        request.send().await.unwrap()
    }
    async fn exchange(&self, id: &str, verifier: &str) -> Response {
        self.request(reqwest::Method::POST, "/api/v1/auth/dingtalk/exchange")
            .json(&json!({"attempt_id":id,"verifier":verifier}))
            .send()
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn real_http_login_rotates_native_tokens_and_does_not_accept_browser_or_forged_identity() {
    let s = Server::start().await;
    let verifier = "11".repeat(32);
    let start = s.start_login(&verifier).await;
    let id = start["attempt_id"].as_str().unwrap();
    assert_eq!(s.exchange(id, &verifier).await.status(), 202);
    let (state, cookie) = s.browser(&start).await;
    assert_eq!(
        s.callback(&state, None, "legitimate-code").await.status(),
        401
    );
    assert_eq!(s.provider.0.load(Ordering::SeqCst), 0);
    let response = s.callback(&state, Some(&cookie), "legitimate-code").await;
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    let html = response.text().await.unwrap();
    assert!(!html.contains("access_token") && !html.contains("legitimate-code"));
    assert_eq!(
        s.callback(&state, Some(&cookie), "legitimate-code")
            .await
            .status(),
        401
    );
    assert_eq!(s.provider.0.load(Ordering::SeqCst), 1);
    assert_eq!(s.exchange(id, &"22".repeat(32)).await.status(), 401);
    let response = s.exchange(id, &verifier).await;
    assert_eq!(response.status(), 200);
    let tokens: Value = response.json().await.unwrap();
    let access = tokens["access_token"].as_str().unwrap();
    let identity = s
        .request(reqwest::Method::GET, "/api/v1/me")
        .bearer_auth(access)
        .header("X-User-ID", "someone-else")
        .send()
        .await
        .unwrap();
    assert_eq!(identity.status(), 200);
    let identity: Value = identity.json().await.unwrap();
    assert_eq!(identity["user_id"], tokens["identity"]["user_id"]);
    assert_eq!(identity["display_name"], "Alice");
    assert_eq!(
        s.request(reqwest::Method::GET, "/api/v1/me")
            .header("X-User-ID", identity["user_id"].as_str().unwrap())
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(s.exchange(id, &verifier).await.status(), 401);
    let next = s
        .request(reqwest::Method::POST, "/api/v1/auth/refresh")
        .json(&json!({"refresh_token":tokens["refresh_token"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(next.status(), 200);
    let next: Value = next.json().await.unwrap();
    assert_eq!(
        s.request(reqwest::Method::GET, "/api/v1/me")
            .bearer_auth(access)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        s.request(reqwest::Method::GET, "/api/v1/me")
            .bearer_auth(next["access_token"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        s.request(reqwest::Method::POST, "/api/v1/auth/refresh")
            .json(&json!({"refresh_token":tokens["refresh_token"]}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        s.request(reqwest::Method::GET, "/api/v1/me")
            .bearer_auth(next["access_token"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
}

#[tokio::test]
async fn bad_requests_and_rate_limits_cannot_be_bypassed_with_forwarded_headers() {
    let s = Server::start().await;
    let path = "/api/v1/auth/dingtalk/start";
    let hash = "33".repeat(32);
    assert_eq!(
        s.client
            .post(s.url(path))
            .json(&json!({"verifier_hash":hash}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        s.request(reqwest::Method::POST, path)
            .header("Origin", "https://attacker.test")
            .json(&json!({"verifier_hash":hash}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        s.request(reqwest::Method::POST, path)
            .json(&json!({"verifier_hash":hash,"return_url":"https://attacker.test"}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let oversized = s
        .request(reqwest::Method::POST, path)
        .header("Content-Type", "application/json")
        .body("X".repeat(8193))
        .send()
        .await
        .unwrap();
    assert_eq!(oversized.status(), 413);
    let mut limited = false;
    for n in 0..20 {
        let response = s
            .request(reqwest::Method::POST, path)
            .header("X-Forwarded-For", format!("10.0.0.{n}"))
            .json(&json!({"verifier_hash":hash}))
            .send()
            .await
            .unwrap();
        if response.status() == 429 {
            limited = true;
            break;
        }
    }
    assert!(limited);
    assert_eq!(s.provider.0.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn expired_cancelled_foreign_and_inactive_logins_never_become_local_or_admin_users() {
    let s = Server::start().await;
    let verifier = "44".repeat(32);
    for (index, code) in ["foreign-code", "invalid-code", "unavailable-code"]
        .into_iter()
        .enumerate()
    {
        let start = s.start_login(&verifier).await;
        let (state, cookie) = s.browser(&start).await;
        let response = s.callback(&state, Some(&cookie), code).await;
        assert_eq!(
            response.status().as_u16(),
            if index == 2 { 503 } else { 401 }
        );
        assert_eq!(
            s.exchange(start["attempt_id"].as_str().unwrap(), &verifier)
                .await
                .status(),
            401
        );
    }
    let start = s.start_login(&verifier).await;
    let (state, cookie) = s.browser(&start).await;
    let cancelled = s
        .request(reqwest::Method::GET, "/api/v1/auth/dingtalk/callback")
        .query(&[("state", state.as_str()), ("error", "access_denied")])
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(cancelled.status(), 200);
    assert_eq!(
        s.exchange(start["attempt_id"].as_str().unwrap(), &verifier)
            .await
            .status(),
        401
    );
    let start = s.start_login(&verifier).await;
    let (state, cookie) = s.browser(&start).await;
    let user = s.store.user_by_union("union-a").unwrap().unwrap().id;
    s.store.mark_member_inactive(&user, now()).unwrap();
    assert_eq!(
        s.callback(&state, Some(&cookie), "inactive-code")
            .await
            .status(),
        401
    );
    let count: i64 = s
        .store
        .db()
        .conn()
        .query_row("SELECT COUNT(*) FROM team_auth_session", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn credentials_in_query_strings_are_refused_before_starting_an_attempt() {
    let s = Server::start().await;
    for parameter in ["access_token", "token", "%61uthorization", "ACCESS_TOKEN"] {
        let response = s
            .request(
                reqwest::Method::POST,
                &format!("/api/v1/auth/dingtalk/start?{parameter}=PRIVATE_NEEDLE"),
            )
            .json(&json!({"verifier_hash":"55".repeat(32)}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        assert!(!response.text().await.unwrap().contains("PRIVATE_NEEDLE"));
    }
    let count: i64 = s
        .store
        .db()
        .conn()
        .query_row("SELECT COUNT(*) FROM team_login_attempt", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn workspace_ticket_handoff_is_bearer_bound_internal_and_one_time() {
    let s = Server::start().await;
    let verifier = "66".repeat(32);
    let start = s.start_login(&verifier).await;
    let id = start["attempt_id"].as_str().unwrap();
    let (state, cookie) = s.browser(&start).await;
    assert_eq!(
        s.callback(&state, Some(&cookie), "workspace-ticket-code")
            .await
            .status(),
        200
    );
    let response = s.exchange(id, &verifier).await;
    assert_eq!(response.status(), 200);
    let tokens: Value = response.json().await.unwrap();
    let access = tokens["access_token"].as_str().unwrap();

    assert_eq!(
        s.request(reqwest::Method::POST, "/api/v1/workspace/tickets")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );

    let response = s
        .request(reqwest::Method::POST, "/api/v1/workspace/tickets")
        .bearer_auth(access)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let handoff: Value = response.json().await.unwrap();
    let ticket = handoff["ticket"].as_str().unwrap();
    assert_eq!(ticket.len(), 64);
    assert!(handoff["expires_at"].as_u64().unwrap() > now());

    let response = s
        .request(
            reqwest::Method::POST,
            "/api/v1/internal/workspace/tickets/consume",
        )
        .json(&json!({"ticket":ticket}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let principal: Value = response.json().await.unwrap();
    assert_eq!(principal["company_id"], s.store.company_id());
    assert_eq!(principal["user_id"], tokens["identity"]["user_id"]);
    assert_eq!(principal["space_id"], tokens["identity"]["space_id"]);
    assert!(principal["session_id"]
        .as_str()
        .is_some_and(|v| !v.is_empty()));
    assert!(principal["auth_version"].as_u64().unwrap() > 0);
    assert!(principal.get("access_token").is_none());
    assert!(principal.get("refresh_token").is_none());

    assert_eq!(
        s.request(
            reqwest::Method::POST,
            "/api/v1/internal/workspace/principals/validate",
        )
        .json(&principal)
        .send()
        .await
        .unwrap()
        .status(),
        204
    );

    {
        let conn = s.store.db().conn();
        conn.execute(
            "INSERT INTO knowledge_item(
                id,title,category,summary,content,tags,created_at,updated_at,siyuan_doc_id
             ) VALUES ('workspace-doc','Workspace doc','general','','body','[]','test','test','siyuan-doc-1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO team_knowledge_owner(company_id,knowledge_id,owner_user_id)
             VALUES (?1,'workspace-doc',?2)",
            rusqlite::params![s.store.company_id(), principal["user_id"].as_str().unwrap()],
        )
        .unwrap();
    }
    assert_eq!(
        s.request(
            reqwest::Method::POST,
            "/api/v1/internal/workspace/documents/authorize",
        )
        .json(&json!({"principal":principal,"document_id":"siyuan-doc-1"}))
        .send()
        .await
        .unwrap()
        .status(),
        204
    );
    assert_eq!(
        s.request(
            reqwest::Method::POST,
            "/api/v1/internal/workspace/documents/authorize",
        )
        .json(&json!({"principal":principal,"document_id":"not-visible"}))
        .send()
        .await
        .unwrap()
        .status(),
        404
    );
    let filtered: Value = s
        .request(
            reqwest::Method::POST,
            "/api/v1/internal/workspace/documents/filter",
        )
        .json(&json!({"principal":principal,"document_ids":["not-visible","siyuan-doc-1","not-visible"]}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(filtered["document_ids"], json!(["siyuan-doc-1"]));

    assert_eq!(
        s.request(reqwest::Method::POST, "/api/v1/auth/logout")
            .bearer_auth(access)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
    assert_eq!(
        s.request(
            reqwest::Method::POST,
            "/api/v1/internal/workspace/principals/validate",
        )
        .json(&principal)
        .send()
        .await
        .unwrap()
        .status(),
        401
    );
    assert_eq!(
        s.request(
            reqwest::Method::POST,
            "/api/v1/internal/workspace/documents/authorize",
        )
        .json(&json!({"principal":principal,"document_id":"siyuan-doc-1"}))
        .send()
        .await
        .unwrap()
        .status(),
        401
    );

    assert_eq!(
        s.request(
            reqwest::Method::POST,
            "/api/v1/internal/workspace/tickets/consume",
        )
        .json(&json!({"ticket":ticket}))
        .send()
        .await
        .unwrap()
        .status(),
        401
    );
}
