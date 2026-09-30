use aiks_core::model::{
    ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind,
};
use aiks_service::weknora::{WeKnoraSettings, WeKnoraSync};
use axum::{
    extract::{Request, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct FakeWeKnora {
    requests: Mutex<Vec<(Method, String, Value, Option<String>)>>,
    failures_remaining: Mutex<usize>,
    put_not_found_remaining: Mutex<usize>,
    manual_failure_status: Mutex<Option<StatusCode>>,
}

async fn handler(State(state): State<Arc<FakeWeKnora>>, request: Request) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let api_key = request
        .headers()
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let tenant_header = request
        .headers()
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    if method == Method::GET && path == "/api/v1/auth/me" {
        if api_key != "synthetic-user-key" && api_key != "synthetic-user-key-2" {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        let id = if api_key == "synthetic-user-key-2" { 88 } else { 77 };
        return Json(json!({
            "success":true,
            "data":{"tenant":{"id":id}}
        }))
        .into_response();
    }
    if method == Method::GET && path == "/api/v1/knowledge-bases/kb-user" {
        if api_key != "synthetic-user-key" {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        return Json(json!({
            "success":true,
            "data":{"id":"kb-user","tenant_id":77}
        }))
        .into_response();
    }
    if method == Method::GET && path == "/api/v1/knowledge-bases/kb-user-2" {
        if api_key != "synthetic-user-key-2" {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        return Json(json!({
            "success":true,
            "data":{"id":"kb-user-2","tenant_id":88}
        }))
        .into_response();
    }
    if api_key != "synthetic-weknora-key" {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let bytes = axum::body::to_bytes(request.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    state
        .requests
        .lock()
        .unwrap()
        .push((method.clone(), path.clone(), body, tenant_header));

    {
        let mut failures = state.failures_remaining.lock().unwrap();
        if *failures > 0 {
            *failures -= 1;
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    }
    if (method == Method::POST || method == Method::PUT)
        && path.contains("/knowledge/manual")
    {
        if let Some(status) = *state.manual_failure_status.lock().unwrap() {
            return status.into_response();
        }
    }
    if method == Method::PUT {
        let mut missing = state.put_not_found_remaining.lock().unwrap();
        if *missing > 0 {
            *missing -= 1;
            return StatusCode::NOT_FOUND.into_response();
        }
    }

    match (method, path.as_str()) {
        (Method::POST, "/api/v1/knowledge-bases/kb-1/knowledge/manual") => {
            Json(json!({"success":true,"data":{"id":"knowledge-1"}})).into_response()
        }
        (Method::POST, "/api/v1/knowledge-bases/kb-user/knowledge/manual") => {
            Json(json!({"success":true,"data":{"id":"knowledge-user"}})).into_response()
        }
        (Method::POST, "/api/v1/knowledge-bases/kb-user-2/knowledge/manual") => {
            Json(json!({"success":true,"data":{"id":"knowledge-user-2"}})).into_response()
        }
        (Method::PUT, "/api/v1/knowledge/manual/knowledge-1") => {
            Json(json!({"success":true,"data":{"id":"knowledge-1"}})).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn start_fake() -> (String, Arc<FakeWeKnora>, tokio::task::JoinHandle<()>) {
    let state = Arc::new(FakeWeKnora::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn({
        let state = state.clone();
        async move {
            axum::serve(listener, Router::new().fallback(handler).with_state(state))
                .await
                .unwrap();
        }
    });
    (origin, state, task)
}

fn settings(origin: String) -> WeKnoraSettings {
    WeKnoraSettings {
        enabled: true,
        base_url: origin,
        knowledge_base_id: "kb-1".into(),
        api_key_env: "AIKS_WEKNORA_API_KEY".into(),
        channel: "aiks".into(),
        dynamic_targets: false,
    }
}

fn dynamic_settings(origin: String) -> WeKnoraSettings {
    WeKnoraSettings {
        enabled: true,
        base_url: origin,
        knowledge_base_id: String::new(),
        api_key_env: "AIKS_WEKNORA_API_KEY".into(),
        channel: "aiks".into(),
        dynamic_targets: true,
    }
}

fn build(settings: &WeKnoraSettings, database: std::path::PathBuf) -> WeKnoraSync {
    WeKnoraSync::build_with(settings, database, |name| {
        (name == "AIKS_WEKNORA_API_KEY").then(|| "synthetic-weknora-key".into())
    })
    .unwrap()
    .unwrap()
}

fn session(text: &str) -> NormalizedSession {
    NormalizedSession {
        source: SourceKind::Codex,
        external_session_id: "session-1".into(),
        title: Some("Adapter contract".into()),
        project_name: Some("AIKS".into()),
        project_path: None,
        source_path: None,
        started_at: None,
        updated_at: None,
        model: None,
        messages: vec![NormalizedMessage {
            external_id: "message-1".into(),
            parent_id: None,
            role: MessageRole::User,
            created_at: None,
            model: None,
            blocks: vec![ContentBlock::Text { text: text.into() }],
            usage: None,
            metadata: HashMap::new(),
        }],
        usage: None,
        metadata: HashMap::new(),
    }
}

#[tokio::test]
async fn accepted_session_is_created_once_then_updated_by_stable_mapping() {
    let (origin, state, server) = start_fake().await;
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("service.db");
    let settings = settings(origin);
    let sync = build(&settings, database.clone());

    sync.sync_session(&session("FIRST_NEEDLE"), 1)
        .await
        .unwrap();
    sync.sync_session(&session("FIRST_NEEDLE"), 1)
        .await
        .unwrap();

    {
        let requests = state.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, Method::POST);
        assert_eq!(requests[0].2["channel"], "aiks");
        assert_eq!(requests[0].2["status"], "publish");
        assert_eq!(requests[0].2["title"], "Adapter contract");
        assert!(requests[0].2["external_id"]
            .as_str()
            .unwrap()
            .starts_with("aiks-"));
        assert!(requests[0].2["content"]
            .as_str()
            .unwrap()
            .contains("FIRST_NEEDLE"));
    }

    sync.sync_session(&session("SECOND_NEEDLE"), 2)
        .await
        .unwrap();
    {
        let requests = state.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].0, Method::PUT);
        assert_eq!(requests[1].1, "/api/v1/knowledge/manual/knowledge-1");
        assert!(requests[1].2["content"]
            .as_str()
            .unwrap()
            .contains("SECOND_NEEDLE"));
    }

    sync.sync_session(&session("SECOND_NEEDLE"), 3)
        .await
        .unwrap();
    assert_eq!(state.requests.lock().unwrap().len(), 2);

    let reopened = build(&settings, database);
    reopened
        .sync_session(&session("SECOND_NEEDLE"), 4)
        .await
        .unwrap();
    assert_eq!(state.requests.lock().unwrap().len(), 2);
    assert_eq!(reopened.pending_count().await.unwrap(), 0);

    server.abort();
}

#[tokio::test]
async fn failed_delivery_remains_durable_and_can_retry_without_resubmission() {
    let (origin, state, server) = start_fake().await;
    *state.failures_remaining.lock().unwrap() = 1;
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("service.db");
    let settings = settings(origin);
    let sync = build(&settings, database.clone());

    assert!(sync
        .sync_session(&session("RETRY_NEEDLE"), 1)
        .await
        .is_err());
    assert_eq!(sync.pending_count().await.unwrap(), 1);
    assert_eq!(state.requests.lock().unwrap().len(), 1);

    let reopened = build(&settings, database);
    let processed = reopened.retry_pending_now().await.unwrap();
    assert_eq!(processed, 1);
    assert_eq!(reopened.pending_count().await.unwrap(), 0);

    let requests = state.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].0, Method::POST);
    assert_eq!(requests[1].0, Method::POST);
    assert_eq!(requests[0].2["external_id"], requests[1].2["external_id"]);
    drop(requests);

    server.abort();
}

#[tokio::test]
async fn missing_remote_mapping_is_recreated_and_remapped() {
    let (origin, state, server) = start_fake().await;
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("service.db");
    let settings = settings(origin);
    let sync = build(&settings, database);

    sync.sync_session(&session("FIRST"), 1).await.unwrap();
    *state.put_not_found_remaining.lock().unwrap() = 1;
    sync.sync_session(&session("SECOND"), 2).await.unwrap();

    let requests = state.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].0, Method::POST);
    assert_eq!(requests[1].0, Method::PUT);
    assert_eq!(requests[2].0, Method::POST);
    drop(requests);
    assert_eq!(sync.pending_count().await.unwrap(), 0);

    server.abort();
}

#[tokio::test]
async fn dynamic_route_verifies_workspace_and_retries_with_server_platform_key() {
    let (origin, state, server) = start_fake().await;
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("service.db");
    let sync = build(&dynamic_settings(origin), database);

    let route = sync
        .resolve_route("synthetic-user-key", "kb-user")
        .await
        .unwrap();
    assert_eq!(route.tenant_id(), 77);
    assert_eq!(route.knowledge_base_id(), "kb-user");
    assert_eq!(route.principal_id(), "wk-principal-77");
    assert_eq!(route.space_id(), "wk-space-77");

    sync.enqueue_session_for(&route, &session("PRIVATE_NEEDLE"), 1)
        .await
        .unwrap();
    assert_eq!(sync.outbox_counts_for(&route).await.unwrap(), (1, 0));
    assert_eq!(sync.retry_pending_now().await.unwrap(), 1);
    assert_eq!(sync.outbox_counts_for(&route).await.unwrap(), (0, 0));

    let requests = state.requests.lock().unwrap();
    let write = requests
        .iter()
        .find(|request| request.1 == "/api/v1/knowledge-bases/kb-user/knowledge/manual")
        .expect("dynamic target write");
    assert_eq!(write.0, Method::POST);
    assert_eq!(write.3.as_deref(), Some("77"));
    assert!(write.2["content"]
        .as_str()
        .unwrap()
        .contains("PRIVATE_NEEDLE"));
    drop(requests);

    server.abort();
}


// Collector-to-WeKnora status must be scoped to the authenticated route.
// These counts never claim that WeKnora has finished parsing a document.
#[tokio::test]
async fn delivery_summary_and_manual_retry_are_route_scoped() {
    let (origin, state, server) = start_fake().await;
    let root = tempfile::tempdir().unwrap();
    let sync = build(&dynamic_settings(origin), root.path().join("service.db"));
    let alice = sync.resolve_route("synthetic-user-key", "kb-user").await.unwrap();
    let bob = sync.resolve_route("synthetic-user-key-2", "kb-user-2").await.unwrap();

    *state.manual_failure_status.lock().unwrap() = Some(StatusCode::BAD_REQUEST);
    sync.enqueue_session_for(&alice, &session("ALICE"), 1).await.unwrap();
    assert_eq!(sync.retry_pending_now().await.unwrap(), 0);
    let alice_status = sync.delivery_summary_for(&alice).await.unwrap();
    assert_eq!((alice_status.delivered, alice_status.pending, alice_status.terminal), (0, 0, 1));
    assert_eq!(alice_status.failures.len(), 1);
    assert_eq!(alice_status.failures[0].source, "codex");
    assert_eq!(alice_status.failures[0].error_code, "upstream_400");
    assert_eq!(alice_status.recent.len(), 1);
    assert_eq!(alice_status.recent[0].state, "failed");
    assert_eq!(alice_status.recent[0].external_session_id, "session-1");
    assert!(alice_status.recent[0].knowledge_id.is_none());

    *state.manual_failure_status.lock().unwrap() = None;
    sync.enqueue_session_for(&bob, &session("BOB"), 1).await.unwrap();
    assert_eq!(sync.delivery_summary_for(&bob).await.unwrap().pending, 1);
    assert_eq!(sync.retry_failed_for(&alice).await.unwrap(), 1);
    assert_eq!(sync.delivery_summary_for(&alice).await.unwrap().terminal, 0);
    assert_eq!(sync.delivery_summary_for(&bob).await.unwrap().pending, 1);
    assert_eq!(sync.retry_pending_now().await.unwrap(), 2);
    let alice_status = sync.delivery_summary_for(&alice).await.unwrap();
    let bob_status = sync.delivery_summary_for(&bob).await.unwrap();
    assert_eq!((alice_status.delivered, alice_status.pending, alice_status.terminal), (1, 0, 0));
    assert_eq!((bob_status.delivered, bob_status.pending, bob_status.terminal), (1, 0, 0));
    assert_eq!(alice_status.recent[0].state, "delivered");
    assert_eq!(alice_status.recent[0].knowledge_id.as_deref(), Some("knowledge-user"));
    assert_eq!(bob_status.recent[0].state, "delivered");
    assert_eq!(bob_status.recent[0].knowledge_id.as_deref(), Some("knowledge-user-2"));
    assert_eq!(sync.retry_failed_for(&alice).await.unwrap(), 0);
    server.abort();
}
