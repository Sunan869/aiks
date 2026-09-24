use std::fmt;

use serde::{Deserialize, Serialize};

use crate::model::NormalizedSession;

pub const API_VERSION: u16 = 1;
pub const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_MESSAGES: usize = 20_000;
pub const MAX_BLOCKS_PER_MESSAGE: usize = 256;
pub const MAX_JSON_DEPTH: usize = 64;
pub const MAX_JSON_NODES: usize = 1_000_000;

/// Intentionally not Debug: this request contains private conversation data.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotSubmission {
    pub api_version: u16,
    pub submission_id: String,
    pub service_instance_id: String,
    pub space_id: String,
    pub source_registration_id: String,
    pub expected_revision: u32,
    pub complete: bool,
    pub parser_version: String,
    pub session: NormalizedSession,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotReceipt {
    pub receipt_id: String,
    pub session_id: String,
    pub snapshot_id: String,
    pub revision: u32,
    pub job_id: String,
    pub pipeline_run_id: String,
    pub state: String,
}

/// All public messages are fixed: no input text, file paths or upstream errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ServiceError {
    #[error("Invalid request")]
    InvalidInput,
    #[error("Unsupported protocol version")]
    UnsupportedVersion,
    #[error("A complete snapshot is required")]
    IncompleteSnapshot,
    #[error("Request exceeds the allowed budget")]
    TooLarge,
    #[error("Authentication required")]
    Unauthorized,
    #[error("Resource not found")]
    NotFound,
    #[error("Operation is not permitted")]
    Forbidden,
    #[error("Request conflicts with a previously accepted revision")]
    Conflict,
    #[error("Dependency temporarily unavailable")]
    Unavailable,
    #[error("AI is not configured or is disabled")]
    AiDisabled,
    #[error("Canonical content is temporarily unavailable")]
    ContentUnavailable,
    #[error("Internal service error")]
    Internal,
    #[error("Revision limit reached")]
    RevisionExhausted,
}

impl ServiceError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::UnsupportedVersion => "unsupported_version",
            Self::IncompleteSnapshot => "incomplete_snapshot",
            Self::TooLarge => "too_large",
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "not_found",
            Self::Forbidden => "forbidden",
            Self::Conflict => "conflict",
            Self::Unavailable => "unavailable",
            Self::AiDisabled => "ai_disabled",
            Self::ContentUnavailable => "content_unavailable",
            Self::Internal => "internal",
            Self::RevisionExhausted => "revision_exhausted",
        }
    }
}

pub struct ValidatedSnapshot {
    pub submission: SnapshotSubmission,
    pub canonical_json: Vec<u8>,
    pub request_hash: String,
    pub content_hash: String,
}

impl fmt::Debug for ValidatedSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidatedSnapshot")
            .field("api_version", &self.submission.api_version)
            .field("message_count", &self.submission.session.messages.len())
            .field("serialized_bytes", &self.canonical_json.len())
            .finish_non_exhaustive()
    }
}

impl From<crate::team::TeamError> for ServiceError {
    fn from(error: crate::team::TeamError) -> Self {
        use crate::team::TeamError;
        match error {
            TeamError::Unauthorized | TeamError::LoginPending => Self::Unauthorized,
            TeamError::NotFound => Self::NotFound,
            TeamError::Forbidden => Self::Forbidden,
            TeamError::Conflict => Self::Conflict,
            TeamError::InvalidInput => Self::InvalidInput,
            TeamError::DirectoryUnavailable | TeamError::Unavailable | TeamError::ConfigInvalid => {
                Self::Unavailable
            }
            TeamError::Storage => Self::Internal,
        }
    }
}
