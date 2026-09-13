use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Extraction status for a session's knowledge extraction (spec §25)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExtractionStatus {
    Pending,
    Running,
    Success,
    Skipped,
    Failed,
    /// Source content changed, needs re-extraction
    Stale,
}

impl ExtractionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExtractionStatus::Pending => "PENDING",
            ExtractionStatus::Running => "RUNNING",
            ExtractionStatus::Success => "SUCCESS",
            ExtractionStatus::Skipped => "SKIPPED",
            ExtractionStatus::Failed => "FAILED",
            ExtractionStatus::Stale => "STALE",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "RUNNING" => Self::Running,
            "SUCCESS" => Self::Success,
            "SKIPPED" => Self::Skipped,
            "FAILED" => Self::Failed,
            "STALE" => Self::Stale,
            _ => Self::Pending,
        }
    }
}

/// A record of knowledge extraction for a session
#[derive(Debug, Clone)]
pub struct ExtractionRecord {
    pub id: i64,
    pub source: String,
    pub external_session_id: String,
    pub source_content_hash: String,
    pub extractor_version: String,
    pub prompt_version: String,
    pub model: String,
    pub model_endpoint: Option<String>,
    pub knowledge_score: Option<f64>,
    pub category: Option<String>,
    pub status: ExtractionStatus,
    pub knowledge_document_id: Option<String>,
    pub knowledge_hash: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Stats about knowledge extraction
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionStats {
    pub total: usize,
    pub success: usize,
    pub skipped: usize,
    pub failed: usize,
    pub pending: usize,
    pub running: usize,
}
