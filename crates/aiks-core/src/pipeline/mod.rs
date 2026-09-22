pub mod ai_stage;
pub mod cleaner;
pub mod embedding_client;
pub mod embedding_stage;
pub mod input;
pub mod job_repo;
pub mod knowledge_repo;
/// V3 Processing Pipeline
pub mod orchestrator;
pub mod repo;
pub mod search;
pub mod session_chunker;
pub mod status;
pub mod worker;

pub use embedding_client::EmbeddingConfig;
pub use knowledge_repo::KnowledgeRepo;
pub use orchestrator::PipelineOrchestrator;
pub use search::hybrid_search;
pub use status::{PipelineStatus, StageStatus};
pub use worker::{recover_interrupted_runs, PipelineJob, PipelineWorker};
