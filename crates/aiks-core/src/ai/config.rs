use serde::{Deserialize, Serialize};

/// Configuration for the OpenAI-compatible AI model.
///
/// Default: disabled, with a loopback OpenAI-compatible endpoint example.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiModelConfig {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub timeout_seconds: u64,
    /// Minimum knowledge_score to create a knowledge document (0.0-1.0)
    pub min_knowledge_score: f32,
    /// Max messages per chunk for long sessions
    pub chunk_size_messages: usize,
    /// Max concurrent extractions
    pub max_concurrent: usize,
    /// Debounce minutes before extracting a recently-updated session
    pub debounce_minutes: u64,
    /// R07: automatically run AI extraction after sync (Settings UI field).
    pub auto_extract: bool,
    /// Hard-disable Qwen3 thinking mode via `chat_template_kwargs`
    /// (vLLM). Leaked reasoning text both pollutes the JSON output and
    /// burns the max_tokens budget, producing truncated unparseable JSON.
    pub disable_thinking: bool,
    /// Total model context window (prompt + output must fit inside it).
    /// Used by the pipeline to cap chunk/prompt sizes BEFORE the request;
    /// without it a single oversized chunk can never succeed at any
    /// max_tokens (observed: 31.7K-token chunk against a 32K window).
    pub max_context_tokens: usize,
}

impl Default for AiModelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://127.0.0.1:11434/v1".to_string(),
            model: "qwen3".to_string(),
            api_key: None,
            temperature: 0.1,
            // 8192: with thinking disabled, large sessions still produce
            // 15-20 KB of pure JSON; 4096 tokens truncated mid-array
            // (observed: "EOF while parsing a list at line 216").
            max_tokens: 8192,
            timeout_seconds: 120,
            min_knowledge_score: 0.6,
            chunk_size_messages: 40,
            max_concurrent: 1,
            debounce_minutes: 10,
            auto_extract: true,
            disable_thinking: true,
            max_context_tokens: 32768,
        }
    }
}

impl AiModelConfig {
    /// User-friendly display name for the model provider
    pub fn display_name(&self) -> &str {
        if self.base_url.contains("api.openai.com") {
            "OpenAI"
        } else if self.base_url.contains("127.0.0.1") || self.base_url.contains("localhost") {
            "本地 AI 服务"
        } else {
            "自定义 AI 服务"
        }
    }
}
