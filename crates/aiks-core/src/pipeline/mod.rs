/// V3 Processing Pipeline
///
/// Orchestrates the multi-stage knowledge extraction pipeline:
/// Session → Parse → Normalize → Clean → LLM Chunk → AI Extract
///         → Knowledge Split → Embed Chunk → Embed → Index → Ready
pub mod orchestrator;
pub mod cleaner;
pub mod status;
pub mod repo;

pub use orchestrator::PipelineOrchestrator;
pub use status::{PipelineStatus, StageStatus};
