//! Owner-only durable content operations and document-scoped managed assets.
use super::{auth_http::one_header, business_routes::TeamBusinessState};
use crate::error::ApiError;
use aiks_core::{
    service::{RequestContext, ServiceError},
    team::{TeamContext, MAX_MANAGED_ASSET_BYTES},
};
use axum::{
    body::{Body, Bytes},
    extract::{rejection::JsonRejection, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::Response,
    routing::{get, post, put},
    Extension, Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContentInput {
    base_revision: u64,
    operation_id: String,
    title: String,
    markdown: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishInput {
    base_revision: u64,
    operation_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetUpload {
    filename: String,
}

pub(super) fn routes() -> Router<Arc<TeamBusinessState>> {
    Router::new()
        .route("/api/v1/knowledge/{id}/content", put(update_content))
        .route("/api/v1/knowledge/{id}/publish", post(publish))
        .route("/api/v1/content-operations/{id}", get(operation))
        .route("/api/v1/knowledge/{id}/assets", post(upload_asset))
        .route("/api/v1/knowledge/{id}/assets/{asset_id}", get(asset))
}

fn json_body<T>(input: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    input.map(|Json(value)| value).map_err(|error| {
        ApiError(if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ServiceError::TooLarge
        } else {
            ServiceError::InvalidInput
        })
    })
}

async fn update_content(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    Path(id): Path<String>,
    input: Result<Json<ContentInput>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let input = json_body(input)?;
    let ctx = RequestContext::Team(team);
    let operation = state
        .runtime
        .submit_content_for(
            &ctx,
            id,
            input.operation_id,
            input.base_revision,
            input.title,
            input.markdown,
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(json!(operation))))
}

async fn publish(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    Path(id): Path<String>,
    input: Result<Json<PublishInput>, JsonRejection>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let input = json_body(input)?;
    let ctx = RequestContext::Team(team);
    let operation = state
        .runtime
        .publish_for(&ctx, id, input.operation_id, input.base_revision)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(json!(operation))))
}

async fn operation(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let ctx = RequestContext::Team(team);
    Ok(Json(json!(
        state.runtime.content_operation_for(&ctx, id).await?
    )))
}

async fn upload_asset(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    Path(id): Path<String>,
    Query(input): Query<AssetUpload>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if bytes.is_empty() {
        return Err(ApiError(ServiceError::InvalidInput));
    }
    if bytes.len() > MAX_MANAGED_ASSET_BYTES {
        return Err(ApiError(ServiceError::TooLarge));
    }
    let content_type = one_header(&headers, header::CONTENT_TYPE.as_str())
        .ok_or(ApiError(ServiceError::InvalidInput))?
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let ctx = RequestContext::Team(team);
    let asset_id = state
        .runtime
        .create_asset_for(&ctx, id, input.filename, content_type, bytes.to_vec())
        .await?;
    Ok((StatusCode::CREATED, Json(json!({"asset_id":asset_id}))))
}

async fn asset(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    Path((id, asset_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let ctx = RequestContext::Team(team);
    let asset = state.runtime.managed_asset_for(&ctx, id, asset_id).await?;
    let content_type =
        HeaderValue::from_str(&asset.content_type).map_err(|_| ApiError(ServiceError::Internal))?;
    let mut response = Response::new(Body::from(asset.bytes));
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, content_type);
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}
