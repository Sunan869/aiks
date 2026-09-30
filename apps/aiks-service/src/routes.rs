use crate::{
    error::ApiError,
    weknora::{WeKnoraRoute, WeKnoraSync},
    LocalAuth,
};
use aiks_core::{
    knowledge::ai_assist::AiAssistOperation,
    model::SourceKind,
    search::{SearchCorpus, UnifiedSearchFilter},
    service::{query::Page, RequestContext, ServiceError, ServiceRuntime, SnapshotSubmission},
};
use axum::{
    extract::{
        rejection::{JsonRejection, QueryRejection},
        DefaultBodyLimit, Path, Query, Request, State,
    },
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

const MAX_BODY: usize = 16 * 1024 * 1024;
struct Gate {
    auth: LocalAuth,
    inflight: Semaphore,
    weknora: Option<WeKnoraSync>,
    collector: bool,
}

#[derive(Clone)]
struct WeKnoraExtension(Option<WeKnoraSync>);

pub fn build_router(runtime: Arc<ServiceRuntime>, auth: LocalAuth) -> Router {
    build_router_inner(runtime, auth, None, false)
}

pub fn build_router_with_weknora(
    runtime: Arc<ServiceRuntime>,
    auth: LocalAuth,
    weknora: Option<WeKnoraSync>,
) -> Router {
    build_router_inner(runtime, auth, weknora, false)
}

pub fn build_collector_router(
    runtime: Arc<ServiceRuntime>,
    auth: LocalAuth,
    weknora: WeKnoraSync,
) -> Router {
    build_router_inner(runtime, auth, Some(weknora), true)
}

fn build_router_inner(
    runtime: Arc<ServiceRuntime>,
    auth: LocalAuth,
    weknora: Option<WeKnoraSync>,
    collector: bool,
) -> Router {
    let gate = Arc::new(Gate {
        auth,
        inflight: Semaphore::new(32),
        weknora: weknora.clone(),
        collector,
    });
    let mut router = Router::new()
        .route("/healthz", get(health))
        .route("/api/v1/capabilities", get(capabilities))
        .route("/api/v1/collector/bootstrap", get(collector_bootstrap))
        .route("/api/v1/source-registrations", post(register))
        .route("/api/v1/session-snapshots", post(ingest))
        .route("/api/v1/integrations/weknora/status", get(weknora_status))
        .route("/api/v1/integrations/weknora/retry", post(weknora_retry))
        .route("/api/v1/sessions", get(sessions))
        .route("/api/v1/sessions/{id}", get(session))
        .route("/api/v1/receipts/{id}", get(receipt))
        .route("/api/v1/jobs/{id}", get(job));

    // Collector mode is deliberately not a second knowledge product.
    // WeKnora owns shared knowledge, retrieval/RAG, AI assist and ACLs.
    // Keep these endpoints only on the personal loopback service.
    if !collector {
        router = router
            .route("/api/v1/search", post(search))
            .route("/api/v1/knowledge", get(knowledge_list))
            .route("/api/v1/knowledge/{id}", get(knowledge))
            .route("/api/v1/knowledge/{id}/assist", post(assist));
    }

    router
        .fallback(|| async { ApiError(ServiceError::NotFound) })
        .method_not_allowed_fallback(|| async { ApiError(ServiceError::InvalidInput) })
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(Extension(WeKnoraExtension(weknora)))
        .layer(middleware::from_fn_with_state(gate, guard))
        .with_state(runtime)
}

async fn guard(State(gate): State<Arc<Gate>>, mut request: Request, next: Next) -> Response {
    let public = request.uri().path() == "/healthz";
    if !gate.auth.check(request.headers(), public) {
        return ApiError(ServiceError::Unauthorized).into_response();
    }
    if gate.collector && !public {
        let Some(sync) = gate.weknora.as_ref() else {
            return ApiError(ServiceError::Unavailable).into_response();
        };
        let Some(user_key) = single_header(request.headers(), "x-aiks-weknora-api-key") else {
            return ApiError(ServiceError::Unauthorized).into_response();
        };
        let Some(kb_id) = single_header(request.headers(), "x-aiks-weknora-kb-id") else {
            return ApiError(ServiceError::Unauthorized).into_response();
        };
        let route = match sync.resolve_route(user_key, kb_id).await {
            Ok(route) => route,
            Err(error) => {
                tracing::warn!(error = %error, "Collector WeKnora identity rejected");
                return ApiError(ServiceError::Unauthorized).into_response();
            }
        };
        request.headers_mut().remove("x-aiks-weknora-api-key");
        request.extensions_mut().insert(route);
    }
    if request.uri().path().len() > 4096 {
        return ApiError(ServiceError::InvalidInput).into_response();
    }
    if let Some(query) = request.uri().query() {
        if query.len() > 4096 {
            return ApiError(ServiceError::InvalidInput).into_response();
        }
        let values = serde_urlencoded::from_str::<Vec<(String, String)>>(query);
        if values.as_ref().map_or(true, |pairs| {
            pairs.iter().any(|(key, _)| {
                matches!(
                    key.to_ascii_lowercase().as_str(),
                    "token" | "access_token" | "authorization"
                )
            })
        }) {
            return ApiError(ServiceError::InvalidInput).into_response();
        }
    }
    if request.headers().contains_key(header::CONTENT_ENCODING) {
        return ApiError(ServiceError::InvalidInput).into_response();
    }
    let Ok(_permit) = gate.inflight.try_acquire() else {
        return ApiError(ServiceError::Unavailable).into_response();
    };
    let length = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if length.is_some_and(|size| size > MAX_BODY as u64) {
        // On Windows, abandoning an in-flight body can reset the connection
        // before the client reads 413. Drain only a bounded near-limit body;
        // never deserialize it, wait unboundedly, or drain a huge declaration.
        const DRAIN_LIMIT: usize = MAX_BODY + 64 * 1024;
        if length.is_some_and(|size| size <= DRAIN_LIMIT as u64)
            && !request.headers().contains_key(header::EXPECT)
        {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                axum::body::to_bytes(request.into_body(), DRAIN_LIMIT),
            )
            .await;
        }
        let mut response = ApiError(ServiceError::TooLarge).into_response();
        response.headers_mut().insert(
            header::CONNECTION,
            header::HeaderValue::from_static("close"),
        );
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
        return response;
    }
    let mut response = match tokio::time::timeout(Duration::from_secs(30), next.run(request)).await
    {
        Ok(response) => response,
        Err(_) => ApiError(ServiceError::Unavailable).into_response(),
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        header::HeaderValue::from_static("nosniff"),
    );
    response
}

fn single_header<'a>(headers: &'a axum::http::HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let first = values.next()?.to_str().ok()?;
    if values.next().is_some() || first.is_empty() {
        return None;
    }
    Some(first)
}

fn collector_context(
    runtime: &ServiceRuntime,
    route: Option<&WeKnoraRoute>,
) -> Result<Option<RequestContext>, ApiError> {
    route
        .map(|route| runtime.collector_context(route.principal_id(), route.space_id()))
        .transpose()
        .map_err(ApiError)
}

fn body<T: DeserializeOwned>(input: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    input.map(|Json(value)| value).map_err(|error| {
        ApiError(if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ServiceError::TooLarge
        } else {
            ServiceError::InvalidInput
        })
    })
}
fn page(input: Result<Query<Page>, QueryRejection>) -> Result<Page, ApiError> {
    let Query(value) = input.map_err(|_| ApiError(ServiceError::InvalidInput))?;
    value.validate()?;
    Ok(value)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    source: SourceKind,
    registration_key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchRequest {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    corpora: Vec<SearchCorpus>,
    project: Option<String>,
    source: Option<String>,
}
fn default_limit() -> usize {
    30
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssistRequest {
    operation: AiAssistOperation,
}

async fn health(State(runtime): State<Arc<ServiceRuntime>>) -> Json<Value> {
    let ctx = runtime.context();
    Json(json!({
        "status":"ok",
        "api_version":1,
        "instance_id":ctx.instance_id(),
        "space_id":ctx.space_id()
    }))
}

async fn capabilities(
    State(runtime): State<Arc<ServiceRuntime>>,
    route: Option<Extension<WeKnoraRoute>>,
) -> Json<Value> {
    let mut value = runtime.capabilities();
    if let Some(Extension(route)) = route {
        value["mode"] = json!("collector");
        value["space_id"] = json!(route.space_id());
        value["weknora_tenant_id"] = json!(route.tenant_id());
        value["weknora_knowledge_base_id"] = json!(route.knowledge_base_id());
        value["keyword_search"] = json!(false);
        value["semantic_search"] = json!(false);
        value["ai_assist"] = json!(false);
        value["content_write"] = json!(false);
        value["rag"] = json!(false);
    }
    Json(value)
}

async fn collector_bootstrap(
    State(runtime): State<Arc<ServiceRuntime>>,
    route: Option<Extension<WeKnoraRoute>>,
) -> Result<Json<Value>, ApiError> {
    let Some(Extension(route)) = route else {
        return Err(ApiError(ServiceError::NotFound));
    };
    Ok(Json(json!({
        "api_version":1,
        "instance_id":runtime.context().instance_id(),
        "space_id":route.space_id(),
        "weknora_tenant_id":route.tenant_id(),
        "weknora_knowledge_base_id":route.knowledge_base_id()
    })))
}
async fn register(
    State(runtime): State<Arc<ServiceRuntime>>,
    route: Option<Extension<WeKnoraRoute>>,
    input: Result<Json<Registration>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let input = body(input)?;
    let context = collector_context(&runtime, route.as_ref().map(|value| &value.0))?;
    let id = if let Some(context) = context {
        runtime
            .register_source_for(&context, input.source, input.registration_key)
            .await?
    } else {
        runtime
            .register_source(input.source, input.registration_key)
            .await?
    };
    Ok(Json(json!({"source_registration_id":id})))
}
async fn ingest(
    State(runtime): State<Arc<ServiceRuntime>>,
    Extension(weknora): Extension<WeKnoraExtension>,
    route: Option<Extension<WeKnoraRoute>>,
    input: Result<Json<SnapshotSubmission>, JsonRejection>,
) -> Result<Response, ApiError> {
    let input = body(input)?;
    let session = input.session.clone();
    let context = collector_context(&runtime, route.as_ref().map(|value| &value.0))?;
    let (receipt, created) = if let Some(context) = context {
        runtime.accept_for(&context, input).await?
    } else {
        runtime.accept(input).await?
    };
    if let Some(sync) = weknora.0.as_ref() {
        let queued = if let Some(Extension(route)) = route.as_ref() {
            sync.enqueue_session_for(route, &session, receipt.revision)
                .await
        } else {
            sync.enqueue_session(&session, receipt.revision).await
        };
        if let Err(error) = queued {
            tracing::error!(
                source = session.source.as_str(),
                external_session_id = %session.external_session_id,
                error = %error,
                "Collector saved snapshot but failed to persist WeKnora sync intent; client must retry"
            );
            // The snapshot is idempotent for the same submission ID. Never
            // acknowledge it until its WeKnora delivery intent is durable.
            return Err(ApiError(ServiceError::Unavailable));
        }
        tracing::info!(
            source = session.source.as_str(),
            revision = receipt.revision,
            "Collector accepted snapshot and persisted WeKnora delivery intent"
        );
    }
    Ok((
        if created {
            StatusCode::ACCEPTED
        } else {
            StatusCode::OK
        },
        Json(receipt),
    )
        .into_response())
}
async fn weknora_status(
    Extension(weknora): Extension<WeKnoraExtension>,
    route: Option<Extension<WeKnoraRoute>>,
) -> Result<Json<Value>, ApiError> {
    let Some(sync) = weknora.0.as_ref() else {
        return Ok(Json(json!({"enabled":false,"pending":0,"terminal":0})));
    };
    if let Some(Extension(route)) = route.as_ref() {
        let summary = sync
            .delivery_summary_for(route)
            .await
            .map_err(|_| ApiError(ServiceError::Internal))?;
        return Ok(Json(json!({
            "enabled":true,
            "delivered":summary.delivered,
            "pending":summary.pending,
            "terminal":summary.terminal,
            "failures":summary.failures,
            "recent":summary.recent
        })));
    }
    let (pending, terminal) = sync
        .outbox_counts()
        .await
        .map_err(|_| ApiError(ServiceError::Internal))?;
    Ok(Json(json!({"enabled":true,"pending":pending,"terminal":terminal})))
}

async fn weknora_retry(
    Extension(weknora): Extension<WeKnoraExtension>,
    route: Option<Extension<WeKnoraRoute>>,
) -> Result<Json<Value>, ApiError> {
    let Some(Extension(route)) = route else {
        return Err(ApiError(ServiceError::NotFound));
    };
    let sync = weknora.0.as_ref().ok_or(ApiError(ServiceError::Unavailable))?;
    let queued = sync
        .retry_failed_for(&route)
        .await
        .map_err(|_| ApiError(ServiceError::Internal))?;
    Ok(Json(json!({"queued":queued})))
}

async fn sessions(
    State(r): State<Arc<ServiceRuntime>>,
    route: Option<Extension<WeKnoraRoute>>,
    input: Result<Query<Page>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let page = page(input)?;
    let context = collector_context(&r, route.as_ref().map(|value| &value.0))?;
    Ok(Json(if let Some(context) = context {
        r.sessions_for(&context, page).await?
    } else {
        r.sessions(page).await?
    }))
}
async fn session(
    State(r): State<Arc<ServiceRuntime>>,
    route: Option<Extension<WeKnoraRoute>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let context = collector_context(&r, route.as_ref().map(|value| &value.0))?;
    Ok(Json(if let Some(context) = context {
        r.session_for(&context, id).await?
    } else {
        r.session(id).await?
    }))
}
async fn receipt(
    State(r): State<Arc<ServiceRuntime>>,
    route: Option<Extension<WeKnoraRoute>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let context = collector_context(&r, route.as_ref().map(|value| &value.0))?;
    let value = if let Some(context) = context {
        r.receipt_for(&context, id).await?
    } else {
        r.receipt(id).await?
    };
    Ok(Json(value).into_response())
}
async fn job(
    State(r): State<Arc<ServiceRuntime>>,
    route: Option<Extension<WeKnoraRoute>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let context = collector_context(&r, route.as_ref().map(|value| &value.0))?;
    Ok(Json(if let Some(context) = context {
        r.job_for(&context, id).await?
    } else {
        r.job(id).await?
    }))
}
async fn knowledge_list(
    State(r): State<Arc<ServiceRuntime>>,
    route: Option<Extension<WeKnoraRoute>>,
    input: Result<Query<Page>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let page = page(input)?;
    let context = collector_context(&r, route.as_ref().map(|value| &value.0))?;
    Ok(Json(if let Some(context) = context {
        r.knowledge_list_for(&context, page).await?
    } else {
        r.knowledge_list(page).await?
    }))
}
async fn knowledge(
    State(r): State<Arc<ServiceRuntime>>,
    route: Option<Extension<WeKnoraRoute>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let context = collector_context(&r, route.as_ref().map(|value| &value.0))?;
    let value = if let Some(context) = context {
        r.knowledge_for(&context, id).await?
    } else {
        r.knowledge(id).await?
    };
    Ok(Json(value).into_response())
}
async fn assist(
    State(r): State<Arc<ServiceRuntime>>,
    route: Option<Extension<WeKnoraRoute>>,
    Path(id): Path<String>,
    input: Result<Json<AssistRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let operation = body(input)?.operation;
    let context = collector_context(&r, route.as_ref().map(|value| &value.0))?;
    Ok(Json(if let Some(context) = context {
        r.assist_for(&context, id, operation).await?
    } else {
        r.assist(id, operation).await?
    }))
}
async fn search(
    State(r): State<Arc<ServiceRuntime>>,
    route: Option<Extension<WeKnoraRoute>>,
    input: Result<Json<SearchRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let input = body(input)?;
    let filter = UnifiedSearchFilter {
        corpora: input.corpora,
        project: input.project,
        source: input.source,
    };
    let context = collector_context(&r, route.as_ref().map(|value| &value.0))?;
    Ok(Json(if let Some(context) = context {
        r.search_for(&context, input.query, input.limit, filter)
            .await?
    } else {
        r.search(input.query, input.limit, filter).await?
    }))
}
