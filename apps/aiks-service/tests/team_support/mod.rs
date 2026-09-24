#![allow(dead_code)]
use aiks_core::{
    service::ServiceRuntime,
    storage::StateDb,
    team::{
        AuthPolicy, DirectorySnapshot, DirectoryUser, ExternalLogin, LoginStore, Membership,
        OrgRecord, SessionStore, TeamStore,
    },
};
use aiks_service::{team::business_routes::business_router, ServiceConfig};
use reqwest::{Client, RequestBuilder};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::oneshot;

#[path = "../../../../crates/aiks-core/tests/support/service_fixture.rs"]
pub mod fixture;

pub struct TeamTestService {
    pub root: tempfile::TempDir,
    pub path: PathBuf,
    pub store: Arc<TeamStore>,
    pub sessions: Arc<SessionStore>,
    pub runtime: Arc<ServiceRuntime>,
    pub base: String,
    pub client: Client,
    pub owner_id: String,
    pub reader_id: String,
    pub child_id: String,
    pub research_id: String,
    pub owner_space: String,
    pub reader_space: String,
    pub child_space: String,
    pub owner_token: String,
    pub reader_token: String,
    pub child_token: String,
    stop: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl TeamTestService {
    pub async fn start() -> Self {
        Self::configured(|_| {}).await
    }

    pub async fn configured(configure: impl FnOnce(&mut ServiceConfig)) -> Self {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("team.db");
        let db = Arc::new(StateDb::open_exclusive(&path).unwrap());
        let store = Arc::new(TeamStore::bind(db, "synthetic-corp", "synthetic-client").unwrap());
        let at = now();
        store.publish_directory(directory(at), at).unwrap();
        let owner = store.user_by_union("union-owner").unwrap().unwrap();
        let reader = store.user_by_union("union-reader").unwrap().unwrap();
        let child = store.user_by_union("union-child").unwrap().unwrap();
        let research_id = store.org_id_by_external("research").unwrap().unwrap();
        let policy = AuthPolicy::default();
        let login = LoginStore::new(store.clone(), policy).unwrap();
        let owner_token = issue(&login, "owner", "employee-owner", "union-owner", at);
        let reader_token = issue(&login, "reader", "employee-reader", "union-reader", at);
        let child_token = issue(&login, "child", "employee-child", "union-child", at);
        let sessions = Arc::new(SessionStore::new(store.clone(), policy).unwrap());
        let mut config = ServiceConfig::personal(path.clone());
        configure(&mut config);
        let runtime = Arc::new(
            ServiceRuntime::open_team_bound(store.clone(), config.runtime_config()).unwrap(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bound = listener.local_addr().unwrap();
        let app = business_router(runtime.clone(), sessions.clone());
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
        });
        Self {
            root,
            path,
            store,
            sessions,
            runtime,
            base: format!("http://{bound}"),
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            owner_id: owner.id,
            reader_id: reader.id,
            child_id: child.id,
            research_id,
            owner_space: owner.private_space_id,
            reader_space: reader.private_space_id,
            child_space: child.private_space_id,
            owner_token,
            reader_token,
            child_token,
            stop: Some(stop),
            task: Some(task),
        }
    }

    pub fn auth(&self, token: &str, request: RequestBuilder) -> RequestBuilder {
        request.bearer_auth(token)
    }

    pub async fn registration(&self, token: &str) -> String {
        let response = self
            .auth(
                token,
                self.client
                    .post(format!("{}/api/v1/source-registrations", self.base)),
            )
            .json(&serde_json::json!({"source":"continue","registration_key":"same-device"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        response.json::<serde_json::Value>().await.unwrap()["source_registration_id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub async fn ingest(&self, token: &str, space: &str, text: &str) -> serde_json::Value {
        let registration = self.registration(token).await;
        let input = fixture::submission(
            space,
            self.store.instance_id(),
            &registration,
            "same-upstream-session",
            0,
            text,
        );
        let response = self
            .auth(
                token,
                self.client
                    .post(format!("{}/api/v1/session-snapshots", self.base)),
            )
            .json(&input)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 202);
        response.json().await.unwrap()
    }

    pub async fn wait_job(&self, token: &str, job: &str) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let response = self
                    .auth(
                        token,
                        self.client.get(format!("{}/api/v1/jobs/{job}", self.base)),
                    )
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), 200);
                let value: serde_json::Value = response.json().await.unwrap();
                if value["status"] == "DONE" {
                    return;
                }
                assert_ne!(value["status"], "FAILED");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    pub async fn stop(mut self) {
        self.stop.take().unwrap().send(()).unwrap();
        self.task.take().unwrap().await.unwrap();
        self.runtime.shutdown(Duration::from_secs(2)).await.unwrap();
    }
}

impl Drop for TeamTestService {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn issue(
    login: &LoginStore,
    label: &str,
    external_user_id: &str,
    union_id: &str,
    at: u64,
) -> String {
    let verifier = hex::encode(Sha256::digest(format!("verifier:{label}").as_bytes()));
    let hash = hex::encode(Sha256::digest(verifier.as_bytes()));
    let start = login.start(&hash, at).unwrap();
    let browser = login
        .open_browser(&start.attempt_id, start.launch_token.expose(), at)
        .unwrap();
    let code = format!("synthetic-code-{label}");
    let claim = login
        .claim_callback(
            browser.state.expose(),
            browser.nonce.expose(),
            Some(&code),
            at,
        )
        .unwrap();
    login
        .finish_callback(
            &claim,
            ExternalLogin {
                corp_id: "synthetic-corp".into(),
                external_user_id: external_user_id.into(),
                union_id: union_id.into(),
                display_name: label.into(),
            },
            at,
        )
        .unwrap();
    login
        .exchange(&start.attempt_id, &verifier, at)
        .unwrap()
        .access_token
        .expose()
        .to_owned()
}

fn directory(at: u64) -> DirectorySnapshot {
    DirectorySnapshot {
        complete: true,
        scope: vec!["root".into()],
        observed_at: at,
        orgs: vec![
            OrgRecord {
                id: "root".into(),
                parent_id: None,
                name: "Company".into(),
            },
            OrgRecord {
                id: "research".into(),
                parent_id: Some("root".into()),
                name: "Research".into(),
            },
            OrgRecord {
                id: "backend".into(),
                parent_id: Some("research".into()),
                name: "Backend".into(),
            },
        ],
        users: vec![
            DirectoryUser {
                external_user_id: "employee-owner".into(),
                union_id: "union-owner".into(),
                display_name: "Owner".into(),
                active: true,
            },
            DirectoryUser {
                external_user_id: "employee-reader".into(),
                union_id: "union-reader".into(),
                display_name: "Reader".into(),
                active: true,
            },
            DirectoryUser {
                external_user_id: "employee-child".into(),
                union_id: "union-child".into(),
                display_name: "Child".into(),
                active: true,
            },
        ],
        memberships: vec![
            Membership {
                user_id: "employee-owner".into(),
                org_id: "root".into(),
            },
            Membership {
                user_id: "employee-reader".into(),
                org_id: "research".into(),
            },
            Membership {
                user_id: "employee-child".into(),
                org_id: "backend".into(),
            },
        ],
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
