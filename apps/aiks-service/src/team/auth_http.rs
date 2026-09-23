//! Limits and fixed error bodies shared by the native authentication endpoints.
use aiks_core::team::TeamError;
use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

pub(crate) enum TeamHttpError {
    Business(TeamError),
    TooLarge,
    Limited,
}
impl From<TeamError> for TeamHttpError {
    fn from(value: TeamError) -> Self {
        Self::Business(value)
    }
}
impl IntoResponse for TeamHttpError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
            Self::Limited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            Self::Business(error) => match error {
                TeamError::LoginPending => (StatusCode::ACCEPTED, "login_pending"),
                TeamError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
                TeamError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
                TeamError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
                TeamError::Conflict => (StatusCode::CONFLICT, "conflict"),
                TeamError::InvalidInput | TeamError::ConfigInvalid => {
                    (StatusCode::BAD_REQUEST, "invalid_input")
                }
                TeamError::DirectoryUnavailable => {
                    (StatusCode::SERVICE_UNAVAILABLE, "directory_unavailable")
                }
                TeamError::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
                TeamError::Storage => (StatusCode::INTERNAL_SERVER_ERROR, "storage_unavailable"),
            },
        };
        let mut response = (
            status,
            Json(json!({"error":{"code":code,"request_id":uuid::Uuid::new_v4().to_string()}})),
        )
            .into_response();
        if status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert("retry-after", axum::http::HeaderValue::from_static("60"));
        }
        response
    }
}
pub(crate) fn one_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    value.to_str().ok()
}
pub(crate) fn bearer(headers: &HeaderMap) -> Result<String, TeamError> {
    one_header(headers, "authorization")
        .and_then(|s| s.strip_prefix("Bearer "))
        .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_owned)
        .ok_or(TeamError::Unauthorized)
}
struct Bucket {
    since: Instant,
    starts: u32,
    other: u32,
}
#[derive(Default)]
pub(crate) struct LoginLimiter {
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
}
impl LoginLimiter {
    pub fn check(&self, ip: IpAddr, start: bool) -> Result<(), TeamHttpError> {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().map_err(|_| TeamError::Unavailable)?;
        buckets.retain(|_, b| now.duration_since(b.since) < Duration::from_secs(60));
        if !buckets.contains_key(&ip) && buckets.len() >= 1024 {
            return Err(TeamHttpError::Limited);
        }
        let bucket = buckets.entry(ip).or_insert(Bucket {
            since: now,
            starts: 0,
            other: 0,
        });
        let (count, limit) = if start {
            (&mut bucket.starts, 12)
        } else {
            (&mut bucket.other, 120)
        };
        if *count >= limit {
            return Err(TeamHttpError::Limited);
        }
        *count += 1;
        Ok(())
    }
}
