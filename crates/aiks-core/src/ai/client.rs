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
    #[serde(default)]
    usage: Option<ChatUsage>,
}

impl ChatResponse {
    fn into_result(self) -> anyhow::Result<ChatResult> {
        let choice = self
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Empty AI response"))?;
        // A gateway may ignore max_tokens even on non-streaming requests.
        // Check decoded text before publishing it to the RAG or extraction
        // pipeline. This bounds accepted output, not the HTTP body's memory.
        check_chat_output_budget(0, choice.message.content.len())?;
        Ok(ChatResult {
            content: choice.message.content,
            usage: self.usage,
            finish_reason: choice.finish_reason,
        })
    }
}

/// Actual token usage reported by an OpenAI-compatible endpoint.
/// Absent on some local model servers; never substitute guessed token counts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatUsage {
    #[serde(default)]
    pub prompt_tokens: Option<u64>,
    #[serde(default)]
    pub completion_tokens: Option<u64>,
    #[serde(default)]
    pub total_tokens: Option<u64>,
}

impl ChatUsage {
    /// Add only metrics the endpoint actually reported; missing never means zero.
    pub fn accumulate(&mut self, other: &Self) {
        fn add(dst: &mut Option<u64>, incoming: Option<u64>) {
            if let Some(value) = incoming {
                *dst = Some(dst.unwrap_or(0).saturating_add(value));
            }
        }
        add(&mut self.prompt_tokens, other.prompt_tokens);
        add(&mut self.completion_tokens, other.completion_tokens);
        add(&mut self.total_tokens, other.total_tokens);
    }
}

#[derive(Debug)]
pub struct ChatResult {
    pub content: String,
    pub usage: Option<ChatUsage>,
    /// Provider-reported stop reason for the chosen completion. Optional for
    /// older local OpenAI-compatible servers; never infer a reason from usage.
    pub finish_reason: Option<String>,
}

fn log_token_usage(model: &str, usage: Option<&ChatUsage>) {
    if let Some(usage) = usage {
        tracing::info!(
            model,
            prompt_tokens = usage.prompt_tokens,
            completion_tokens = usage.completion_tokens,
            total_tokens = usage.total_tokens,
            "AIKS_MODEL_TOKEN_USAGE"
        );
    }
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: AssistantMessage,
    #[serde(default)]
    finish_reason: Option<String>,
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
    #[serde(default)]
    delta: Option<AssistantDelta>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssistantDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum StreamEvent {
    Ignored,
    Done,
    Completion {
        text: Option<String>,
        finish_reason: Option<String>,
    },
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
                // /models is advisory during health checks; do not
                // deserialize an unbounded provider-controlled catalog.
                if let Ok(body_text) =
                    crate::util::read_bounded_http_text(resp, MAX_MODEL_LIST_RESPONSE_BYTES).await
                {
                    if let Ok(body) = serde_json::from_str::<ModelsResponse>(&body_text) {
                        let found = body.data.iter().any(|m| m.id == self.config.model);
                        if !found {
                            debug!(
                                "Model {} not in models list, but API is accessible",
                                self.config.model
                            );
                        }
                    }
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

    pub async fn chat_detailed(&self, system: &str, user: &str) -> anyhow::Result<ChatResult> {
        self.chat_with_max_tokens_detailed(system, user, self.config.max_tokens)
            .await
    }

    pub async fn chat_with_max_tokens(
        &self,
        system: &str,
        user: &str,
        requested_max_tokens: u32,
    ) -> anyhow::Result<String> {
        let result = self
            .chat_with_max_tokens_detailed(system, user, requested_max_tokens)
            .await?;
        validate_chat_finish_reason(result.finish_reason.as_deref())?;
        Ok(result.content)
    }

    async fn chat_with_max_tokens_detailed(
        &self,
        system: &str,
        user: &str,
        requested_max_tokens: u32,
    ) -> anyhow::Result<ChatResult> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let mut max_tokens = requested_max_tokens.max(1);
        let mut overload_retries = 0usize;
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
                .map_err(|error| {
                    anyhow::anyhow!(
                        "AI HTTP request failed (model={}, endpoint={}, timeout={}s, is_timeout={}, is_connect={}): {:#}",
                        self.config.model,
                        url,
                        self.config.timeout_seconds,
                        error.is_timeout(),
                        error.is_connect(),
                        error
                    )
                })?;

            let status = resp.status();
            // Only retry explicit capacity/rate-limit responses. Retrying arbitrary
            // 4xx responses hides configuration errors and wastes local GPU time.
            if let Some(delay_ms) = overload_retry_delay_ms(status.as_u16(), overload_retries) {
                overload_retries += 1;
                tracing::warn!(
                    status = %status,
                    attempt = overload_retries,
                    delay_ms,
                    "AI model busy; retrying after bounded backoff"
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                continue;
            }
            // Bound the transport body before JSON parsing. The 8 MiB
            // accepted-answer cap cannot protect unbounded resp.text().
            let limit = if status.is_success() {
                MAX_CHAT_JSON_RESPONSE_BYTES
            } else {
                MAX_ERROR_RESPONSE_BYTES
            };
            let body_text = crate::util::read_bounded_http_text(resp, limit)
                .await
                .with_context(|| format!("read bounded AI response (HTTP {status})"))?;

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
            log_token_usage(&self.config.model, body.usage.as_ref());
            return body.into_result();
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
        let mut overload_retries = 0usize;

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
                .map_err(|error| {
                    anyhow::anyhow!(
                        "AI streaming HTTP request failed (model={}, endpoint={}, timeout={}s, is_timeout={}, is_connect={}): {:#}",
                        self.config.model,
                        url,
                        self.config.timeout_seconds,
                        error.is_timeout(),
                        error.is_connect(),
                        error
                    )
                })?;

            let status = resp.status();
            if let Some(delay_ms) = overload_retry_delay_ms(status.as_u16(), overload_retries) {
                overload_retries += 1;
                tracing::warn!(
                    status = %status,
                    attempt = overload_retries,
                    delay_ms,
                    "AI streaming model busy; retrying after bounded backoff"
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                continue;
            }
            if !status.is_success() {
                let body_text = crate::util::read_bounded_http_text(resp, MAX_ERROR_RESPONSE_BYTES)
                    .await
                    .with_context(|| format!("read bounded AI error (HTTP {status})"))?;
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
                let body_text =
                    crate::util::read_bounded_http_text(resp, MAX_CHAT_JSON_RESPONSE_BYTES)
                        .await
                        .context("read bounded streaming JSON fallback")?;
                let body: ChatResponse =
                    serde_json::from_str(&body_text).context("parse AI response")?;
                let result = body.into_result()?;
                validate_chat_finish_reason(result.finish_reason.as_deref())?;
                log_token_usage(&self.config.model, result.usage.as_ref());
                let content = result.content;
                anyhow::ensure!(!content.is_empty(), "Empty AI streaming response");
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
            let mut terminal_seen = false;
            let mut received_bytes = 0usize;

            while let Some(chunk) = resp.chunk().await.context("read AI stream chunk")? {
                // A limit per SSE line is not enough: a non-compliant
                // gateway can send unlimited valid, tiny events or pings.
                // Count raw bytes before accumulating/parsing this chunk.
                received_bytes = next_stream_wire_size(received_bytes, chunk.len())?;
                pending.extend_from_slice(&chunk);
                while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
                    anyhow::ensure!(
                        newline <= MAX_STREAM_EVENT_BYTES,
                        "AI stream event exceeded the safe line-size limit"
                    );
                    let mut line = pending.drain(..=newline).collect::<Vec<_>>();
                    line.pop();
                    if let Some(delta) =
                        consume_stream_event(parse_stream_line(&line)?, &mut terminal_seen)?
                    {
                        if !first_text_seen {
                            first_text_seen = true;
                            tracing::info!(
                                model = %self.config.model,
                                ttft_ms = request_started.elapsed().as_millis() as u64,
                                "[AI_TIMING] first response text"
                            );
                        }
                        check_chat_output_budget(content.len(), delta.len())?;
                        on_delta(&delta)?;
                        content.push_str(&delta);
                    }
                }
                anyhow::ensure!(
                    pending.len() <= MAX_STREAM_EVENT_BYTES,
                    "AI stream event exceeded the safe line-size limit"
                );
            }

            if !pending.is_empty() {
                if let Some(delta) =
                    consume_stream_event(parse_stream_line(&pending)?, &mut terminal_seen)?
                {
                    if !first_text_seen {
                        tracing::info!(
                            model = %self.config.model,
                            ttft_ms = request_started.elapsed().as_millis() as u64,
                            "[AI_TIMING] first response text"
                        );
                    }
                    check_chat_output_budget(content.len(), delta.len())?;
                    on_delta(&delta)?;
                    content.push_str(&delta);
                }
            }

            ensure_stream_completed(terminal_seen, &content)?;
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

/// Bound the complete OpenAI-compatible JSON response (including usage
/// metadata). Decoded accepted answer text is separately limited to 8 MiB.
const MAX_CHAT_JSON_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
/// Error payloads are only needed for short diagnostics, not full buffering.
const MAX_ERROR_RESPONSE_BYTES: usize = 32 * 1024;
/// /models is optional for health checking; it may return a large catalog.
const MAX_MODEL_LIST_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
/// Maximum bytes held for a single unterminated SSE event.
const MAX_STREAM_EVENT_BYTES: usize = 1024 * 1024;
/// A broken local gateway must not return an unbounded chat answer even if it
/// silently ignores the advertised max_tokens. Both ordinary and SSE answers
/// share the same decoded UTF-8 byte limit (8 MiB).
const MAX_CHAT_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
/// Include SSE metadata, keepalives and discarded choices in the wire cap.
/// An endpoint can otherwise keep the reader busy forever without producing
/// accepted answer text. This is not a connection deadline.
const MAX_STREAM_WIRE_BYTES: usize = 16 * 1024 * 1024;

fn check_chat_output_budget(existing_bytes: usize, delta_bytes: usize) -> anyhow::Result<()> {
    anyhow::ensure!(
        existing_bytes
            .checked_add(delta_bytes)
            .is_some_and(|total| total <= MAX_CHAT_OUTPUT_BYTES),
        "AI response exceeded the safe output-size limit"
    );
    Ok(())
}

fn next_stream_wire_size(existing_bytes: usize, incoming_bytes: usize) -> anyhow::Result<usize> {
    let total = existing_bytes
        .checked_add(incoming_bytes)
        .ok_or_else(|| anyhow::anyhow!("AI stream exceeded the safe transport-size limit"))?;
    anyhow::ensure!(
        total <= MAX_STREAM_WIRE_BYTES,
        "AI stream exceeded the safe transport-size limit"
    );
    Ok(total)
}

/// Transient capacity signals from OpenAI-compatible local model servers.
fn is_model_overloaded(status: u16) -> bool {
    matches!(status, 429 | 503)
}

/// Retry at most twice for explicit rate/capacity errors, with a deterministic
/// 250ms/500ms backoff. Streaming and non-streaming calls share this policy.
fn overload_retry_delay_ms(status: u16, retries_so_far: usize) -> Option<u64> {
    if !is_model_overloaded(status) {
        return None;
    }
    match retries_so_far {
        0 => Some(250),
        1 => Some(500),
        _ => None,
    }
}

/// A received delta is provisional until an explicit terminal SSE signal.
fn validate_chat_finish_reason(reason: Option<&str>) -> anyhow::Result<()> {
    match reason {
        None | Some("stop") => Ok(()),
        Some(reason) => {
            let printable = reason
                .chars()
                .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .take(40)
                .collect::<String>();
            anyhow::bail!(
                "AI stream did not finish normally (finish_reason={printable}); partial output must not be treated as complete"
            );
        }
    }
}

fn consume_stream_event(
    event: StreamEvent,
    terminal_seen: &mut bool,
) -> anyhow::Result<Option<String>> {
    match event {
        StreamEvent::Ignored => Ok(None),
        StreamEvent::Done => {
            *terminal_seen = true;
            Ok(None)
        }
        StreamEvent::Completion {
            text,
            finish_reason,
        } => {
            anyhow::ensure!(
                !*terminal_seen || text.is_none(),
                "AI stream returned text after its completion marker"
            );
            validate_chat_finish_reason(finish_reason.as_deref())?;
            if finish_reason.is_some() {
                *terminal_seen = true;
            }
            Ok(text)
        }
    }
}

fn ensure_stream_completed(terminal_seen: bool, content: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        terminal_seen,
        "AI stream ended before a terminal marker; refusing incomplete answer"
    );
    anyhow::ensure!(!content.is_empty(), "Empty AI streaming response");
    Ok(())
}

fn parse_stream_line(line: &[u8]) -> anyhow::Result<StreamEvent> {
    anyhow::ensure!(
        line.len() <= MAX_STREAM_EVENT_BYTES,
        "AI stream event too large"
    );
    let line = std::str::from_utf8(line)
        .context("AI stream returned invalid UTF-8")?
        .trim();
    let Some(data) = line.strip_prefix("data:") else {
        return Ok(StreamEvent::Ignored);
    };
    let data = data.trim();
    if data.is_empty() {
        return Ok(StreamEvent::Ignored);
    }
    if data == "[DONE]" {
        return Ok(StreamEvent::Done);
    }
    let body: ChatStreamResponse = serde_json::from_str(data).context("parse AI stream event")?;
    let Some(choice) = body.choices.into_iter().next() else {
        return Ok(StreamEvent::Ignored);
    };
    Ok(StreamEvent::Completion {
        text: choice
            .delta
            .and_then(|delta| delta.content)
            .filter(|text| !text.is_empty()),
        finish_reason: choice.finish_reason,
    })
}

#[cfg(test)]
mod stream_tests {
    use super::parse_stream_line;

    #[test]
    fn reported_token_usage_is_accumulated_without_guessing_missing_metrics() {
        let mut total = super::ChatUsage::default();
        total.accumulate(&super::ChatUsage {
            prompt_tokens: Some(12),
            completion_tokens: None,
            total_tokens: Some(15),
        });
        total.accumulate(&super::ChatUsage {
            prompt_tokens: None,
            completion_tokens: Some(7),
            total_tokens: Some(9),
        });
        assert_eq!(total.prompt_tokens, Some(12));
        assert_eq!(total.completion_tokens, Some(7));
        assert_eq!(total.total_tokens, Some(24));
    }

    #[test]
    fn parses_actual_usage_and_handles_missing_usage() {
        let with_usage = r#"{"choices":[{"message":{"content":"OK"}}],"usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}}"#;
        let result: super::ChatResponse = serde_json::from_str(with_usage).unwrap();
        let usage = result.usage.unwrap();
        assert_eq!(usage.prompt_tokens, Some(12));
        assert_eq!(usage.completion_tokens, Some(3));
        assert_eq!(usage.total_tokens, Some(15));

        let without_usage = r#"{"choices":[{"message":{"content":"OK"}}]}"#;
        let result: super::ChatResponse = serde_json::from_str(without_usage).unwrap();
        assert!(result.usage.is_none());

        // Partial usage metadata is common on OpenAI-compatible local gateways:
        // successful text must not become a parse failure when totals are absent.
        let partial =
            r#"{"choices":[{"message":{"content":"OK"}}],"usage":{"completion_tokens":3}}"#;
        let result: super::ChatResponse = serde_json::from_str(partial).unwrap();
        let usage = result.usage.unwrap();
        assert_eq!(usage.prompt_tokens, None);
        assert_eq!(usage.completion_tokens, Some(3));
        assert_eq!(usage.total_tokens, None);

        let null_usage = r#"{"choices":[{"message":{"content":"OK"}}],"usage":null}"#;
        let result: super::ChatResponse = serde_json::from_str(null_usage).unwrap();
        assert!(result.usage.is_none());
    }

    #[test]
    fn completion_stop_reason_survives_provider_parsing() {
        for (reason, expected) in [
            (r#""finish_reason":"stop""#, Some("stop")),
            (r#""finish_reason":"length""#, Some("length")),
            (
                r#""finish_reason":"content_filter""#,
                Some("content_filter"),
            ),
            (r#""finish_reason":null"#, None),
            ("", None),
        ] {
            let metadata = if reason.is_empty() {
                String::new()
            } else {
                format!(",{reason}")
            };
            let response = format!(
                r#"{{"choices":[{{"message":{{"content":"complete JSON"}}{metadata}}}],"usage":{{"completion_tokens":12}}}}"#
            );
            let result: super::ChatResponse = serde_json::from_str(&response).unwrap();
            let result = result.into_result().unwrap();
            assert_eq!(result.content, "complete JSON");
            assert_eq!(result.finish_reason.as_deref(), expected);
            assert_eq!(result.usage.unwrap().completion_tokens, Some(12));
        }
        let empty: super::ChatResponse = serde_json::from_str(r#"{"choices":[]}"#).unwrap();
        assert!(empty.into_result().is_err());
    }

    #[test]
    fn retry_only_transient_capacity_errors() {
        assert!(super::is_model_overloaded(429));
        assert!(super::is_model_overloaded(503));
        for status in [400, 401, 403, 404, 408, 422, 500, 502] {
            assert!(!super::is_model_overloaded(status));
        }
    }

    #[test]
    fn overloaded_model_backoff_is_bounded_and_excludes_configuration_errors() {
        for status in [429, 503] {
            assert_eq!(super::overload_retry_delay_ms(status, 0), Some(250));
            assert_eq!(super::overload_retry_delay_ms(status, 1), Some(500));
            assert_eq!(super::overload_retry_delay_ms(status, 2), None);
            assert_eq!(super::overload_retry_delay_ms(status, usize::MAX), None);
        }
        for status in [400, 401, 403, 404, 408, 422, 500, 502, 504] {
            assert_eq!(super::overload_retry_delay_ms(status, 0), None);
        }
    }

    #[test]
    fn streaming_missing_terminal_marker_is_not_a_completed_answer() {
        let mut finished = false;
        let event =
            parse_stream_line(br#"data: {"choices":[{"delta":{"content":"partial answer"}}]}"#)
                .unwrap();
        assert_eq!(
            super::consume_stream_event(event, &mut finished)
                .unwrap()
                .as_deref(),
            Some("partial answer")
        );
        assert!(!finished);
        assert!(super::ensure_stream_completed(finished, "partial answer").is_err());

        let stop = parse_stream_line(br#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#)
            .unwrap();
        assert_eq!(
            super::consume_stream_event(stop, &mut finished).unwrap(),
            None
        );
        assert!(super::ensure_stream_completed(finished, "complete answer").is_ok());
        assert!(super::ensure_stream_completed(finished, "").is_err());
    }

    #[test]
    fn streaming_done_allows_legacy_no_reason_but_refuses_late_text() {
        let mut finished = false;
        let done = parse_stream_line(b"data: [DONE]").unwrap();
        assert_eq!(
            super::consume_stream_event(done, &mut finished).unwrap(),
            None
        );
        assert!(finished);
        assert!(super::ensure_stream_completed(finished, "legacy answer").is_ok());
        let extra =
            parse_stream_line(br#"data: {"choices":[{"delta":{"content":"late"}}]}"#).unwrap();
        assert!(super::consume_stream_event(extra, &mut finished).is_err());
    }

    #[test]
    fn streaming_abnormal_completion_rejects_valid_looking_text() {
        for reason in [
            "length",
            "content_filter",
            "tool_calls",
            "function_call",
            "error",
        ] {
            let input = format!(
                r#"data: {{"choices":[{{"delta":{{"content":"complete"}},"finish_reason":"{reason}"}}]}}"#
            );
            let mut finished = false;
            let event = parse_stream_line(input.as_bytes()).unwrap();
            assert!(super::consume_stream_event(event, &mut finished).is_err());
            assert!(!finished);
        }
        let json = r#"{"choices":[{"message":{"content":"complete"},"finish_reason":"length"}]}"#;
        let fallback: super::ChatResponse = serde_json::from_str(json).unwrap();
        let fallback = fallback.into_result().unwrap();
        let abnormal = super::validate_chat_finish_reason(fallback.finish_reason.as_deref());
        assert!(abnormal.is_err());
        assert!(super::validate_chat_finish_reason(None).is_ok());
        assert!(super::validate_chat_finish_reason(Some("stop")).is_ok());
    }

    #[test]
    fn streamed_choices_are_not_accidentally_concatenated() {
        let event = parse_stream_line(
            br#"data: {"choices":[{"delta":{"content":"first"}},{"delta":{"content":"second"}}]}"#,
        )
        .unwrap();
        assert_eq!(
            event,
            super::StreamEvent::Completion {
                text: Some("first".into()),
                finish_reason: None,
            }
        );
        assert_eq!(
            parse_stream_line(br#"data: {"choices":[],"usage":{"total_tokens":42}}"#).unwrap(),
            super::StreamEvent::Ignored
        );
    }

    #[test]
    fn answer_budget_applies_to_streaming_and_nonstreaming_completions() {
        // Check the end-of-buffer boundary without allocating giant strings:
        // every UTF-8 byte (including multi-byte Chinese) counts.
        assert!(super::check_chat_output_budget(0, 0).is_ok());
        assert!(super::check_chat_output_budget(0, "中文".len()).is_ok());
        assert!(super::check_chat_output_budget(super::MAX_CHAT_OUTPUT_BYTES - 3, 3).is_ok());
        assert!(super::check_chat_output_budget(super::MAX_CHAT_OUTPUT_BYTES, 1).is_err());
        assert!(super::check_chat_output_budget(usize::MAX, 1).is_err());

        // A JSON fallback and an ordinary completion both pass through
        // ChatResponse::into_result and cannot bypass the decoded-text cap.
        let response = super::ChatResponse {
            choices: vec![super::Choice {
                message: super::AssistantMessage {
                    content: "answer".to_string(),
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: None,
        };
        assert_eq!(response.into_result().unwrap().content, "answer");
        let oversized_response = super::ChatResponse {
            choices: vec![super::Choice {
                message: super::AssistantMessage {
                    content: "x".repeat(super::MAX_CHAT_OUTPUT_BYTES + 1),
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: None,
        };
        assert!(oversized_response.into_result().is_err());
    }

    #[test]
    fn streamed_transport_budget_counts_ignored_heartbeat_frames() {
        assert_eq!(super::next_stream_wire_size(0, 10).unwrap(), 10);
        assert_eq!(
            super::next_stream_wire_size(super::MAX_STREAM_WIRE_BYTES - 1, 1).unwrap(),
            super::MAX_STREAM_WIRE_BYTES
        );
        assert!(super::next_stream_wire_size(super::MAX_STREAM_WIRE_BYTES, 1).is_err());
        assert!(super::next_stream_wire_size(usize::MAX, 1).is_err());
        // Even an event that does not produce answer text consumes wire
        // budget; discarding a usage-only SSE frame cannot reset the cap.
        let ignored =
            parse_stream_line(br#"data: {"choices":[],"usage":{"total_tokens":42}}"#).unwrap();
        assert_eq!(ignored, super::StreamEvent::Ignored);
    }

    #[test]
    fn json_transport_limits_are_distinct_from_decoded_answer_limits() {
        assert!(super::MAX_CHAT_JSON_RESPONSE_BYTES > super::MAX_CHAT_OUTPUT_BYTES);
        assert!(super::MAX_ERROR_RESPONSE_BYTES < super::MAX_CHAT_OUTPUT_BYTES);
        assert!(super::MAX_MODEL_LIST_RESPONSE_BYTES < super::MAX_CHAT_JSON_RESPONSE_BYTES);
    }

    #[test]
    fn oversized_unterminated_sse_event_is_rejected() {
        let oversized = vec![b'x'; super::MAX_STREAM_EVENT_BYTES + 1];
        assert!(parse_stream_line(&oversized).is_err());
    }

    #[test]
    fn parses_openai_stream_delta_and_ignores_done() {
        let delta =
            parse_stream_line(br#"data: {"choices":[{"delta":{"content":"hello"}}]}"#).unwrap();
        assert_eq!(
            delta,
            super::StreamEvent::Completion {
                text: Some("hello".into()),
                finish_reason: None,
            }
        );
        assert_eq!(
            parse_stream_line(b"data: [DONE]").unwrap(),
            super::StreamEvent::Done
        );
        assert_eq!(
            parse_stream_line(b"event: message").unwrap(),
            super::StreamEvent::Ignored
        );
    }
}
