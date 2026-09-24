//! Authenticated team business surface. This router is intentionally not mounted
//! by the executable until the deployment/bootstrap task is completed.
use super::{middleware::require_session, share_routes};
use crate::error::ApiError;
use aiks_core::{
    knowledge::ai_assist::AiAssistOperation,
    model::SourceKind,
    search::{SearchCorpus, UnifiedSearchFilter},
    service::{query::Page, RequestContext, ServiceError, ServiceRuntime, SnapshotSubmission},
    team::{SessionStore, TeamContext},
};
use axum::{
    extract::{
        rejection::{JsonRejection, QueryRejection},
        DefaultBodyLimit, Path, Query, State,
    },
    middleware,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::sync::Arc;

const MAX_BODY: usize = 16 * 1024 * 1024;

pub(super) struct TeamBusinessState {
    pub(super) runtime: Arc<ServiceRuntime>,
}

pub fn business_router(runtime: Arc<ServiceRuntime>, sessions: Arc<SessionStore>) -> Router {
    let state = Arc::new(TeamBusinessState { runtime });
    Router::new()
        .route("/api/v1/capabilities", get(capabilities))
        .route("/api/v1/source-registrations", post(register))
        .route("/api/v1/session-snapshots", post(ingest))
        .route("/api/v1/sessions", get(sessions_list))
        .route("/api/v1/sessions/{id}", get(session))
        .route("/api/v1/receipts/{id}", get(receipt))
        .route("/api/v1/jobs/{id}", get(job))
        .route("/api/v1/search", post(search))
        .route("/api/v1/knowledge", get(knowledge_list))
        .route("/api/v1/knowledge/{id}", get(knowledge))
        .route("/api/v1/knowledge/{id}/assist", post(assist))
        .merge(share_routes::routes())
        .fallback(|| async { ApiError(ServiceError::NotFound) })
        .method_not_allowed_fallback(|| async { ApiError(ServiceError::InvalidInput) })
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(sessions, require_session))
        .with_state(state)
}

fn request_context(team: TeamContext) -> RequestContext {
    RequestContext::Team(team)
}

fn body<T: DeserializeOwned>(input: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    input.map(|Json(value)| value).map_err(|error| {
        ApiError(
            if error.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
                ServiceError::TooLarge
            } else {
                ServiceError::InvalidInput
            },
        )
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

async fn capabilities(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(_team): Extension<TeamContext>,
) -> Json<Value> {
    Json(state.runtime.capabilities())
}

async fn register(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    input: Result<Json<Registration>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let input = body(input)?;
    let id = state
        .runtime
        .register_source_for(&request_context(team), input.source, input.registration_key)
        .await?;
    Ok(Json(json!({"source_registration_id":id})))
}

async fn ingest(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    input: Result<Json<SnapshotSubmission>, JsonRejection>,
) -> Result<(axum::http::StatusCode, Json<SnapshotReceipt>), ApiError> {
    let (receipt, created) = state
        .runtime
        .accept_for(&request_context(team), body(input)?)
        .await?;
    Ok((
        if created {
            axum::http::StatusCode::ACCEPTED
        } else {
            axum::http::StatusCode::OK
        },
        Json(receipt),
    ))
}

use aiks_core::service::SnapshotReceipt;

async fn sessions_list(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    input: Result<Query<Page>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        state
            .runtime
            .sessions_for(&request_context(team), page(input)?)
            .await?,
    ))
}

async fn session(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        state
            .runtime
            .session_for(&request_context(team), id)
            .await?,
    ))
}

async fn receipt(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        state
            .runtime
            .receipt_for(&request_context(team), id)
            .await?
    )))
}

async fn job(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        state.runtime.job_for(&request_context(team), id).await?,
    ))
}

async fn knowledge_list(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    input: Result<Query<Page>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        state
            .runtime
            .knowledge_list_for(&request_context(team), page(input)?)
            .await?,
    ))
}

async fn knowledge(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        state
            .runtime
            .knowledge_for(&request_context(team), id)
            .await?
    )))
}

async fn assist(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    Path(id): Path<String>,
    input: Result<Json<AssistRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        state
            .runtime
            .assist_for(&request_context(team), id, body(input)?.operation)
            .await?,
    ))
}

async fn search(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    input: Result<Json<SearchRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let input = body(input)?;
    Ok(Json(
        state
            .runtime
            .search_for(
                &request_context(team),
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
