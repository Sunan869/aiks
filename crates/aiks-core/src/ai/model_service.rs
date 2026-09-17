use serde::de::DeserializeOwned;

use crate::ai::{AiClient, AiModelConfig};
use crate::pipeline::embedding_client::{EmbeddingClient, EmbeddingConfig};

/// Single AIKS entry point for model capabilities.
///
/// LLM extraction, AI Assist, unified search, and embedding may use different
/// physical models, but callers resolve those capabilities through this service
/// instead of owning endpoint configuration independently.
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

    pub async fn complete_text(&self, system: &str, user: &str) -> anyhow::Result<String> {
        if !self.llm.enabled {
            anyhow::bail!("AI model is disabled");
        }
        AiClient::new(self.llm.clone())?.chat(system, user).await
    }

    pub async fn complete_json<T: DeserializeOwned>(
        &self,
        system: &str,
        user: &str,
    ) -> anyhow::Result<T> {
        let raw = self.complete_text(system, user).await?;
        let json = extract_json_payload(&raw);
        serde_json::from_str(json).map_err(|error| {
            anyhow::anyhow!(
                "AI returned invalid JSON: {error}; response preview: {}",
                crate::util::truncate_chars(&raw, 240)
            )
        })
    }

    pub async fn embed_texts(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        self.embedding.embed_batch(texts).await
    }

    pub async fn embedding_health_check(&self) -> bool {
        self.embedding.health_check().await
    }
}

fn extract_json_payload(raw: &str) -> &str {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }

    let after_open = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .trim_start_matches(['\r', '\n', ' ']);
    after_open
        .strip_suffix("```")
        .unwrap_or(after_open)
        .trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fenced_json_is_unwrapped_without_touching_plain_json() {
        assert_eq!(extract_json_payload("{\"ok\":true}"), "{\"ok\":true}");
        assert_eq!(
            extract_json_payload("```json\n{\"ok\":true}\n```"),
            "{\"ok\":true}"
        );
    }
}
