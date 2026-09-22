#![allow(dead_code)]
use std::{path::PathBuf, sync::Arc, time::Duration};

use aiks_service::{build_router, LocalAuth, ServiceConfig, ServiceRuntime};
use reqwest::{Client, RequestBuilder};
use serde_json::Value;
use tokio::sync::oneshot;

#[path = "../../../../crates/aiks-core/tests/support/service_fixture.rs"]
mod fixture;

pub struct RunningService {
    pub root: tempfile::TempDir,
    pub path: PathBuf,
    pub base: String,
    pub token: String,
    pub instance_id: String,
    pub space_id: String,
    pub client: Client,
    runtime: Arc<ServiceRuntime>,
    stop: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl RunningService {
    pub async fn start() -> Self {
        Self::configured(|_| {}).await
    }

    pub async fn configured(configure: impl FnOnce(&mut ServiceConfig)) -> Self {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state.db");
        let mut config = ServiceConfig::personal(path.clone());
        configure(&mut config);
        config.validate().unwrap();
        let runtime = Arc::new(ServiceRuntime::open(config.runtime_config()).await.unwrap());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bound = listener.local_addr().unwrap();
        let token = "ab".repeat(32);
        let instance_id = runtime.context().instance_id().to_owned();
        let space_id = runtime.context().space_id().to_owned();
        let auth = LocalAuth::new(&token, &instance_id)
            .unwrap()
            .with_authority(bound)
            .unwrap();
        let app = build_router(runtime.clone(), auth);
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { let _ = stopped.await; })
                .await
                .unwrap();
        });
        Self {
            root, path, base: format!("http://{bound}"), token, instance_id, space_id,
            client: Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap(),
            runtime, stop: Some(stop), task: Some(task),
        }
    }

    pub fn auth(&self, request: RequestBuilder) -> RequestBuilder {
        request.bearer_auth(&self.token).header("X-AIKS-Instance-ID", &self.instance_id)
    }

    pub async fn registration(&self) -> String {
        let response = self.auth(self.client.post(format!("{}/api/v1/source-registrations", self.base)))
            .json(&serde_json::json!({"source":"continue","registration_key":"synthetic-device"}))
            .send().await.unwrap();
        assert_eq!(response.status(), 200);
        response.json::<Value>().await.unwrap()["source_registration_id"].as_str().unwrap().to_owned()
    }

    pub async fn ingest_ready(&self, text: &str) -> Value {
        let reg = self.registration().await;
        let input = fixture::submission(&self.space_id, &self.instance_id, &reg, "fixture", 0, text);
        let response = self.auth(self.client.post(format!("{}/api/v1/session-snapshots", self.base)))
            .json(&input).send().await.unwrap();
        assert_eq!(response.status(), 202);
        let receipt: Value = response.json().await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let response = self.auth(self.client.get(format!("{}/api/v1/jobs/{}", self.base, receipt["job_id"].as_str().unwrap())))
                    .send().await.unwrap();
                assert_eq!(response.status(), 200);
                let job: Value = response.json().await.unwrap();
                if job["status"] == "DONE" { break; }
                assert_ne!(job["status"], "FAILED");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        receipt
    }

    pub fn seed_knowledge(&self, receipt: &Value, id: &str, doc: Option<&str>) {
        let db = rusqlite::Connection::open(&self.path).unwrap();
        db.execute("INSERT INTO knowledge_item (id,source_session_id,title,category,summary,content,created_at,updated_at,siyuan_doc_id)
                    VALUES (?1,?2,'Synthetic knowledge','implementation','Synthetic summary','SQLITE_DRAFT_ONLY','test','test',?3)",
            rusqlite::params![id,receipt["session_id"].as_str().unwrap(),doc]).unwrap();
        db.execute("UPDATE service_derived_state SET knowledge_revision=1", []).unwrap();
    }

    pub async fn stop(mut self) {
        self.stop.take().unwrap().send(()).unwrap();
        self.task.take().unwrap().await.unwrap();
        self.runtime.shutdown(Duration::from_secs(2)).await.unwrap();
    }
}

impl Drop for RunningService {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() { let _ = stop.send(()); }
        if let Some(task) = self.task.take() { task.abort(); }
    }
}
