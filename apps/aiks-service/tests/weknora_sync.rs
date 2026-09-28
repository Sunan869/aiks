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
    requests: Mutex<Vec<(Method, String, Value)>>,
    failures_remaining: Mutex<usize>,
}

async fn handler(State(state): State<Arc<FakeWeKnora>>, request: Request) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    if request
        .headers()
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        != Some("synthetic-weknora-key")
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let bytes = axum::body::to_bytes(request.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    state
        .requests
        .lock()
        .unwrap()
        .push((method.clone(), path.clone(), body));

    {
        let mut failures = state.failures_remaining.lock().unwrap();
        if *failures > 0 {
            *failures -= 1;
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    }

    match (method, path.as_str()) {
        (Method::POST, "/api/v1/knowledge-bases/kb-1/knowledge/manual") => {
            Json(json!({"success":true,"data":{"id":"knowledge-1"}})).into_response()
        }
        (Method::PUT, "/api/v1/knowledge/manual/knowledge-1") => {
            Json(json!({"success":true,"data":{"id":"knowledge-1"}})).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn start_fake() -> (String, Arc<FakeWeKnora>, tokio::task::JoinHandle<()>) {
    let state = Arc::new(FakeWeKnora::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
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

    assert!(sync.sync_session(&session("RETRY_NEEDLE"), 1).await.is_err());
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
