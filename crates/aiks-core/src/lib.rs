pub mod ai;
pub mod bootstrap;
pub mod config;
pub mod engine;
pub mod knowledge;
pub mod model;
pub mod pipeline;
pub mod providers;
pub mod renderer;
pub mod runtime;
pub mod sink;
pub mod storage;
pub mod sync;
pub mod util;
pub mod watcher;

// Re-export the most commonly used types
pub use ai::AiModelConfig;
pub use config::Config;
pub use engine::{
    AiStatus, AiksEngine, AiksEngineConfig, AppStatus, DoctorCheck, DoctorResult, FullStatus,
    ScanResult,
};
pub use knowledge::model::ExtractionStats;
pub use model::pipeline::{KnowledgeChunk, KnowledgeItem, PipelineStage, PipelineStats};
pub use model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind};
pub use pipeline::{
    EmbeddingConfig, KnowledgeRepo, PipelineJob, PipelineOrchestrator, PipelineWorker,
};
pub use providers::SessionSummary;
pub use sync::{ExtractionCandidate, SyncOptions, SyncOutcome, SyncStats};
