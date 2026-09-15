// CI lint baseline: pre-existing Clippy debt; remove allowances incrementally.
#![allow(
    clippy::ptr_arg,
    clippy::redundant_closure,
    clippy::type_complexity,
    clippy::unnecessary_sort_by
)]

/// Gemini CLI session provider.
///
/// Session storage: ~/.gemini/tmp/{project}/chats/session-*.json
/// Each JSON file contains a complete session with messages array.
///
/// Format: { sessionId, startTime, messages: [{type, id, content, toolCalls, thoughts, tokens, timestamp, model}] }
/// Message types: "user" | "gemini" | "info" | "error"
///
/// Derived from: AICoder Session Viewer (MIT)
/// Original: https://github.com/seastart/aicoder-session-viewer
/// Commit: b750098594c3bb969d5039a9f2e44a283c38af9b
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use walkdir::WalkDir;

use crate::model::{
    ContentBlock, MessageRole, MessageUsage, NormalizedMessage, NormalizedSession, SessionUsage,
    SourceKind,
};
use crate::providers::{ProviderHealth, SessionSummary};

const PARSER_VERSION: &str = "gemini-json-v1";

pub struct GeminiProvider {
    base_dir: PathBuf,
}

impl GeminiProvider {
    pub fn default_path() -> anyhow::Result<PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot locate home directory"))?;
        Ok(home.join(".gemini"))
    }

    pub fn new(path_override: Option<PathBuf>) -> anyhow::Result<Self> {
        let base_dir = match path_override {
            Some(p) => p,
            None => Self::default_path()?,
        };
        Ok(Self { base_dir })
    }

    fn find_session_files(&self) -> Vec<PathBuf> {
        let tmp_dir = self.base_dir.join("tmp");
        if !tmp_dir.exists() {
            return Vec::new();
        }

        WalkDir::new(&tmp_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.extension().is_some_and(|ext| ext == "json")
                    && path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with("session-"))
            })
            .map(|e| e.path().to_path_buf())
            .collect()
    }

    fn session_id_from_path(path: &PathBuf) -> String {
        path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }

    /// Read the real project path from .project_root file.
    /// Path structure: ~/.gemini/tmp/{project}/chats/session-*.json
    fn project_from_path(path: &PathBuf) -> Option<String> {
        let project_dir = path.parent()?.parent()?; // ~/.gemini/tmp/{project}/
        let project_root_file = project_dir.join(".project_root");
        if let Ok(content) = fs::read_to_string(&project_root_file) {
            let trimmed = content.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
        project_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
    }

    fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    fn extract_title_from_messages(msgs: &[serde_json::Value]) -> Option<String> {
        for msg in msgs {
            if msg.get("type").and_then(|t| t.as_str()) != Some("user") {
                continue;
            }
            if let Some(arr) = msg.get("content").and_then(|c| c.as_array()) {
                if let Some(text) = arr
                    .first()
                    .and_then(|item| item.get("text"))
                    .and_then(|t| t.as_str())
                {
                    let clean = text.trim();
                    if clean.is_empty() {
                        continue;
                    }
                    let first_line = clean.lines().next().unwrap_or(clean);
                    let truncated: String = first_line.chars().take(60).collect();
                    if truncated.len() < first_line.len() {
                        return Some(format!("{}...", truncated));
                    }
                    return Some(truncated);
                }
            }
        }
        None
    }

    fn parse_message(msg: &serde_json::Value, index: usize) -> Option<NormalizedMessage> {
        let msg_type = msg.get("type")?.as_str()?;

        let role = match msg_type {
            "user" => MessageRole::User,
            "gemini" => MessageRole::Assistant,
            "info" | "error" => MessageRole::System,
            _ => return None,
        };

        let mut blocks = Vec::new();

        match msg_type {
            "user" => {
                if let Some(arr) = msg.get("content").and_then(|c| c.as_array()) {
                    for item in arr {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                blocks.push(ContentBlock::Text {
                                    text: text.to_string(),
                                });
                            }
                        } else if let Some(inline) = item.get("inlineData") {
                            let data = inline
                                .get("data")
                                .and_then(|d| d.as_str())
                                .unwrap_or("")
                                .to_string();
                            if !data.is_empty() {
                                let media_type = inline
                                    .get("mimeType")
                                    .and_then(|m| m.as_str())
                                    .map(|s| s.to_string());
                                blocks.push(ContentBlock::Image {
                                    source: data,
                                    media_type,
                                });
                            }
                        }
                    }
                }
            }
            "gemini" => {
                // Thoughts
                if let Some(thoughts) = msg.get("thoughts").and_then(|t| t.as_array()) {
                    for thought in thoughts {
                        let subject = thought
                            .get("subject")
                            .and_then(|s| s.as_str())
                            .unwrap_or("");
                        let desc = thought
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("");
                        let text = if subject.is_empty() {
                            desc.to_string()
                        } else {
                            format!("**{}**\n{}", subject, desc)
                        };
                        if !text.is_empty() {
                            blocks.push(ContentBlock::Thinking { text });
                        }
                    }
                }

                // Tool calls
                if let Some(tool_calls) = msg.get("toolCalls").and_then(|t| t.as_array()) {
                    for tc in tool_calls {
                        let name = tc
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let id = tc
                            .get("id")
                            .and_then(|id| id.as_str())
                            .map(|s| s.to_string());
                        let input = tc.get("args").cloned().unwrap_or(serde_json::Value::Null);

                        blocks.push(ContentBlock::ToolCall {
                            id: id.clone(),
                            name,
                            input,
                        });

                        // Tool results embedded in toolCalls[].result
                        if let Some(results) = tc.get("result").and_then(|r| r.as_array()) {
                            for res in results {
                                if let Some(fr) = res.get("functionResponse") {
                                    let output = fr
                                        .get("response")
                                        .and_then(|r| {
                                            r.get("output")
                                                .and_then(|o| o.as_str())
                                                .map(|s| s.to_string())
                                                .or_else(|| {
                                                    r.get("error")
                                                        .and_then(|e| e.as_str())
                                                        .map(|s| s.to_string())
                                                })
                                                .or_else(|| serde_json::to_string_pretty(r).ok())
                                        })
                                        .unwrap_or_default();
                                    let is_error =
                                        fr.get("response").and_then(|r| r.get("error")).is_some();
                                    blocks.push(ContentBlock::ToolResult {
                                        id: id.clone(),
                                        content: output,
                                        is_error,
                                    });
                                }
                            }
                        }
                    }
                }

                // Main text content
                if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                    if !text.is_empty() {
                        blocks.push(ContentBlock::Text {
                            text: text.to_string(),
                        });
                    }
                }
            }
            "info" | "error" => {
                if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                    if !text.is_empty() {
                        let prefix = if msg_type == "error" { "⚠ " } else { "" };
                        blocks.push(ContentBlock::Text {
                            text: format!("{}{}", prefix, text),
                        });
                    }
                }
            }
            _ => {}
        }

        if blocks.is_empty() {
            return None;
        }

        let timestamp = msg
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(|s| Self::parse_timestamp(s));

        let usage = msg.get("tokens").map(|tokens| MessageUsage {
            input_tokens: tokens.get("input").and_then(|v| v.as_u64()),
            output_tokens: tokens.get("output").and_then(|v| v.as_u64()),
            cache_read_tokens: tokens.get("cached").and_then(|v| v.as_u64()),
            cache_creation_tokens: None,
        });

        let external_id = msg
            .get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("gemini-msg-{}", index));

        Some(NormalizedMessage {
            external_id,
            parent_id: None,
            role,
            created_at: timestamp,
            model: msg
                .get("model")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string()),
            blocks,
            usage,
            metadata: HashMap::new(),
        })
    }

    fn parse_session_file(
        path: &PathBuf,
    ) -> anyhow::Result<(
        Vec<NormalizedMessage>,
        Option<String>,
        Option<DateTime<Utc>>,
        Option<String>,
    )> {
        let content = fs::read_to_string(path)?;
        let data: serde_json::Value = serde_json::from_str(&content)?;

        let real_id = data
            .get("sessionId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let start_time = data
            .get("startTime")
            .and_then(|t| t.as_str())
            .and_then(|s| Self::parse_timestamp(s));

        let msgs_array = data.get("messages").and_then(|m| m.as_array());
        let title = msgs_array.and_then(|m| Self::extract_title_from_messages(m));

        let messages = msgs_array
            .map(|msgs| {
                msgs.iter()
                    .enumerate()
                    .filter_map(|(i, msg)| Self::parse_message(msg, i))
                    .collect()
            })
            .unwrap_or_default();

        Ok((messages, title, start_time, real_id))
    }
}

#[async_trait]
impl super::SessionProvider for GeminiProvider {
    fn source(&self) -> SourceKind {
        SourceKind::GeminiCli
    }

    fn parser_version(&self) -> &'static str {
        PARSER_VERSION
    }

    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        let files = self.find_session_files();
        let mut summaries = Vec::new();

        for path in &files {
            let fallback_id = Self::session_id_from_path(path);
            let project = Self::project_from_path(path);

            let mtime = fs::metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from);

            // Parse file to get real session ID
            let (real_id, msg_count, title, start_time) = match fs::read_to_string(path) {
                Ok(content) => {
                    let data: serde_json::Value =
                        serde_json::from_str(&content).unwrap_or_default();
                    let real_id = data
                        .get("sessionId")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let msgs = data.get("messages").and_then(|m| m.as_array());
                    let count = msgs.map(|a| a.len()).unwrap_or(0);
                    let title = msgs.and_then(|m| Self::extract_title_from_messages(m));
                    let start_time = data
                        .get("startTime")
                        .and_then(|t| t.as_str())
                        .and_then(|s| Self::parse_timestamp(s));
                    (real_id, count, title, start_time)
                }
                Err(_) => (None, 0, None, None),
            };

            summaries.push(SessionSummary {
                source: SourceKind::GeminiCli,
                external_session_id: real_id.unwrap_or(fallback_id),
                title,
                project_name: None,
                project_path: project,
                source_path: Some(path.clone()),
                started_at: start_time.or(mtime),
                updated_at: mtime,
                message_count: msg_count,
            });
        }

        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }

    async fn load_session(&self, summary: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        // Find the file by session ID
        let files = self.find_session_files();
        let path = files
            .iter()
            .find(|p| {
                // Match by real sessionId in JSON
                if let Ok(content) = fs::read_to_string(p) {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                        if data.get("sessionId").and_then(|v| v.as_str())
                            == Some(&summary.external_session_id)
                        {
                            return true;
                        }
                    }
                }
                // Fall back to file name
                Self::session_id_from_path(p) == summary.external_session_id
            })
            .cloned()
            .or_else(|| summary.source_path.clone())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Gemini session file not found: {}",
                    summary.external_session_id
                )
            })?;

        let (messages, title, start_time, _) = Self::parse_session_file(&path)?;

        let mut usage = SessionUsage::default();
        let mut has_usage = false;
        for msg in &messages {
            if let Some(u) = &msg.usage {
                usage.add_message_usage(u);
                has_usage = true;
            }
        }

        let updated_at = messages.last().and_then(|m| m.created_at);

        Ok(NormalizedSession {
            source: SourceKind::GeminiCli,
            external_session_id: summary.external_session_id.clone(),
            title: title.or_else(|| summary.title.clone()),
            project_name: None,
            project_path: summary.project_path.clone(),
            source_path: Some(path),
            started_at: start_time.or(summary.started_at),
            updated_at,
            model: messages
                .iter()
                .filter(|m| m.role == MessageRole::Assistant)
                .find_map(|m| m.model.clone()),
            messages,
            usage: if has_usage { Some(usage) } else { None },
            metadata: HashMap::new(),
        })
    }

    async fn health_check(&self) -> ProviderHealth {
        if !self.base_dir.exists() {
            return ProviderHealth::NotFound {
                message: format!("~/.gemini not found at {}", self.base_dir.display()),
            };
        }
        ProviderHealth::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_session_file(dir: &std::path::Path, content: &str) -> PathBuf {
        let chats_dir = dir.join("tmp").join("my-project").join("chats");
        fs::create_dir_all(&chats_dir).unwrap();
        let path = chats_dir.join("session-abc123.json");
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn parses_gemini_session() {
        let dir = tempdir().unwrap();
        let content = r#"{
  "sessionId": "real-session-id",
  "startTime": "2024-01-01T00:00:00Z",
  "messages": [
    {
      "type": "user",
      "id": "user-1",
      "content": [{"text": "How do I sort a list in Python?"}],
      "timestamp": "2024-01-01T00:00:00Z"
    },
    {
      "type": "gemini",
      "id": "gemini-1",
      "content": "You can use sorted() or list.sort()",
      "toolCalls": [
        {
          "id": "tool-1",
          "name": "code_execution",
          "args": {"code": "sorted([3,1,2])"},
          "result": [{"functionResponse": {"response": {"output": "[1, 2, 3]"}}}]
        }
      ],
      "tokens": {"input": 100, "output": 50},
      "timestamp": "2024-01-01T00:00:01Z",
      "model": "gemini-2.5-pro"
    }
  ]
}"#;
        let path = write_session_file(dir.path(), content);
        let (messages, title, start_time, real_id) =
            GeminiProvider::parse_session_file(&path).unwrap();

        assert_eq!(real_id.as_deref(), Some("real-session-id"));
        assert_eq!(title.as_deref(), Some("How do I sort a list in Python?"));
        assert!(start_time.is_some());
        assert_eq!(messages.len(), 2);

        // User message
        assert_eq!(messages[0].role, MessageRole::User);
        assert!(
            matches!(&messages[0].blocks[0], ContentBlock::Text { text } if text.contains("sort"))
        );

        // Assistant message - should have ToolCall + ToolResult + Text
        assert_eq!(messages[1].role, MessageRole::Assistant);
        let tool_call = messages[1].blocks.iter().find_map(|b| match b {
            ContentBlock::ToolCall { name, .. } => Some(name.clone()),
            _ => None,
        });
        assert_eq!(tool_call.as_deref(), Some("code_execution"));

        let tool_result = messages[1].blocks.iter().find_map(|b| match b {
            ContentBlock::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        });
        assert_eq!(tool_result.as_deref(), Some("[1, 2, 3]"));

        assert_eq!(messages[1].model.as_deref(), Some("gemini-2.5-pro"));
        assert!(messages[1].usage.is_some());
    }
}
