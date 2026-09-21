/// Embedding Client — calls OpenAI-compatible /v1/embeddings API
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingConfig {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub dimensions: Option<usize>,
    pub batch_size: usize,
    pub chunk_target_tokens: usize,
    pub chunk_max_tokens: usize,
    pub chunk_overlap_tokens: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            model: "LCO-Embedding/LCO-Embedding-Omni-3B-2605".to_string(),
            api_key: None,
            dimensions: Some(2048),
            batch_size: 16,
            chunk_target_tokens: 800,
            chunk_max_tokens: 1200,
            chunk_overlap_tokens: 120,
        }
    }
}

#[derive(Serialize)]
struct EmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
    index: usize,
}

pub struct EmbeddingClient {
    pub config: EmbeddingConfig,
    http: Client,
}

impl EmbeddingClient {
    pub fn new(config: EmbeddingConfig) -> anyhow::Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Self { config, http })
    }

    /// Embed a batch of texts. Returns embeddings in the same order as input.
    pub async fn embed_batch(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let url = format!("{}/embeddings", self.config.base_url.trim_end_matches('/'));

        let mut req = self.http.post(&url).json(&EmbedRequest {
            model: self.config.model.clone(),
            input: texts.clone(),
        });

        if let Some(key) = &self.config.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Embedding API error {}: {}",
                status,
                &body[..body.len().min(200)]
            );
        }

        let embed_resp: EmbedResponse = resp.json().await?;
        let mut results = vec![vec![]; texts.len()];
        for item in embed_resp.data {
            if item.index < results.len() {
                results[item.index] = item.embedding;
            }
        }
        Ok(results)
    }

    /// Health check
    pub async fn health_check(&self) -> bool {
        if !self.config.enabled || self.config.base_url.is_empty() {
            return false;
        }
        match self.embed_batch(vec!["test".to_string()]).await {
            Ok(v) => !v.is_empty() && !v[0].is_empty(),
            Err(_) => false,
        }
    }
}

pub fn l2_norm(vector: &[f32]) -> f32 {
    vector.iter().map(|value| value * value).sum::<f32>().sqrt()
}

/// Cosine similarity when the caller already has the left-hand vector norm.
/// Query-time retrieval uses this to avoid recomputing the same query norm for
/// every document candidate.
pub fn cosine_sim_with_left_norm(a: &[f32], norm_a: f32, b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() || norm_a == 0.0 {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_b = l2_norm(b);
    if norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Cosine similarity between two vectors
pub fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    cosine_sim_with_left_norm(a, l2_norm(a), b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_with_precomputed_left_norm_matches_regular_cosine() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![3.0, 2.0, 1.0];
        let expected = cosine_sim(&a, &b);
        let actual = cosine_sim_with_left_norm(&a, l2_norm(&a), &b);
        assert!((expected - actual).abs() < 1e-6);
    }

    #[test]
    fn cosine_identical_vectors() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_sim(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_sim(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_empty() {
        assert_eq!(cosine_sim(&[], &[]), 0.0);
    }

    #[test]
    fn default_embedding_identity_is_lco_but_remains_disabled() {
        let config = EmbeddingConfig::default();
        assert!(!config.enabled);
        assert!(config.base_url.is_empty());
        assert_eq!(config.model, "LCO-Embedding/LCO-Embedding-Omni-3B-2605");
        assert_eq!(config.dimensions, Some(2048));
    }
}
