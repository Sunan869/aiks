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
            model: String::new(),
            api_key: None,
            dimensions: Some(1024),
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
        if texts.is_empty() { return Ok(vec![]); }

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
            anyhow::bail!("Embedding API error {}: {}", status, &body[..body.len().min(200)]);
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

/// Cosine similarity between two vectors
pub fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
