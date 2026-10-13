use serde::{Deserialize, Serialize};

/// Status of a pipeline run (used by UI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStatus {
    pub run_id: String,
    pub session_id: i64,
    pub session_title: Option<String>,
    pub source: String,
    pub status: String,
    pub current_stage: Option<String>,
    pub pipeline_version: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error_stage: Option<String>,
    pub error_message: Option<String>,
    pub stage_runs: Vec<StageStatus>,
    pub knowledge_count: usize,
}

/// Status of a single pipeline stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageStatus {
    pub stage: String,
    pub status: String,
    pub input_count: Option<i32>,
    pub output_count: Option<i32>,
    pub latency_ms: Option<i64>,
    pub error_message: Option<String>,
    pub detail: Option<serde_json::Value>,
}

impl StageStatus {
    pub fn pending(stage: &str) -> Self {
        Self {
            stage: stage.to_string(),
            status: "PENDING".to_string(),
            input_count: None,
            output_count: None,
            latency_ms: None,
            error_message: None,
            detail: None,
        }
    }
}
