/// OpenAI-compatible HTTP client for knowledge extraction.
use std::time::{Duration, Instant};

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
    #[serde(skip_serializing_if = "is_false")]
    stream: bool,
    /// vLLM hard switch for Qwen3-style thinking models. Sent only when
    /// config.disable_thinking is set; unknown servers simply ignore or
    /// reject the extra field (a 400 surfaces in the error message).
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<serde_json::Value>,
}

fn is_false(value: &bool) -> bool {
    !*value
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
struct ChatStreamResponse {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: AssistantDelta,
}

#[derive(Debug, Deserialize)]
struct AssistantDelta {
    #[serde(default)]
    content: Option<String>,
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
        let mut builder = Client::builder().timeout(Duration::from_secs(config.timeout_seconds));

        if let Some(api_key) = &config.api_key {
            if !api_key.is_empty() {
                let mut headers = reqwest::header::HeaderMap::new();
                let value = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", api_key))?;
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
                    debug!(
                        "Model {} not in models list, but API is accessible",
                        self.config.model
                    );
                    return true; // API accessible even if model not listed
                }
                return true;
            }
        }

        // Fallback: try a minimal chat completion
        self.test_completion().await.is_ok()
    }

    async fn test_completion(&self) -> anyhow::Result<()> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let req = ChatRequest {
            model: &self.config.model,
            messages: vec![ChatMessage {
                role: "user",
                content: "Hi",
            }],
            temperature: self.config.temperature,
            max_tokens: 5,
            stream: false,
            chat_template_kwargs: self.thinking_switch(),
        };
        let resp = self.client.post(&url).json(&req).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP {}", resp.status());
        }
        Ok(())
    }

    /// `chat_template_kwargs` payload that turns Qwen3 thinking off, or None
    /// when the feature is disabled in config (non-vLLM endpoints).
    fn thinking_switch(&self) -> Option<serde_json::Value> {
        if self.config.disable_thinking {
            Some(serde_json::json!({ "enable_thinking": false }))
        } else {
            None
        }
    }

    /// Call the chat completions endpoint.
    ///
    /// Context-length guard: when the server rejects the request because
    /// prompt + max_tokens exceeds the model context, max_tokens is halved
    /// and the request retried (down to a 1024 floor). A large output budget
    /// must not make long-prompt chunks unprocessable.
    pub async fn chat(&self, system: &str, user: &str) -> anyhow::Result<String> {
        self.chat_with_max_tokens(system, user, self.config.max_tokens)
            .await
    }

    pub async fn chat_with_max_tokens(
        &self,
        system: &str,
        user: &str,
        requested_max_tokens: u32,
    ) -> anyhow::Result<String> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let mut max_tokens = requested_max_tokens.max(1);
        loop {
            let req = ChatRequest {
                model: &self.config.model,
                messages: vec![
                    ChatMessage {
                        role: "system",
                        content: system,
                    },
                    ChatMessage {
                        role: "user",
                        content: user,
                    },
                ],
                temperature: self.config.temperature,
                max_tokens,
                stream: false,
                chat_template_kwargs: self.thinking_switch(),
            };

            debug!(url = %url, model = %self.config.model, max_tokens, "Calling AI");

            let resp = self
                .client
                .post(&url)
                .json(&req)
                .send()
                .await
                .context("AI HTTP request failed")?;

            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();

            if status.as_u16() == 400
                && body_text.contains("maximum context length")
                && max_tokens > 1024
            {
                let reduced = (max_tokens / 2).max(1024);
                tracing::warn!(
                    requested = max_tokens,
                    reduced,
                    "Prompt + max_tokens exceeded model context; retrying with reduced output budget"
                );
                max_tokens = reduced;
                continue;
            }

            if !status.is_success() {
                let preview = crate::util::truncate_chars(&body_text, 200);
                anyhow::bail!("AI API error {}: {}", status, preview);
            }

            let body: ChatResponse =
                serde_json::from_str(&body_text).context("parse AI response")?;
            let content = body
                .choices
                .into_iter()
                .next()
                .map(|c| c.message.content)
                .ok_or_else(|| anyhow::anyhow!("Empty AI response"))?;

            return Ok(content);
        }
    }

    /// Stream an OpenAI-compatible chat completion and publish text deltas as
    /// soon as the server sends them. Servers that ignore stream=true and
    /// return a normal JSON completion are handled as a one-delta fallback.
    pub async fn chat_stream_with_max_tokens<F>(
        &self,
        system: &str,
        user: &str,
        requested_max_tokens: u32,
        mut on_delta: F,
    ) -> anyhow::Result<String>
    where
        F: FnMut(&str) -> anyhow::Result<()> + Send,
    {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let mut max_tokens = requested_max_tokens.max(1);

        loop {
            let request_started = Instant::now();
            let req = ChatRequest {
                model: &self.config.model,
                messages: vec![
                    ChatMessage {
                        role: "system",
                        content: system,
                    },
                    ChatMessage {
                        role: "user",
                        content: user,
                    },
                ],
                temperature: self.config.temperature,
                max_tokens,
                stream: true,
                chat_template_kwargs: self.thinking_switch(),
            };

            debug!(
                url = %url,
                model = %self.config.model,
                max_tokens,
                stream = true,
                "Calling AI"
            );

            let mut resp = self
                .client
                .post(&url)
                .json(&req)
                .send()
                .await
                .context("AI streaming HTTP request failed")?;

            let status = resp.status();
            if !status.is_success() {
                let body_text = resp.text().await.unwrap_or_default();
                if status.as_u16() == 400
                    && body_text.contains("maximum context length")
                    && max_tokens > 1024
                {
                    let reduced = (max_tokens / 2).max(1024);
                    tracing::warn!(
                        requested = max_tokens,
                        reduced,
                        "Prompt + max_tokens exceeded model context; retrying streaming request with reduced output budget"
                    );
                    max_tokens = reduced;
                    continue;
                }
                let preview = crate::util::truncate_chars(&body_text, 200);
                anyhow::bail!("AI API error {}: {}", status, preview);
            }

            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_ascii_lowercase();

            if content_type.contains("application/json") {
                let body_text = resp.text().await.unwrap_or_default();
                let body: ChatResponse =
                    serde_json::from_str(&body_text).context("parse AI response")?;
                let content = body
                    .choices
                    .into_iter()
                    .next()
                    .map(|choice| choice.message.content)
                    .ok_or_else(|| anyhow::anyhow!("Empty AI response"))?;
                if !content.is_empty() {
                    tracing::info!(
                        model = %self.config.model,
                        ttft_ms = request_started.elapsed().as_millis() as u64,
                        fallback_json = true,
                        "[AI_TIMING] first response text"
                    );
                    on_delta(&content)?;
                }
                tracing::info!(
                    model = %self.config.model,
                    llm_total_ms = request_started.elapsed().as_millis() as u64,
                    output_chars = content.chars().count(),
                    streaming = false,
                    "[AI_TIMING] completion complete"
                );
                return Ok(content);
            }

            let mut pending = Vec::<u8>::new();
            let mut content = String::new();
            let mut first_text_seen = false;

            while let Some(chunk) = resp.chunk().await.context("read AI stream chunk")? {
                pending.extend_from_slice(&chunk);
                while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
                    let mut line = pending.drain(..=newline).collect::<Vec<_>>();
                    line.pop();
                    if let Some(delta) = parse_stream_line(&line)? {
                        if !first_text_seen {
                            first_text_seen = true;
                            tracing::info!(
                                model = %self.config.model,
                                ttft_ms = request_started.elapsed().as_millis() as u64,
                                "[AI_TIMING] first response text"
                            );
                        }
                        on_delta(&delta)?;
                        content.push_str(&delta);
                    }
                }
            }

            if !pending.is_empty() {
                if let Some(delta) = parse_stream_line(&pending)? {
                    if !first_text_seen {
                        tracing::info!(
                            model = %self.config.model,
                            ttft_ms = request_started.elapsed().as_millis() as u64,
                            "[AI_TIMING] first response text"
                        );
                    }
                    on_delta(&delta)?;
                    content.push_str(&delta);
                }
            }

            anyhow::ensure!(!content.is_empty(), "Empty AI streaming response");
            tracing::info!(
                model = %self.config.model,
                llm_total_ms = request_started.elapsed().as_millis() as u64,
                output_chars = content.chars().count(),
                streaming = true,
                "[AI_TIMING] completion complete"
            );
            return Ok(content);
        }
    }

    pub fn config(&self) -> &AiModelConfig {
        &self.config
    }
}

fn parse_stream_line(line: &[u8]) -> anyhow::Result<Option<String>> {
    let line = std::str::from_utf8(line)
        .context("AI stream returned invalid UTF-8")?
        .trim();
    let Some(data) = line.strip_prefix("data:") else {
        return Ok(None);
    };
    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return Ok(None);
    }

    let body: ChatStreamResponse =
        serde_json::from_str(data).context("parse AI stream event")?;
    let delta = body
        .choices
        .into_iter()
        .filter_map(|choice| choice.delta.content)
        .collect::<String>();
    Ok((!delta.is_empty()).then_some(delta))
}

#[cfg(test)]
mod stream_tests {
    use super::parse_stream_line;

    #[test]
    fn parses_openai_stream_delta_and_ignores_done() {
        let delta = parse_stream_line(
            br#"data: {"choices":[{"delta":{"content":"hello"}}]}"#,
        )
        .unwrap();
        assert_eq!(delta.as_deref(), Some("hello"));
        assert_eq!(parse_stream_line(b"data: [DONE]").unwrap(), None);
        assert_eq!(parse_stream_line(b"event: message").unwrap(), None);
    }
}
