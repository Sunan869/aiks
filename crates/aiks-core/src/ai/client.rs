/// OpenAI-compatible HTTP client for knowledge extraction.
use std::time::Duration;

use anyhow::Context;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::ai::config::AiModelConfig;

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: AssistantMessage,
}

#[derive(Debug, Deserialize)]
struct AssistantMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    id: String,
}

/// OpenAI-compatible HTTP client.
pub struct AiClient {
    config: AiModelConfig,
    client: Client,
}

impl AiClient {
    pub fn new(config: AiModelConfig) -> anyhow::Result<Self> {
        let mut builder = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds));

        if let Some(api_key) = &config.api_key {
            if !api_key.is_empty() {
                let mut headers = reqwest::header::HeaderMap::new();
                let value = reqwest::header::HeaderValue::from_str(
                    &format!("Bearer {}", api_key),
                )?;
                headers.insert(reqwest::header::AUTHORIZATION, value);
                builder = builder.default_headers(headers);
            }
        }

        let client = builder.build()?;
        Ok(Self { config, client })
    }

    /// Check if the AI model is accessible.
    pub async fn health_check(&self) -> bool {
        // Try /v1/models first
        let models_url = format!("{}/models", self.config.base_url.trim_end_matches('/'));
        if let Ok(resp) = self.client.get(&models_url).send().await {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<ModelsResponse>().await {
                    let found = body.data.iter().any(|m| m.id == self.config.model);
                    if found {
                        return true;
                    }
                    debug!("Model {} not in models list, but API is accessible", self.config.model);
                    return true; // API accessible even if model not listed
                }
                return true;
            }
        }

        // Fallback: try a minimal chat completion
        self.test_completion().await.is_ok()
    }

    async fn test_completion(&self) -> anyhow::Result<()> {
        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));
        let req = ChatRequest {
            model: &self.config.model,
            messages: vec![ChatMessage { role: "user", content: "Hi" }],
            temperature: self.config.temperature,
            max_tokens: 5,
        };
        let resp = self.client.post(&url).json(&req).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP {}", resp.status());
        }
        Ok(())
    }

    /// Call the chat completions endpoint.
    pub async fn chat(
        &self,
        system: &str,
        user: &str,
    ) -> anyhow::Result<String> {
        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));
        let req = ChatRequest {
            model: &self.config.model,
            messages: vec![
                ChatMessage { role: "system", content: system },
                ChatMessage { role: "user", content: user },
            ],
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        };

        debug!(url = %url, model = %self.config.model, "Calling AI");

        let resp = self.client
            .post(&url)
            .json(&req)
            .send()
            .await
            .context("AI HTTP request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let preview = crate::util::truncate_chars(&body, 200);
            anyhow::bail!("AI API error {}: {}", status, preview);
        }

        let body: ChatResponse = resp.json().await.context("parse AI response")?;
        let content = body.choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| anyhow::anyhow!("Empty AI response"))?;

        Ok(content)
    }

    pub fn config(&self) -> &AiModelConfig {
        &self.config
    }
}
