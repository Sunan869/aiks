use serde::{Deserialize, Serialize};

/// Configuration for the OpenAI-compatible AI model.
///
/// Default: Company internal vLLM at http://10.10.23.16:18000/v1 with Qwen3.8-27B.
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
}

impl Default for AiModelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: "http://10.10.23.16:18000/v1".to_string(),
            model: "Qwen3.8-27B".to_string(),
            api_key: None,
            temperature: 0.1,
            max_tokens: 4096,
            timeout_seconds: 120,
            min_knowledge_score: 0.6,
            chunk_size_messages: 40,
            max_concurrent: 1,
            debounce_minutes: 10,
        }
    }
}

impl AiModelConfig {
    /// User-friendly display name for the model provider
    pub fn display_name(&self) -> &str {
        if self.base_url.contains("10.10.23") {
            "公司内部 AI"
        } else if self.base_url.contains("openai") {
            "OpenAI"
        } else {
            "自定义 AI 服务"
        }
    }
}
