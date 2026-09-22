use crate::{error::ApiError, LocalAuth};
use aiks_core::{
    knowledge::ai_assist::AiAssistOperation,
    model::SourceKind,
    search::{SearchCorpus, UnifiedSearchFilter},
    service::{query::Page, ServiceError, ServiceRuntime, SnapshotSubmission},
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
    Json, Router,
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

const MAX_BODY: usize = 16 * 1024 * 1024;
struct Gate {
    auth: LocalAuth,
    inflight: Semaphore,
}

pub fn build_router(runtime: Arc<ServiceRuntime>, auth: LocalAuth) -> Router {
    let gate = Arc::new(Gate {
        auth,
        inflight: Semaphore::new(32),
    });
    Router::new()
        .route("/healthz", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/api/v1/capabilities", get(capabilities))
        .route("/api/v1/source-registrations", post(register))
        .route("/api/v1/session-snapshots", post(ingest))
        .route("/api/v1/sessions", get(sessions))
        .route("/api/v1/sessions/{id}", get(session))
        .route("/api/v1/receipts/{id}", get(receipt))
        .route("/api/v1/jobs/{id}", get(job))
        .route("/api/v1/search", post(search))
        .route("/api/v1/knowledge", get(knowledge_list))
        .route("/api/v1/knowledge/{id}", get(knowledge))
        .route("/api/v1/knowledge/{id}/assist", post(assist))
        .fallback(|| async { ApiError(ServiceError::NotFound) })
        .method_not_allowed_fallback(|| async { ApiError(ServiceError::InvalidInput) })
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(gate, guard))
        .with_state(runtime)
}

async fn guard(State(gate): State<Arc<Gate>>, request: Request, next: Next) -> Response {
    let public = request.uri().path() == "/healthz";
    if !gate.auth.check(request.headers(), public) {
        return ApiError(ServiceError::Unauthorized).into_response();
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
    if request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|size| size > MAX_BODY as u64)
    {
        return ApiError(ServiceError::TooLarge).into_response();
    }
    let Ok(_permit) = gate.inflight.try_acquire() else {
        return ApiError(ServiceError::Unavailable).into_response();
    };
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

async fn capabilities(State(runtime): State<Arc<ServiceRuntime>>) -> Json<Value> {
    Json(runtime.capabilities())
}
async fn register(
    State(runtime): State<Arc<ServiceRuntime>>,
    input: Result<Json<Registration>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let input = body(input)?;
    let id = runtime
        .register_source(input.source, input.registration_key)
        .await?;
    Ok(Json(json!({"source_registration_id":id})))
}
async fn ingest(
    State(runtime): State<Arc<ServiceRuntime>>,
    input: Result<Json<SnapshotSubmission>, JsonRejection>,
) -> Result<Response, ApiError> {
    let (receipt, created) = runtime.accept(body(input)?).await?;
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
async fn sessions(
    State(r): State<Arc<ServiceRuntime>>,
    input: Result<Query<Page>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(r.sessions(page(input)?).await?))
}
async fn session(
    State(r): State<Arc<ServiceRuntime>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(r.session(id).await?))
}
async fn receipt(
    State(r): State<Arc<ServiceRuntime>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    Ok(Json(r.receipt(id).await?).into_response())
}
async fn job(
    State(r): State<Arc<ServiceRuntime>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(r.job(id).await?))
}
async fn knowledge_list(
    State(r): State<Arc<ServiceRuntime>>,
    input: Result<Query<Page>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(r.knowledge_list(page(input)?).await?))
}
async fn knowledge(
    State(r): State<Arc<ServiceRuntime>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    Ok(Json(r.knowledge(id).await?).into_response())
}
async fn assist(
    State(r): State<Arc<ServiceRuntime>>,
    Path(id): Path<String>,
    input: Result<Json<AssistRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(r.assist(id, body(input)?.operation).await?))
}
async fn search(
    State(r): State<Arc<ServiceRuntime>>,
    input: Result<Json<SearchRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let input = body(input)?;
    Ok(Json(
        r.search(
            input.query,
            input.limit,
            UnifiedSearchFilter {
                corpora: input.corpora,
                project: input.project,
                source: input.source,
            },
        )
        .await?,
    ))
}
