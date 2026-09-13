/// V3 Processing Pipeline
pub mod orchestrator;
pub mod cleaner;
pub mod status;
pub mod repo;
pub mod session_chunker;
pub mod ai_stage;
pub mod knowledge_repo;
pub mod embedding_client;
pub mod embedding_stage;
pub mod search;
pub mod worker;

pub use orchestrator::PipelineOrchestrator;
pub use status::{PipelineStatus, StageStatus};
pub use worker::{PipelineWorker, PipelineJob};
pub use embedding_client::EmbeddingConfig;
pub use knowledge_repo::KnowledgeRepo;
pub use search::hybrid_search;
