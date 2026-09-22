//! Client-only transport and upload state. Never opens a business StateDb.
mod outbox;
mod transport;

use aiks_core::service::{validate_submission, SnapshotSubmission};
pub use outbox::{ClaimedUpload, CollectorOutbox, EnqueueOutcome, UploadStatus};
use std::fmt;
pub use transport::{ServiceClient, ServiceConnection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientError {
    InvalidInput,
    WrongInstance,
    Unauthorized,
    NotFound,
    Conflict,
    Retryable,
    Busy,
    TooLarge,
    Storage,
    InvalidResponse,
}
impl ClientError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::WrongInstance => "wrong_instance",
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Retryable => "retryable",
            Self::Busy => "previous_upload_pending",
            Self::TooLarge => "too_large",
            Self::Storage => "collector_storage_unavailable",
            Self::InvalidResponse => "invalid_service_response",
        }
    }
}
impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for ClientError {}
impl From<rusqlite::Error> for ClientError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}
pub type ClientResult<T> = Result<T, ClientError>;

/// Immutable after validation; deliberately not Debug/Serialize.
#[derive(Clone)]
pub struct PendingSubmission {
    submission: SnapshotSubmission,
    body: Vec<u8>,
    request_hash: String,
    content_hash: String,
}
impl PendingSubmission {
    pub fn new(submission: SnapshotSubmission) -> ClientResult<Self> {
        let validated = validate_submission(&submission).map_err(|error| match error {
            aiks_core::service::ServiceError::TooLarge => ClientError::TooLarge,
            _ => ClientError::InvalidInput,
        })?;
        let body = serde_json::to_vec(&submission).map_err(|_| ClientError::InvalidInput)?;
        Ok(Self {
            submission,
            body,
            request_hash: validated.request_hash,
            content_hash: validated.content_hash,
        })
    }
    pub fn submission(&self) -> &SnapshotSubmission {
        &self.submission
    }
    pub fn payload_hash(&self) -> &str {
        &self.request_hash
    }
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}

pub(crate) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
