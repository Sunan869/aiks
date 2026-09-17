use crate::ai::AiModelConfig;
use crate::pipeline::embedding_client::{EmbeddingClient, EmbeddingConfig};

/// Single AIKS entry point for model capabilities.
///
/// LLM extraction and embedding may use different physical models, but callers
/// resolve both capabilities through this service instead of owning endpoint
/// configuration independently.
pub struct ModelService {
    llm: AiModelConfig,
    embedding: EmbeddingClient,
}

impl ModelService {
    pub fn new(llm: AiModelConfig, embedding: EmbeddingConfig) -> anyhow::Result<Self> {
        Ok(Self {
            llm,
            embedding: EmbeddingClient::new(embedding)?,
        })
    }

    pub fn llm_config(&self) -> &AiModelConfig {
        &self.llm
    }

    pub fn embedding_config(&self) -> &EmbeddingConfig {
        &self.embedding.config
    }

    pub async fn embed_texts(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        self.embedding.embed_batch(texts).await
    }

    pub async fn embedding_health_check(&self) -> bool {
        self.embedding.health_check().await
    }
}
