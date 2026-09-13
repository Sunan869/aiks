pub mod client;
pub mod config;
pub mod schema;
pub mod schema_v3;
pub mod prompts;
pub mod prompts_v3;
pub mod chunker;
pub mod extractor;

pub use client::AiClient;
pub use config::AiModelConfig;
pub use schema::KnowledgeDocument;
pub use schema_v3::{V3ExtractionResult, V3KnowledgeItem};
pub use extractor::KnowledgeExtractor;
