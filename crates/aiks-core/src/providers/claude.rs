// CI lint baseline: pre-existing Clippy debt; remove allowances incrementally.
#![allow(clippy::bind_instead_of_map, clippy::useless_format)]

/// Claude Code session provider.
///
/// Session storage: ~/.claude/projects/{project-hash}/{session-uuid}.jsonl
/// Each JSONL line is a JSON event with type: user|assistant|system|summary|progress
///
/// Derived from: AICoder Session Viewer (MIT)
/// Original: https://github.com/seastart/aicoder-session-viewer
/// Commit: b750098594c3bb969d5039a9f2e44a283c38af9b
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::model::{
    ContentBlock, MessageRole, MessageUsage, NormalizedMessage, NormalizedSession, SessionUsage,
    SourceKind,
};
use crate::providers::{ProviderHealth, SessionSummary};

const PARSER_VERSION: &str = "claude-v1";

type ClaudeSessionSummary = (Option<String>, usize, Option<String>, Option<DateTime<Utc>>);

pub struct ClaudeProvider {
    base_dir: PathBuf,
}

impl ClaudeProvider {
    pub fn default_path() -> anyhow::Result<PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot locate home directory"))?;
        Ok(home.join(".claude"))
    }

    pub fn new(path_override: Option<PathBuf>) -> anyhow::Result<Self> {
        let base_dir = match path_override {
            Some(p) => p,
            None => Self::default_path()?,
        };
        Ok(Self { base_dir })
    }

    fn projects_dir(&self) -> PathBuf {
        self.base_dir.join("projects")
    }

    /// Scan projects directory for all main session JSONL files (excluding subagents/).
    fn scan_jsonl_files(&self) -> Vec<(String, PathBuf, String)> {
        // Returns (session_id, file_path, project_dir_name)
        let projects_dir = self.projects_dir();
        if !projects_dir.exists() {
            return Vec::new();
        }

        let mut results = Vec::new();

        for project_entry in fs::read_dir(&projects_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
        {
            let project_path = project_entry.path();
            if !project_path.is_dir() {
                continue;
            }
            let project_dir_name = project_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // Only scan direct .jsonl files (not in subagents/)
            let Ok(entries) = fs::read_dir(&project_path) else {
                continue;
            };
            for file_entry in entries.flatten() {
                let file_path = file_entry.path();
                if file_path.extension().is_some_and(|e| e == "jsonl") && file_path.is_file() {
                    let session_id = file_path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    results.push((session_id, file_path, project_dir_name.clone()));
                }
            }
        }
        results
    }

    /// Convert encoded project dir name back to real path.
    /// "-Users-alice-Projects-myapp" → "/Users/alice/Projects/myapp"
    /// "-C-Users-alice-project" (Windows) → "C:/Users/alice/project"
    fn dir_name_to_path(dir_name: &str) -> String {
        if !dir_name.starts_with('-') {
            return dir_name.replace('-', "/");
        }
        let raw = format!("/{}", &dir_name[1..]).replace('-', "/");
        // Detect Windows drive letter: /C/Users/... → C:/Users/...
        if raw.len() >= 3 {
            let bytes = raw.as_bytes();
            if bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b'/' {
                let drive = bytes[1] as char;
                return format!("{}:{}", drive, &raw[2..]);
            }
        }
        raw
    }

    /// Extract session summary from JSONL content in a single pass.
    fn scan_summary(content: &str) -> anyhow::Result<ClaudeSessionSummary> {
        let mut title: Option<String> = None;
        let mut cwd: Option<String> = None;
        let mut started_at: Option<DateTime<Utc>> = None;
        let mut count = 0usize;

        let lines = content.lines().collect::<Vec<_>>();
        for (index, raw_line) in lines.iter().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            let parsed = serde_json::from_str::<serde_json::Value>(line);
            let entry = match parsed {
                Ok(entry) => entry,
                Err(error) => {
                    let is_last_nonempty = lines[index + 1..]
                        .iter()
                        .all(|value| value.trim().is_empty());
                    if is_last_nonempty && !content.ends_with('\n') {
                        break;
                    }
                    anyhow::bail!(
                        "Claude transcript contains malformed JSON at line {}: {}",
                        index + 1,
                        error
                    );
                }
            };

            let is_user = matches!(
                entry.get("type").and_then(|value| value.as_str()),
                Some("user" | "human")
            );
            let is_assistant =
                entry.get("type").and_then(|value| value.as_str()) == Some("assistant");
            if is_user || is_assistant {
                count += 1;
            }

            let need_parse = cwd.is_none() || started_at.is_none() || (title.is_none() && is_user);
            if !need_parse {
                continue;
            }

            if cwd.is_none() {
                if let Some(c) = entry.get("cwd").and_then(|v| v.as_str()) {
                    cwd = Some(c.to_string());
                }
            }
            if started_at.is_none() {
                if let Some(ts) = entry.get("timestamp").and_then(|t| t.as_str()) {
                    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
                        started_at = Some(dt.with_timezone(&Utc));
                    }
                }
            }
            if title.is_none() && is_user {
                title = Self::extract_title(&entry);
            }
        }

        Ok((title, count, cwd, started_at))
    }

    fn extract_title(entry: &serde_json::Value) -> Option<String> {
        let content = entry.get("message").and_then(|m| m.get("content"))?;
        let text = match content {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => arr
                .iter()
                .find_map(|block| {
                    if block.get("type")?.as_str()? == "text" {
                        block.get("text")?.as_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
            _ => return None,
        };
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.starts_with('<') || trimmed.starts_with('[') {
            return None;
        }
        let oneline: String = trimmed
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let title: String = oneline.chars().take(80).collect();
        Some(if title.len() < oneline.len() {
            format!("{}...", title)
        } else {
            title
        })
    }

    /// Parse a JSONL file into NormalizedMessages.
    fn parse_jsonl(path: &PathBuf) -> anyhow::Result<Vec<NormalizedMessage>> {
        let content = fs::read_to_string(path)?;
        let mut messages = Vec::new();
        // Collect agent_id mapping from progress events (tool_use_id → agent_id)
        let mut agent_map: HashMap<String, String> = HashMap::new();

        let lines = content.lines().collect::<Vec<_>>();
        for (index, raw_line) in lines.iter().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            let entry = match serde_json::from_str::<serde_json::Value>(line) {
                Ok(entry) => entry,
                Err(error) => {
                    let is_last_nonempty = lines[index + 1..]
                        .iter()
                        .all(|value| value.trim().is_empty());
                    if is_last_nonempty && !content.ends_with('\n') {
                        anyhow::bail!("Claude transcript is still being written; retry later");
                    }
                    anyhow::bail!(
                        "Claude transcript contains malformed JSON at line {}: {}",
                        index + 1,
                        error
                    );
                }
            };
            {
                if entry.get("type").and_then(|t| t.as_str()) == Some("progress") {
                    let parent_id = entry.get("parentToolUseID").and_then(|v| v.as_str());
                    let agent_id = entry
                        .get("data")
                        .and_then(|d| d.get("agentId"))
                        .and_then(|v| v.as_str());
                    if let (Some(tid), Some(aid)) = (parent_id, agent_id) {
                        if !aid.is_empty() {
                            agent_map.insert(tid.to_string(), aid.to_string());
                        }
                    }
                }
            }
        }

        let mut msg_index = 0usize;
        for (index, raw_line) in lines.iter().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            let entry = serde_json::from_str::<serde_json::Value>(line).map_err(|error| {
                anyhow::anyhow!(
                    "Claude transcript contains malformed JSON at line {}: {}",
                    index + 1,
                    error
                )
            })?;
            if let Some(msg) = Self::parse_entry(&entry, msg_index, &agent_map) {
                msg_index += 1;
                messages.push(msg);
            }
        }
        Ok(messages)
    }

    fn parse_entry(
        entry: &serde_json::Value,
        index: usize,
        agent_map: &HashMap<String, String>,
    ) -> Option<NormalizedMessage> {
        let msg_type = entry.get("type")?.as_str()?;
        let message = entry.get("message")?;

        let role = match msg_type {
            "human" | "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            _ => return None,
        };

        let blocks = Self::parse_content(message.get("content")?, agent_map);
        if blocks.is_empty() && role != MessageRole::System {
            return None;
        }

        let timestamp = entry
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let usage = message.get("usage").and_then(|u| {
            let raw_input = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let cache_read = u
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cache_creation = u
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            Some(MessageUsage {
                input_tokens: Some(raw_input + cache_read + cache_creation),
                output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()),
                cache_read_tokens: if cache_read > 0 {
                    Some(cache_read)
                } else {
                    None
                },
                cache_creation_tokens: if cache_creation > 0 {
                    Some(cache_creation)
                } else {
                    None
                },
            })
        });

        let external_id = entry
            .get("uuid")
            .and_then(|u| u.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("claude-{}", index));

        let parent_id = entry
            .get("parentUuid")
            .and_then(|u| u.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let model = message
            .get("model")
            .and_then(|m| m.as_str())
            .map(|s| s.to_string());

        Some(NormalizedMessage {
            external_id,
            parent_id,
            role,
            created_at: timestamp,
            model,
            blocks,
            usage,
            metadata: HashMap::new(),
        })
    }

    fn parse_content(
        content: &serde_json::Value,
        agent_map: &HashMap<String, String>,
    ) -> Vec<ContentBlock> {
        match content {
            serde_json::Value::String(text) => {
                if text.is_empty() {
                    vec![]
                } else {
                    vec![ContentBlock::Text { text: text.clone() }]
                }
            }
            serde_json::Value::Array(blocks) => blocks
                .iter()
                .filter_map(|b| Self::parse_block(b, agent_map))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn parse_block(
        block: &serde_json::Value,
        agent_map: &HashMap<String, String>,
    ) -> Option<ContentBlock> {
        let block_type = block.get("type")?.as_str()?;
        match block_type {
            "text" => {
                let text = block.get("text")?.as_str()?.to_string();
                if text.is_empty() {
                    return None;
                }
                Some(ContentBlock::Text { text })
            }
            "tool_use" => {
                let name = block
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let id = block
                    .get("id")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string());
                let input = block
                    .get("input")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                // Check for agent_id (Claude subagent)
                let _agent_id = id.as_deref().and_then(|id| agent_map.get(id)).cloned();

                Some(ContentBlock::ToolCall { id, name, input })
            }
            "tool_result" => {
                let id = block
                    .get("tool_use_id")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string());
                let is_error = block
                    .get("is_error")
                    .and_then(|e| e.as_bool())
                    .unwrap_or(false);
                let content = match block.get("content") {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(serde_json::Value::Array(arr)) => arr
                        .iter()
                        .filter_map(|item| {
                            if item.get("type")?.as_str()? == "text" {
                                item.get("text")?.as_str().map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };
                Some(ContentBlock::ToolResult {
                    id,
                    content,
                    is_error,
                })
            }
            "thinking" => {
                let text = block.get("thinking")?.as_str()?.to_string();
                if text.is_empty() {
                    return None;
                }
                Some(ContentBlock::Thinking { text })
            }
            "image" => {
                let source = block
                    .get("source")
                    .and_then(|s| s.get("data"))
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();
                let media_type = block
                    .get("source")
                    .and_then(|s| s.get("media_type"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string());
                Some(ContentBlock::Image { source, media_type })
            }
            _ => {
                // Unknown block type - preserve as Unknown
                Some(ContentBlock::Unknown { raw: block.clone() })
            }
        }
    }
}

#[async_trait]
impl super::SessionProvider for ClaudeProvider {
    fn source(&self) -> SourceKind {
        SourceKind::ClaudeCode
    }

    fn parser_version(&self) -> &'static str {
        PARSER_VERSION
    }

    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        let files = self.scan_jsonl_files();
        let mut summaries = Vec::new();

        for (session_id, file_path, project_dir_name) in files {
            // Single session failure must not fail the whole scan
            let content = match fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!(path = %file_path.display(), error = %e, "Cannot read Claude session file");
                    continue;
                }
            };

            let (title, count, cwd, started_at) = match Self::scan_summary(&content) {
                Ok(summary) => summary,
                Err(error) => {
                    tracing::warn!(
                        path = %file_path.display(),
                        error = %error,
                        "Ignoring malformed Claude session during discovery"
                    );
                    continue;
                }
            };
            let project_path = cwd.unwrap_or_else(|| Self::dir_name_to_path(&project_dir_name));

            let mtime = fs::metadata(&file_path)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from);

            summaries.push(SessionSummary {
                source: SourceKind::ClaudeCode,
                external_session_id: session_id,
                title,
                project_name: None,
                project_path: Some(project_path),
                source_path: Some(file_path),
                started_at,
                updated_at: mtime,
                message_count: count,
            });
        }

        Ok(summaries)
    }

    async fn load_session(&self, summary: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        let file_path = summary
            .source_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No source_path for Claude session"))?;

        let messages = Self::parse_jsonl(file_path)?;

        // Compute aggregate usage
        let mut usage = SessionUsage::default();
        let mut has_usage = false;
        for msg in &messages {
            if let Some(u) = &msg.usage {
                usage.add_message_usage(u);
                has_usage = true;
            }
        }

        let started_at = messages.first().and_then(|m| m.created_at);
        let updated_at = messages.last().and_then(|m| m.created_at);

        Ok(NormalizedSession {
            source: SourceKind::ClaudeCode,
            external_session_id: summary.external_session_id.clone(),
            title: summary.title.clone(),
            project_name: summary.project_name.clone(),
            project_path: summary.project_path.clone(),
            source_path: summary.source_path.clone(),
            started_at: started_at.or(summary.started_at),
            updated_at: updated_at.or(summary.updated_at),
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
                message: format!("~/.claude not found at {}", self.base_dir.display()),
            };
        }
        let projects_dir = self.projects_dir();
        if !projects_dir.exists() {
            return ProviderHealth::NotFound {
                message: format!("~/.claude/projects not found"),
            };
        }
        ProviderHealth::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn dir_name_to_path_unix() {
        assert_eq!(
            ClaudeProvider::dir_name_to_path("-home-alice-project"),
            "/home/alice/project"
        );
    }

    #[test]
    fn dir_name_to_path_windows() {
        // Windows: -C-Users-alice-project → C:/Users/alice/project
        let result = ClaudeProvider::dir_name_to_path("-C-Users-alice-project");
        assert_eq!(result, "C:/Users/alice/project");
    }

    #[test]
    fn parse_jsonl_basic() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("session.jsonl");
        let content = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Hello"}]},"uuid":"msg-1","sessionId":"sess-1","cwd":"/project","timestamp":"2024-01-01T00:00:00Z","parentUuid":null}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hi there!"}],"model":"claude-opus-4-5","usage":{"input_tokens":10,"output_tokens":5}},"uuid":"msg-2","sessionId":"sess-1","cwd":"/project","timestamp":"2024-01-01T00:00:01Z","parentUuid":"msg-1"}
"#;
        std::fs::write(&file, content).unwrap();
        let messages = ClaudeProvider::parse_jsonl(&file).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[1].role, MessageRole::Assistant);
        assert!(matches!(
            &messages[0].blocks[0],
            ContentBlock::Text { text } if text == "Hello"
        ));
        assert_eq!(messages[1].model.as_deref(), Some("claude-opus-4-5"));
    }

    #[test]
    fn parse_tool_call_and_result() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("tools.jsonl");
        let content = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"bash","input":{"command":"ls"}}]},"uuid":"msg-1","sessionId":"s","timestamp":"2024-01-01T00:00:00Z","parentUuid":null}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":"file1.txt\nfile2.txt"}]},"uuid":"msg-2","sessionId":"s","timestamp":"2024-01-01T00:00:01Z","parentUuid":"msg-1"}
"#;
        std::fs::write(&file, content).unwrap();
        let messages = ClaudeProvider::parse_jsonl(&file).unwrap();
        assert_eq!(messages.len(), 2);
        assert!(matches!(
            &messages[0].blocks[0],
            ContentBlock::ToolCall { name, .. } if name == "bash"
        ));
        assert!(matches!(
            &messages[1].blocks[0],
            ContentBlock::ToolResult { content, .. } if content.contains("file1.txt")
        ));
    }

    #[test]
    fn active_tail_is_visible_to_discovery_but_rejected_on_load() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("session.jsonl");
        let content = concat!(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"question\"},\"uuid\":\"u1\"}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":\"answer\"},\"uuid\":\"a1\"}\n",
            "{\"type\":\"assistant\",\"message\":"
        );
        std::fs::write(&file, content).unwrap();
        let summary = ClaudeProvider::scan_summary(content).unwrap();
        assert_eq!(summary.1, 2);
        assert!(ClaudeProvider::parse_jsonl(&file).is_err());
    }

    #[test]
    fn scan_summary_extracts_title() {
        let content = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"How do I implement a binary search?"}]},"uuid":"m1","sessionId":"s","cwd":"/home/user/project","timestamp":"2024-01-01T00:00:00Z","parentUuid":null}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Binary search is..."}]},"uuid":"m2","sessionId":"s","cwd":"/home/user/project","timestamp":"2024-01-01T00:00:01Z","parentUuid":"m1"}
"#;
        let (title, count, cwd, started_at) = ClaudeProvider::scan_summary(content).unwrap();
        assert_eq!(
            title.as_deref(),
            Some("How do I implement a binary search?")
        );
        assert_eq!(count, 2);
        assert_eq!(cwd.as_deref(), Some("/home/user/project"));
        assert!(started_at.is_some());
    }
}
