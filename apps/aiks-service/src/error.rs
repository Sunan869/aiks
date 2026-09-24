use aiks_core::service::ServiceError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub(crate) struct ApiError(pub ServiceError);
impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        Self(error)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            ServiceError::Unauthorized => StatusCode::UNAUTHORIZED,
            ServiceError::NotFound => StatusCode::NOT_FOUND,
            ServiceError::Forbidden => StatusCode::FORBIDDEN,
            ServiceError::Conflict | ServiceError::RevisionExhausted => StatusCode::CONFLICT,
            ServiceError::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            ServiceError::IncompleteSnapshot => StatusCode::UNPROCESSABLE_ENTITY,
            ServiceError::Unavailable
            | ServiceError::AiDisabled
            | ServiceError::ContentUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            ServiceError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        };
        (status,Json(json!({"error":{"code":self.0.code(),"message":self.0.to_string(),"request_id":uuid::Uuid::new_v4().to_string()}}))).into_response()
    }
}
