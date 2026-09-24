use super::business_routes::TeamBusinessState;
use crate::error::ApiError;
use aiks_core::{
    service::{RequestContext, ServiceError},
    team::{GrantInput, GrantTarget, TeamContext},
};
use axum::{
    extract::{rejection::JsonRejection, rejection::QueryRejection, Path, Query, State},
    routing::get,
    Extension, Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceShares {
    expected_grant_version: u64,
    grants: Vec<ShareInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShareInput {
    target_type: String,
    target_id: String,
    #[serde(default)]
    include_descendants: bool,
    permission: String,
}

pub(super) fn routes() -> Router<Arc<TeamBusinessState>> {
    Router::new()
        .route("/api/v1/knowledge/{id}/shares", get(list).put(replace))
        .route("/api/v1/directory/search", get(directory_search))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectorySearch {
    q: String,
    #[serde(default = "default_directory_limit")]
    limit: usize,
}

fn default_directory_limit() -> usize {
    30
}

async fn directory_search(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    input: Result<Query<DirectorySearch>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let Query(input) = input.map_err(|_| ApiError(ServiceError::InvalidInput))?;
    let ctx = RequestContext::Team(team);
    Ok(Json(json!({
        "items": state
            .runtime
            .directory_search_for(&ctx, input.q, input.limit)
            .await?
    })))
}

async fn list(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let ctx = RequestContext::Team(team);
    Ok(Json(json!(state.runtime.share_state_for(&ctx, id).await?)))
}

async fn replace(
    State(state): State<Arc<TeamBusinessState>>,
    Extension(team): Extension<TeamContext>,
    Path(id): Path<String>,
    input: Result<Json<ReplaceShares>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(input) = input.map_err(|error| {
        ApiError(
            if error.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
                ServiceError::TooLarge
            } else {
                ServiceError::InvalidInput
            },
        )
    })?;
    if input.grants.len() > 512 {
        return Err(ApiError(ServiceError::TooLarge));
    }
    let mut grants = Vec::with_capacity(input.grants.len());
    for grant in input.grants {
        if grant.permission != "read" {
            return Err(ApiError(ServiceError::InvalidInput));
        }
        let target = match grant.target_type.as_str() {
            "user" if !grant.include_descendants => GrantTarget::User(grant.target_id),
            "org" => GrantTarget::Org {
                id: grant.target_id,
                descendants: grant.include_descendants,
            },
            _ => return Err(ApiError(ServiceError::InvalidInput)),
        };
        grants.push(GrantInput { target });
    }
    let ctx = RequestContext::Team(team);
    let version = state
        .runtime
        .replace_shares_for(&ctx, id, input.expected_grant_version, grants)
        .await?;
    Ok(Json(json!({"grant_version":version})))
}
