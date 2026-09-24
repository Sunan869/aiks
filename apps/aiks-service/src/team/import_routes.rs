use super::business_routes::TeamBusinessState;
use crate::error::ApiError;
use aiks_core::{
    service::{RequestContext, ServiceError},
    team::TeamContext,
};
use axum::{
    extract::{rejection::JsonRejection, State},
    http::StatusCode,
    routing::post,
    Extension, Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportRequest {
    operation_id: String,
    title: String,
    markdown: String,
    source_fingerprint: String,
}

pub(super) fn routes() -> Router<Arc<TeamBusinessState>> {
    Router::new().route("/api/v1/knowledge/import", post(import))
}

async fn import(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    input: Result<Json<ImportRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<aiks_core::team::ImportReceipt>), ApiError> {
    let Json(input) = input.map_err(|error| {
        ApiError(if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ServiceError::TooLarge
        } else {
            ServiceError::InvalidInput
        })
    })?;
    let ctx = RequestContext::Team(team);
    let (receipt, created) = state
        .runtime
        .import_knowledge_for(
            &ctx,
            input.operation_id,
            input.title,
            input.markdown,
            input.source_fingerprint,
        )
        .await?;
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(receipt),
    ))
}
