pub mod chunker;
pub mod client;
pub mod config;
pub mod extractor;
pub mod prompts;
pub mod prompts_v3;
pub mod schema;
pub mod schema_v3;

pub use client::AiClient;
pub use config::AiModelConfig;
pub use extractor::KnowledgeExtractor;
pub use schema::KnowledgeDocument;
pub use schema_v3::{V3ExtractionResult, V3KnowledgeItem};

#[cfg(test)]
mod model_service_contract_tests {
    use super::*;
    use crate::pipeline::EmbeddingConfig;

    #[test]
    fn model_service_exposes_one_llm_and_embedding_identity() {
        let llm = AiModelConfig {
            model: "qwen-test".into(),
            ..Default::default()
        };
        let embedding = EmbeddingConfig {
            model: "embed-test".into(),
            dimensions: Some(768),
            ..Default::default()
        };

        let service = ModelService::new(llm, embedding).unwrap();

        assert_eq!(service.llm_config().model, "qwen-test");
        assert_eq!(service.embedding_config().model, "embed-test");
        assert_eq!(service.embedding_config().dimensions, Some(768));
    }
}
