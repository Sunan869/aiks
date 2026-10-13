// CI lint baseline: pre-existing Clippy debt; remove allowances incrementally.
#![allow(clippy::should_implement_trait)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// V3 Pipeline processing stages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PipelineStage {
    Discovered,
    Parsed,
    Normalized,
    Cleaned,
    LlmChunked,
    AiExtracted,
    KnowledgeSplit,
    EmbedChunked,
    Embedded,
    Indexed,
    Ready,
    RawOnly,
    Failed,
}

impl PipelineStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            PipelineStage::Discovered => "DISCOVERED",
            PipelineStage::Parsed => "PARSED",
            PipelineStage::Normalized => "NORMALIZED",
            PipelineStage::Cleaned => "CLEANED",
            PipelineStage::LlmChunked => "LLM_CHUNKED",
            PipelineStage::AiExtracted => "AI_EXTRACTED",
            PipelineStage::KnowledgeSplit => "KNOWLEDGE_SPLIT",
            PipelineStage::EmbedChunked => "EMBED_CHUNKED",
            PipelineStage::Embedded => "EMBEDDED",
            PipelineStage::Indexed => "INDEXED",
            PipelineStage::Ready => "READY",
            PipelineStage::RawOnly => "RAW_ONLY",
            PipelineStage::Failed => "FAILED",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "DISCOVERED" => PipelineStage::Discovered,
            "PARSED" => PipelineStage::Parsed,
            "NORMALIZED" => PipelineStage::Normalized,
            "CLEANED" => PipelineStage::Cleaned,
            "LLM_CHUNKED" => PipelineStage::LlmChunked,
            "AI_EXTRACTED" => PipelineStage::AiExtracted,
            "KNOWLEDGE_SPLIT" => PipelineStage::KnowledgeSplit,
            "EMBED_CHUNKED" => PipelineStage::EmbedChunked,
            "EMBEDDED" => PipelineStage::Embedded,
            "INDEXED" => PipelineStage::Indexed,
            "READY" => PipelineStage::Ready,
            "RAW_ONLY" => PipelineStage::RawOnly,
            _ => PipelineStage::Failed,
        }
    }

    /// True if this stage represents a terminal success state
    pub fn is_terminal_success(&self) -> bool {
        matches!(self, PipelineStage::Ready | PipelineStage::RawOnly)
    }

    /// True if this stage is failed
    pub fn is_failed(&self) -> bool {
        matches!(self, PipelineStage::Failed)
    }

    /// Returns the next stage in normal pipeline order
    pub fn next(&self) -> Option<PipelineStage> {
        match self {
            PipelineStage::Discovered => Some(PipelineStage::Parsed),
            PipelineStage::Parsed => Some(PipelineStage::Normalized),
            PipelineStage::Normalized => Some(PipelineStage::Cleaned),
            PipelineStage::Cleaned => Some(PipelineStage::LlmChunked),
            PipelineStage::LlmChunked => Some(PipelineStage::AiExtracted),
            PipelineStage::AiExtracted => Some(PipelineStage::KnowledgeSplit),
            PipelineStage::KnowledgeSplit => Some(PipelineStage::EmbedChunked),
            PipelineStage::EmbedChunked => Some(PipelineStage::Embedded),
            PipelineStage::Embedded => Some(PipelineStage::Indexed),
            PipelineStage::Indexed => Some(PipelineStage::Ready),
            _ => None,
        }
    }
}

/// A pipeline run record — tracks the full processing lifecycle of a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineRun {
    pub id: String,
    pub session_id: i64,
    pub status: String,
    pub current_stage: Option<String>,
    pub pipeline_version: String,
    pub source_hash: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_stage: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Stage runs (populated when detail requested)
    pub stage_runs: Vec<StageRun>,
}

/// A single stage execution within a pipeline run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRun {
    pub id: String,
    pub pipeline_run_id: String,
    pub stage: String,
    pub status: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub input_count: Option<i32>,
    pub output_count: Option<i32>,
    pub latency_ms: Option<i64>,
    pub detail_json: Option<serde_json::Value>,
    pub error_message: Option<String>,
}

/// Session chunk for LLM processing (splits long sessions)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionChunk {
    pub id: String,
    pub session_id: i64,
    pub chunk_index: i32,
    pub message_start: i32,
    pub message_end: i32,
    pub token_count: i32,
    pub content: String,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
}

/// Knowledge category
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeCategory {
    Troubleshooting,
    Architecture,
    Implementation,
    Configuration,
    Research,
    Decision,
    General,
}

impl KnowledgeCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            KnowledgeCategory::Troubleshooting => "troubleshooting",
            KnowledgeCategory::Architecture => "architecture",
            KnowledgeCategory::Implementation => "implementation",
            KnowledgeCategory::Configuration => "configuration",
            KnowledgeCategory::Research => "research",
            KnowledgeCategory::Decision => "decision",
            KnowledgeCategory::General => "general",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "troubleshooting" | "bug" | "fix" => KnowledgeCategory::Troubleshooting,
            "architecture" | "design" => KnowledgeCategory::Architecture,
            "implementation" | "coding" => KnowledgeCategory::Implementation,
            "configuration" | "config" | "setup" => KnowledgeCategory::Configuration,
            "research" | "exploration" => KnowledgeCategory::Research,
            "decision" | "adr" => KnowledgeCategory::Decision,
            _ => KnowledgeCategory::General,
        }
    }
}

/// A single extracted knowledge unit (0~N per session in V3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeItem {
    pub id: String,
    pub source_session_id: i64,
    pub project_name: Option<String>,
    pub title: String,
    pub category: KnowledgeCategory,
    pub summary: String,
    pub content: String,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub worth_extracting: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Chunks for embedding (populated when detail requested)
    pub chunks: Vec<KnowledgeChunk>,
}

/// A sub-chunk of a KnowledgeItem for vector embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeChunk {
    pub id: String,
    pub knowledge_id: String,
    pub heading: Option<String>,
    pub chunk_index: i32,
    pub token_count: i32,
    pub text: String,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
    /// Embedding record (populated when available)
    pub embedding: Option<EmbeddingRecord>,
}

/// An embedding vector stored for a knowledge chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRecord {
    pub id: String,
    pub chunk_id: String,
    pub model: String,
    pub dimensions: i32,
    /// Not serialized by default — too large
    #[serde(skip_serializing)]
    pub vector: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
}

/// Embedding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub dimensions: Option<usize>,
    pub batch_size: usize,
    pub chunk_target_tokens: usize,
    pub chunk_max_tokens: usize,
    pub chunk_overlap_tokens: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            model: String::new(),
            dimensions: Some(1024),
            batch_size: 16,
            chunk_target_tokens: 800,
            chunk_max_tokens: 1200,
            chunk_overlap_tokens: 120,
        }
    }
}

/// Summary stats for the processing pipeline
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PipelineStats {
    pub total: usize,
    pub processing: usize,
    pub ready: usize,
    pub raw_only: usize,
    pub failed: usize,
    pub knowledge_items: usize,
    pub knowledge_chunks: usize,
    pub embeddings: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_stage_roundtrip() {
        for stage in [
            PipelineStage::Discovered,
            PipelineStage::Parsed,
            PipelineStage::Normalized,
            PipelineStage::Cleaned,
            PipelineStage::LlmChunked,
            PipelineStage::AiExtracted,
            PipelineStage::KnowledgeSplit,
            PipelineStage::EmbedChunked,
            PipelineStage::Embedded,
            PipelineStage::Indexed,
            PipelineStage::Ready,
            PipelineStage::RawOnly,
        ] {
            assert_eq!(PipelineStage::from_str(stage.as_str()), stage);
        }
    }

    #[test]
    fn pipeline_stage_sequence() {
        assert_eq!(
            PipelineStage::Discovered.next(),
            Some(PipelineStage::Parsed)
        );
        assert_eq!(PipelineStage::Indexed.next(), Some(PipelineStage::Ready));
        assert_eq!(PipelineStage::Ready.next(), None);
        assert_eq!(PipelineStage::Failed.next(), None);
    }

    #[test]
    fn knowledge_category_from_str() {
        assert_eq!(
            KnowledgeCategory::from_str("troubleshooting"),
            KnowledgeCategory::Troubleshooting
        );
        assert_eq!(
            KnowledgeCategory::from_str("bug"),
            KnowledgeCategory::Troubleshooting
        );
        assert_eq!(
            KnowledgeCategory::from_str("architecture"),
            KnowledgeCategory::Architecture
        );
        assert_eq!(
            KnowledgeCategory::from_str("unknown"),
            KnowledgeCategory::General
        );
    }
}
