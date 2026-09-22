use axum::{
    http::{HeaderMap, StatusCode},
    response::Redirect,
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::Semaphore;
mod support;
use support::RunningService;

struct Upstream {
    base: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Upstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn upstream(app: Router) -> Upstream {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Upstream { base, task }
}

#[tokio::test]
async fn published_body_comes_from_fixed_content_api_and_draft_is_explicit() {
    let remote = upstream(Router::new().route(
        "/api/block/getBlockKramdown",
        post(|headers: HeaderMap, Json(value): Json<Value>| async move {
            assert_eq!(headers["authorization"], "Token synthetic-content-secret");
            assert_eq!(value, json!({"id":"mapped-doc"}));
            Json(json!({"code":0,"data":{"kramdown":"# CANONICAL_REMOTE_BODY"}}))
        }),
    ))
    .await;
    let s = RunningService::configured(|config| {
        config.siyuan.base_url = remote.base.clone();
        config.siyuan.token = "synthetic-content-secret".into();
    })
    .await;
    let receipt = s.ingest_ready("Synthetic source").await;
    s.seed_knowledge(&receipt, "published", Some("mapped-doc"));
    s.seed_knowledge(&receipt, "draft", None);
    let response = s
        .auth(
            s.client
                .get(format!("{}/api/v1/knowledge/published", s.base)),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let text = response.text().await.unwrap();
    assert!(text.contains("CANONICAL_REMOTE_BODY"));
    for secret in [
        "SQLITE_DRAFT_ONLY",
        "mapped-doc",
        "synthetic-content-secret",
        &remote.base,
    ] {
        assert!(!text.contains(secret));
    }
    let draft: Value = s
        .auth(s.client.get(format!("{}/api/v1/knowledge/draft", s.base)))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(draft["content_state"], "draft");
    assert_eq!(draft["content"], "SQLITE_DRAFT_ONLY");
    s.stop().await;
}

#[tokio::test]
async fn unavailable_published_body_is_not_replaced_with_the_sqlite_projection() {
    let remote = upstream(
        Router::new()
            .fallback(|| async { (StatusCode::SERVICE_UNAVAILABLE, "PRIVATE_UPSTREAM_ERROR") }),
    )
    .await;
    let s = RunningService::configured(|config| {
        config.siyuan.base_url = remote.base.clone();
    })
    .await;
    let receipt = s.ingest_ready("Synthetic source").await;
    s.seed_knowledge(&receipt, "published", Some("mapped-doc"));
    let response = s
        .auth(
            s.client
                .get(format!("{}/api/v1/knowledge/published", s.base)),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], "content_unavailable");
    assert!(!body.to_string().contains("PRIVATE_UPSTREAM_ERROR"));
    assert!(!body.to_string().contains("SQLITE_DRAFT_ONLY"));
    s.stop().await;
}

#[tokio::test]
async fn disabled_ai_does_not_contact_content_or_model_services() {
    let seen = Arc::new(AtomicUsize::new(0));
    let count = seen.clone();
    let remote = upstream(Router::new().fallback(move || {
        count.fetch_add(1, Ordering::SeqCst);
        async { Json(json!({"code":0,"data":{"kramdown":"# Canonical content"}})) }
    }))
    .await;
    let s = RunningService::configured(|config| {
        config.siyuan.base_url = remote.base.clone();
        config.ai.base_url = remote.base.clone();
        config.ai.enabled = false;
    })
    .await;
    let receipt = s.ingest_ready("Synthetic source").await;
    s.seed_knowledge(&receipt, "published", Some("mapped-doc"));
    let response = s
        .auth(
            s.client
                .post(format!("{}/api/v1/knowledge/published/assist", s.base)),
        )
        .json(&json!({"operation":"summary"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"]["code"],
        "ai_disabled"
    );
    assert_eq!(seen.load(Ordering::SeqCst), 0);
    let response = s
        .auth(
            s.client
                .post(format!("{}/api/v1/knowledge/nonexistent/assist", s.base)),
        )
        .json(&json!({"operation":"summary"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    s.stop().await;
}

#[tokio::test]
async fn content_redirects_never_forward_the_internal_credential() {
    let seen = Arc::new(AtomicUsize::new(0));
    let count = seen.clone();
    let target = upstream(Router::new().fallback(move || {
        count.fetch_add(1, Ordering::SeqCst);
        async { Json(json!({"code":0,"data":{"kramdown":"not permitted"}})) }
    }))
    .await;
    let url = target.base.clone();
    let remote = upstream(Router::new().fallback(move || {
        let target = url.clone();
        async move { Redirect::temporary(&target) }
    }))
    .await;
    let s = RunningService::configured(|config| {
        config.siyuan.base_url = remote.base.clone();
        config.siyuan.token = "synthetic-content-secret".into();
    })
    .await;
    let receipt = s.ingest_ready("Synthetic source").await;
    s.seed_knowledge(&receipt, "published", Some("mapped-doc"));
    let response = s
        .auth(
            s.client
                .get(format!("{}/api/v1/knowledge/published", s.base)),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    assert_eq!(seen.load(Ordering::SeqCst), 0);
    s.stop().await;
}

#[tokio::test]
async fn content_read_rechecks_visibility_without_holding_a_database_lock_across_await() {
    let seen = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    let (ready, gate) = (seen.clone(), release.clone());
    let remote = upstream(Router::new().fallback(move || {
        let (ready, gate) = (ready.clone(), gate.clone());
        async move {
            ready.add_permits(1);
            gate.acquire().await.unwrap().forget();
            Json(json!({"code":0,"data":{"kramdown":"REVOKED_CONTENT"}}))
        }
    }))
    .await;
    let s = RunningService::configured(|config| {
        config.siyuan.base_url = remote.base.clone();
    })
    .await;
    let receipt = s.ingest_ready("Synthetic source").await;
    s.seed_knowledge(&receipt, "published", Some("mapped-doc"));
    let request = s.auth(
        s.client
            .get(format!("{}/api/v1/knowledge/published", s.base)),
    );
    let task = tokio::spawn(async move { request.send().await.unwrap() });
    tokio::time::timeout(Duration::from_secs(2), seen.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();
    let other = tokio::time::timeout(
        Duration::from_secs(1),
        s.auth(s.client.get(format!("{}/api/v1/sessions", s.base)))
            .send(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(other.status(), 200);
    rusqlite::Connection::open(&s.path)
        .unwrap()
        .execute(
            "UPDATE knowledge_item SET status='archived' WHERE id='published'",
            [],
        )
        .unwrap();
    release.add_permits(1);
    let response = task.await.unwrap();
    assert_eq!(response.status(), 404);
    assert!(!response.text().await.unwrap().contains("REVOKED_CONTENT"));
    s.stop().await;
}
