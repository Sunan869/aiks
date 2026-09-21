pub mod ai;
pub mod bootstrap;
pub mod config;
pub mod engine;
pub mod indexing;
pub mod knowledge;
pub mod model;
pub mod pipeline;
pub mod providers;
pub mod renderer;
pub mod runtime;
pub mod search;
pub mod share_import;
pub mod semantic_index;
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
pub use indexing::{
    EmbeddingProvider, KnowledgeIndexInput, KnowledgeIndexResult, KnowledgeIndexService,
    SessionIndexInput, SessionIndexResult, SessionIndexService,
};
pub use knowledge::model::ExtractionStats;
pub use knowledge::{
    CreateKnowledgeInput, KnowledgeListFilter, KnowledgeListResult, KnowledgeRecord,
    KnowledgeService, UpdateKnowledgeInput,
};
pub use model::pipeline::{KnowledgeChunk, KnowledgeItem, PipelineStage, PipelineStats};
pub use model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind};
pub use pipeline::{
    EmbeddingConfig, KnowledgeRepo, PipelineJob, PipelineOrchestrator, PipelineWorker,
};
pub use providers::SessionSummary;
pub use search::{
    SearchCorpus, UnifiedSearchFilter, UnifiedSearchHit, UnifiedSearchOutcome, UnifiedSearchService,
};
pub use semantic_index::{
    rebuild_semantic_index, SemanticIndexRebuildProgress, SemanticIndexRebuildStats,
};
pub use sync::{ExtractionCandidate, SyncOptions, SyncOutcome, SyncStats};

pub use share_import::{
    canonical_share_url, detect_share_source, persist_share_conversation, share_cache_root,
    share_external_id, ShareAssetInput, ShareConversationInput, ShareImportResult,
    ShareMessageInput,
};
