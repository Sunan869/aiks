pub mod config;
pub mod model;
pub mod providers;
pub mod renderer;
pub mod sink;
pub mod storage;
pub mod sync;
pub mod util;
pub mod watcher;
pub mod runtime;
pub mod bootstrap;
pub mod engine;
pub mod ai;
pub mod knowledge;
pub mod pipeline;

// Re-export the most commonly used types
pub use engine::{AiksEngine, AiksEngineConfig, DoctorResult, DoctorCheck, AppStatus, ScanResult, AiStatus, FullStatus};
pub use config::Config;
pub use model::{NormalizedSession, NormalizedMessage, ContentBlock, MessageRole, SourceKind};
pub use model::pipeline::{PipelineStage, KnowledgeItem, KnowledgeChunk, EmbeddingConfig, PipelineStats};
pub use providers::SessionSummary;
pub use sync::{SyncOptions, SyncStats, SyncOutcome};
pub use ai::AiModelConfig;
pub use knowledge::model::ExtractionStats;
pub use pipeline::{PipelineOrchestrator};
