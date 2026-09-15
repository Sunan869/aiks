// CI lint baseline: pre-existing Clippy debt; remove allowances incrementally.
#![allow(clippy::should_implement_trait)]

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod hash;
pub mod pipeline;

/// AI session source kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    ClaudeCode,
    Codex,
    GeminiCli,
    OpenCode,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceKind::ClaudeCode => "claude_code",
            SourceKind::Codex => "codex",
            SourceKind::GeminiCli => "gemini_cli",
            SourceKind::OpenCode => "opencode",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            SourceKind::ClaudeCode => "Claude Code",
            SourceKind::Codex => "Codex",
            SourceKind::GeminiCli => "Gemini CLI",
            SourceKind::OpenCode => "OpenCode",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude_code" | "claude" | "claudecode" => Some(SourceKind::ClaudeCode),
            "codex" => Some(SourceKind::Codex),
            "gemini_cli" | "gemini" | "geminicli" => Some(SourceKind::GeminiCli),
            "opencode" | "open_code" => Some(SourceKind::OpenCode),
            _ => None,
        }
    }
}

/// A fully normalized session from any AI tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedSession {
    pub source: SourceKind,
    /// The original session ID from the source tool
    pub external_session_id: String,
    pub title: Option<String>,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
    /// Path to the source file/db
    pub source_path: Option<PathBuf>,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub model: Option<String>,
    pub messages: Vec<NormalizedMessage>,
    pub usage: Option<SessionUsage>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl NormalizedSession {
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Extract project name from project_path
    pub fn infer_project_name(&self) -> Option<String> {
        self.project_name.clone().or_else(|| {
            self.project_path
                .as_ref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .map(|n| n.to_string_lossy().to_string())
        })
    }
}

/// A normalized message from any AI tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedMessage {
    pub external_id: String,
    pub parent_id: Option<String>,
    pub role: MessageRole,
    pub created_at: Option<DateTime<Utc>>,
    pub model: Option<String>,
    pub blocks: Vec<ContentBlock>,
    pub usage: Option<MessageUsage>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Message roles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
    Unknown,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::Tool => "tool",
            MessageRole::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "user" | "human" => MessageRole::User,
            "assistant" | "gemini" | "model" => MessageRole::Assistant,
            "system" => MessageRole::System,
            "tool" => MessageRole::Tool,
            _ => MessageRole::Unknown,
        }
    }
}

/// Content block types in a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    ToolCall {
        id: Option<String>,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        id: Option<String>,
        content: String,
        is_error: bool,
    },
    Image {
        source: String,
        media_type: Option<String>,
    },
    FileReference {
        path: String,
        name: Option<String>,
    },
    Unknown {
        raw: serde_json::Value,
    },
}

impl ContentBlock {
    /// Returns true if this block contains user-visible text
    pub fn has_text(&self) -> bool {
        matches!(
            self,
            ContentBlock::Text { .. }
                | ContentBlock::Thinking { .. }
                | ContentBlock::ToolCall { .. }
                | ContentBlock::ToolResult { .. }
        )
    }

    /// Extract text content for display/search
    pub fn text_content(&self) -> Option<&str> {
        match self {
            ContentBlock::Text { text } => Some(text.as_str()),
            ContentBlock::Thinking { text } => Some(text.as_str()),
            ContentBlock::ToolResult { content, .. } => Some(content.as_str()),
            _ => None,
        }
    }
}

/// Session-level token usage summary
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionUsage {
    pub total_input_tokens: Option<u64>,
    pub total_output_tokens: Option<u64>,
    pub total_cache_read_tokens: Option<u64>,
    pub total_cache_creation_tokens: Option<u64>,
}

impl SessionUsage {
    pub fn add_message_usage(&mut self, usage: &MessageUsage) {
        self.total_input_tokens =
            Some(self.total_input_tokens.unwrap_or(0) + usage.input_tokens.unwrap_or(0));
        self.total_output_tokens =
            Some(self.total_output_tokens.unwrap_or(0) + usage.output_tokens.unwrap_or(0));
        if let Some(cr) = usage.cache_read_tokens {
            self.total_cache_read_tokens = Some(self.total_cache_read_tokens.unwrap_or(0) + cr);
        }
        if let Some(cw) = usage.cache_creation_tokens {
            self.total_cache_creation_tokens =
                Some(self.total_cache_creation_tokens.unwrap_or(0) + cw);
        }
    }
}

/// Per-message token usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_creation_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_kind_roundtrip() {
        for kind in [
            SourceKind::ClaudeCode,
            SourceKind::Codex,
            SourceKind::GeminiCli,
            SourceKind::OpenCode,
        ] {
            assert_eq!(SourceKind::from_str(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn message_role_from_str() {
        assert_eq!(MessageRole::from_str("user"), MessageRole::User);
        assert_eq!(MessageRole::from_str("human"), MessageRole::User);
        assert_eq!(MessageRole::from_str("assistant"), MessageRole::Assistant);
        assert_eq!(MessageRole::from_str("gemini"), MessageRole::Assistant);
        assert_eq!(MessageRole::from_str("system"), MessageRole::System);
        assert_eq!(MessageRole::from_str("unknown_role"), MessageRole::Unknown);
    }

    #[test]
    fn session_usage_accumulation() {
        let mut usage = SessionUsage::default();
        usage.add_message_usage(&MessageUsage {
            input_tokens: Some(100),
            output_tokens: Some(50),
            cache_read_tokens: Some(20),
            cache_creation_tokens: None,
        });
        usage.add_message_usage(&MessageUsage {
            input_tokens: Some(200),
            output_tokens: Some(100),
            cache_read_tokens: Some(30),
            cache_creation_tokens: Some(10),
        });
        assert_eq!(usage.total_input_tokens, Some(300));
        assert_eq!(usage.total_output_tokens, Some(150));
        assert_eq!(usage.total_cache_read_tokens, Some(50));
        assert_eq!(usage.total_cache_creation_tokens, Some(10));
    }
}
