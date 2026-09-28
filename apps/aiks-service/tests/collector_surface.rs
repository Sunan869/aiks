use aiks_service::{
    build_collector_router,
    weknora::{WeKnoraSettings, WeKnoraSync},
    LocalAuth, ServiceConfig, ServiceRuntime,
};
use axum::{
    extract::Request,
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;

async fn fake_weknora(request: Request) -> Response {
    let path = request.uri().path();
    let api_key = request
        .headers()
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    if api_key != "synthetic-user-key" {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match (request.method(), path) {
        (&Method::GET, "/api/v1/auth/me") => Json(json!({
            "success": true,
            "data": {"tenant": {"id": 77}}
        }))
        .into_response(),
        (&Method::GET, "/api/v1/knowledge-bases/kb-user") => Json(json!({
            "success": true,
            "data": {"id": "kb-user", "tenant_id": 77}
        }))
        .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

#[tokio::test]
async fn collector_exposes_transport_not_knowledge_product_surface() {
    let fake_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fake_origin = format!("http://{}", fake_listener.local_addr().unwrap());
    let fake_server = tokio::spawn(async move {
        axum::serve(fake_listener, Router::new().fallback(fake_weknora))
            .await
            .unwrap();
    });

    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("collector.db");
    let runtime = Arc::new(
        ServiceRuntime::open(ServiceConfig::personal(database.clone()).runtime_config())
            .await
            .unwrap(),
    );
    let instance_id = runtime.context().instance_id().to_owned();
    let token = "a".repeat(64);
    let auth = LocalAuth::new(&token, &instance_id).unwrap();
    let sync = WeKnoraSync::build_with(
        &WeKnoraSettings {
            enabled: true,
            base_url: fake_origin,
            knowledge_base_id: String::new(),
            api_key_env: "AIKS_WEKNORA_API_KEY".into(),
            channel: "aiks".into(),
            dynamic_targets: true,
        },
        database,
        |name| (name == "AIKS_WEKNORA_API_KEY").then(|| "synthetic-platform-key".into()),
    )
    .unwrap()
    .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let service = tokio::spawn(async move {
        axum::serve(listener, build_collector_router(runtime, auth, sync))
            .await
            .unwrap();
    });

    let client = reqwest::Client::new();
    let request = |method: Method, path: &str| {
        client
            .request(method, format!("{base}{path}"))
            .bearer_auth(&token)
            .header("X-AIKS-Instance-ID", &instance_id)
            .header("X-AIKS-WeKnora-API-Key", "synthetic-user-key")
            .header("X-AIKS-WeKnora-KB-ID", "kb-user")
    };

    let caps: Value = request(Method::GET, "/api/v1/capabilities")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(caps["mode"], "collector");
    assert_eq!(caps["keyword_search"], false);
    assert_eq!(caps["semantic_search"], false);
    assert_eq!(caps["ai_assist"], false);
    assert_eq!(caps["content_write"], false);
    assert_eq!(caps["rag"], false);

    for (method, path) in [
        (Method::POST, "/api/v1/search"),
        (Method::GET, "/api/v1/knowledge"),
        (Method::GET, "/api/v1/knowledge/knowledge-1"),
        (Method::POST, "/api/v1/knowledge/knowledge-1/assist"),
    ] {
        let response = request(method, path).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }

    service.abort();
    fake_server.abort();
}
