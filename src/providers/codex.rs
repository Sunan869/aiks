/// Codex session provider.
///
/// Session storage: ~/.codex/sessions/{Y}/{M}/{D}/rollout-*.jsonl
/// Also reads from ~/.codex/state_5.sqlite for fast session listing.
///
/// JSONL format: { timestamp, type, payload }
/// type: "session_meta" | "event_msg" | "response_item" | "turn_context"
///
/// Derived from: AICoder Session Viewer (MIT)
/// Original: https://github.com/seastart/aicoder-session-viewer
/// Commit: b750098594c3bb969d5039a9f2e44a283c38af9b
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use walkdir::WalkDir;

use crate::model::{
    ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SessionUsage, SourceKind,
};
use crate::providers::{ProviderHealth, SessionSummary};

const PARSER_VERSION: &str = "codex-v1";

pub struct CodexProvider {
    base_dir: PathBuf,
}

impl CodexProvider {
    pub fn default_path() -> anyhow::Result<PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot locate home directory"))?;
        Ok(home.join(".codex"))
    }

    pub fn new(path_override: Option<PathBuf>) -> anyhow::Result<Self> {
        let base_dir = match path_override {
            Some(p) => p,
            None => Self::default_path()?,
        };
        Ok(Self { base_dir })
    }

    fn find_session_files(&self) -> Vec<PathBuf> {
        let sessions_dir = self.base_dir.join("sessions");
        if !sessions_dir.exists() {
            return Vec::new();
        }

        WalkDir::new(&sessions_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.extension().is_some_and(|ext| ext == "jsonl")
                    && path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with("rollout-"))
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

    fn date_from_path(path: &PathBuf) -> Option<DateTime<Utc>> {
        let components: Vec<&str> = path
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();

        let sessions_idx = components.iter().position(|&c| c == "sessions")?;
        if components.len() < sessions_idx + 4 {
            return None;
        }

        let year: i32 = components[sessions_idx + 1].parse().ok()?;
        let month: u32 = components[sessions_idx + 2].parse().ok()?;
        let day: u32 = components[sessions_idx + 3].parse().ok()?;

        NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|ndt| ndt.and_utc())
    }

    fn parse_timestamp(entry: &serde_json::Value) -> Option<DateTime<Utc>> {
        entry.get("timestamp").and_then(|t| {
            if let Some(s) = t.as_str() {
                DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            } else if let Some(n) = t.as_f64() {
                DateTime::from_timestamp(n as i64, 0)
            } else if let Some(n) = t.as_i64() {
                DateTime::from_timestamp(n, 0)
            } else {
                None
            }
        })
    }

    /// Quick scan of a JSONL file to extract summary metadata.
    fn scan_summary(path: &PathBuf) -> (Option<String>, usize, Option<String>) {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return (None, 0, None),
        };
        let reader = BufReader::new(file);

        let mut title: Option<String> = None;
        let mut project_path: Option<String> = None;
        let mut msg_count = 0;
        let mut user_src = UserMsgSource::Unknown;

        for line in reader.lines() {
            let Ok(line) = line else { continue };
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            let has_event = line.contains("\"event_msg\"");
            let has_response = line.contains("\"response_item\"");
            let has_meta = line.contains("\"session_meta\"");

            if !has_event && !has_response && !has_meta {
                continue;
            }

            let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };

            let entry_type = entry.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let Some(payload) = entry.get("payload") else {
                continue;
            };

            match entry_type {
                "session_meta" => {
                    if project_path.is_none() {
                        project_path = payload
                            .get("cwd")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                    }
                }
                "event_msg" => {
                    let payload_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match payload_type {
                        "user_message" => {
                            if !user_src.accept(UserMsgSource::Legacy) {
                                continue;
                            }
                            msg_count += 1;
                            if title.is_none() {
                                if let Some(msg) = payload.get("message").and_then(|m| m.as_str()) {
                                    title = Some(truncate_title(msg, 80));
                                }
                            }
                        }
                        "item_completed" => {
                            let item = payload.get("item");
                            let is_user_msg = item
                                .and_then(|i| i.get("type"))
                                .and_then(|t| t.as_str())
                                .is_some_and(|t| t == "UserMessage");
                            if !is_user_msg || !user_src.accept(UserMsgSource::ItemCompleted) {
                                continue;
                            }
                            msg_count += 1;
                            if title.is_none() {
                                let text = item.map(collect_item_text).unwrap_or_default();
                                if !text.is_empty() {
                                    title = Some(truncate_title(&text, 80));
                                }
                            }
                        }
                        "error" => {
                            msg_count += 1;
                        }
                        _ => {}
                    }
                }
                "response_item" => {
                    let payload_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    let role = payload.get("role").and_then(|r| r.as_str()).unwrap_or("");
                    if role == "assistant" || is_tool_item(payload_type) {
                        msg_count += 1;
                    }
                }
                _ => {}
            }
        }

        (title, msg_count, project_path)
    }

    /// Parse a Codex JSONL rollout file into messages.
    fn parse_session_file(
        path: &PathBuf,
    ) -> anyhow::Result<(Vec<NormalizedMessage>, Option<String>, Option<String>)> {
        let content = fs::read_to_string(path)?;
        let mut messages = Vec::new();
        let mut title: Option<String> = None;
        let mut project_path: Option<String> = None;
        let mut msg_index = 0usize;
        let mut pending_user_images: Vec<ContentBlock> = Vec::new();
        let mut user_src = UserMsgSource::Unknown;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };

            let entry_type = entry.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let timestamp = Self::parse_timestamp(&entry);
            let Some(payload) = entry.get("payload") else {
                continue;
            };

            match entry_type {
                "session_meta" => {
                    if project_path.is_none() {
                        project_path = payload
                            .get("cwd")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                    }
                }

                "event_msg" => {
                    let payload_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match payload_type {
                        "user_message" => {
                            if !user_src.accept(UserMsgSource::Legacy) {
                                continue;
                            }
                            let text = payload
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("")
                                .to_string();
                            push_user_message(
                                &mut messages,
                                &mut msg_index,
                                &mut title,
                                &mut pending_user_images,
                                text,
                                timestamp,
                            );
                        }
                        "item_completed" => {
                            let item = payload.get("item");
                            let is_user_msg = item
                                .and_then(|i| i.get("type"))
                                .and_then(|t| t.as_str())
                                .is_some_and(|t| t == "UserMessage");
                            if !is_user_msg || !user_src.accept(UserMsgSource::ItemCompleted) {
                                continue;
                            }
                            let text = item.map(collect_item_text).unwrap_or_default();
                            push_user_message(
                                &mut messages,
                                &mut msg_index,
                                &mut title,
                                &mut pending_user_images,
                                text,
                                timestamp,
                            );
                        }
                        "error" => {
                            let text = payload
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("Unknown error")
                                .to_string();
                            messages.push(NormalizedMessage {
                                external_id: format!("codex-{}", msg_index),
                                parent_id: None,
                                role: MessageRole::System,
                                created_at: timestamp,
                                model: None,
                                blocks: vec![ContentBlock::Text { text }],
                                usage: None,
                                metadata: HashMap::new(),
                            });
                            msg_index += 1;
                        }
                        "token_count" => {
                            // Back-fill token usage to last assistant message
                            if let Some(last_usage) = payload
                                .get("info")
                                .and_then(|i| i.get("last_token_usage"))
                            {
                                let usage = crate::model::MessageUsage {
                                    input_tokens: last_usage
                                        .get("input_tokens")
                                        .and_then(|v| v.as_u64()),
                                    output_tokens: last_usage
                                        .get("output_tokens")
                                        .and_then(|v| v.as_u64()),
                                    cache_read_tokens: last_usage
                                        .get("cached_input_tokens")
                                        .and_then(|v| v.as_u64()),
                                    cache_creation_tokens: None,
                                };
                                if let Some(last_msg) = messages
                                    .iter_mut()
                                    .rev()
                                    .find(|m| m.role == MessageRole::Assistant && m.usage.is_none())
                                {
                                    last_msg.usage = Some(usage);
                                }
                            }
                        }
                        _ => {}
                    }
                }

                "response_item" => {
                    let payload_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");

                    match payload_type {
                        "message" => {
                            let role_str =
                                payload.get("role").and_then(|r| r.as_str()).unwrap_or("");
                            if role_str == "assistant" {
                                let blocks = parse_assistant_content_blocks(payload);
                                if !blocks.is_empty() {
                                    messages.push(NormalizedMessage {
                                        external_id: format!("codex-{}", msg_index),
                                        parent_id: None,
                                        role: MessageRole::Assistant,
                                        created_at: timestamp,
                                        model: None,
                                        blocks,
                                        usage: None,
                                        metadata: HashMap::new(),
                                    });
                                    msg_index += 1;
                                }
                            } else if role_str == "user" {
                                pending_user_images
                                    .extend(parse_user_image_blocks(payload));
                            }
                        }

                        "function_call" => {
                            let name = payload
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let input = payload
                                .get("arguments")
                                .and_then(|a| {
                                    if let Some(s) = a.as_str() {
                                        serde_json::from_str(s).ok()
                                    } else {
                                        Some(a.clone())
                                    }
                                })
                                .unwrap_or(serde_json::Value::Null);
                            let id = payload
                                .get("call_id")
                                .and_then(|i| i.as_str())
                                .map(|s| s.to_string());

                            messages.push(NormalizedMessage {
                                external_id: format!("codex-{}", msg_index),
                                parent_id: None,
                                role: MessageRole::Assistant,
                                created_at: timestamp,
                                model: None,
                                blocks: vec![ContentBlock::ToolCall { id, name, input }],
                                usage: None,
                                metadata: HashMap::new(),
                            });
                            msg_index += 1;
                        }

                        "custom_tool_call" => {
                            let name = payload
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let input = payload
                                .get("input")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null);
                            let id = payload
                                .get("call_id")
                                .and_then(|i| i.as_str())
                                .map(|s| s.to_string());

                            messages.push(NormalizedMessage {
                                external_id: format!("codex-{}", msg_index),
                                parent_id: None,
                                role: MessageRole::Assistant,
                                created_at: timestamp,
                                model: None,
                                blocks: vec![ContentBlock::ToolCall { id, name, input }],
                                usage: None,
                                metadata: HashMap::new(),
                            });
                            msg_index += 1;
                        }

                        "web_search_call" => {
                            let input = payload
                                .get("action")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null);
                            let id = payload
                                .get("id")
                                .and_then(|i| i.as_str())
                                .map(|s| s.to_string());

                            messages.push(NormalizedMessage {
                                external_id: format!("codex-{}", msg_index),
                                parent_id: None,
                                role: MessageRole::Assistant,
                                created_at: timestamp,
                                model: None,
                                blocks: vec![ContentBlock::ToolCall {
                                    id,
                                    name: "web_search".to_string(),
                                    input,
                                }],
                                usage: None,
                                metadata: HashMap::new(),
                            });
                            msg_index += 1;
                        }

                        "function_call_output" | "custom_tool_call_output" => {
                            let output = extract_tool_output(payload);
                            let id = payload
                                .get("call_id")
                                .and_then(|i| i.as_str())
                                .map(|s| s.to_string());

                            messages.push(NormalizedMessage {
                                external_id: format!("codex-{}", msg_index),
                                parent_id: None,
                                role: MessageRole::Tool,
                                created_at: timestamp,
                                model: None,
                                blocks: vec![ContentBlock::ToolResult {
                                    id,
                                    content: output,
                                    is_error: false,
                                }],
                                usage: None,
                                metadata: HashMap::new(),
                            });
                            msg_index += 1;
                        }

                        _ => {}
                    }
                }
                _ => {}
            }
        }

        Ok((messages, title, project_path))
    }

    /// Try to list sessions from the state SQLite (fast path).
    fn list_from_db(&self) -> anyhow::Result<Vec<SessionSummary>> {
        let db_path = self.base_dir.join("state_5.sqlite");
        if !db_path.exists() {
            anyhow::bail!("Codex state DB not found");
        }

        let conn = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, title, cwd, created_at, updated_at, tokens_used, rollout_path
             FROM threads WHERE archived = 0 ORDER BY updated_at DESC",
        )?;

        let mut summaries = Vec::new();
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let rollout_path: String = row.get(6)?;
            let session_id = std::path::Path::new(&rollout_path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(id);
            Ok((
                session_id,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;

        for row in rows.flatten() {
            let (session_id, title, cwd, created_at, updated_at, tokens_used) = row;
            summaries.push(SessionSummary {
                source: SourceKind::Codex,
                external_session_id: session_id,
                title: Some(truncate_title(&title, 80)),
                project_name: None,
                project_path: Some(cwd),
                source_path: None,
                started_at: DateTime::from_timestamp(created_at, 0),
                updated_at: DateTime::from_timestamp(updated_at, 0),
                message_count: 0,
            });
        }

        Ok(summaries)
    }
}

#[async_trait]
impl super::SessionProvider for CodexProvider {
    fn source(&self) -> SourceKind {
        SourceKind::Codex
    }

    fn parser_version(&self) -> &'static str {
        PARSER_VERSION
    }

    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        // Try fast DB path first
        if let Ok(summaries) = self.list_from_db() {
            return Ok(summaries);
        }

        // Fallback: scan JSONL files
        let files = self.find_session_files();
        let mut summaries = Vec::new();

        for path in &files {
            let session_id = Self::session_id_from_path(path);
            let date = Self::date_from_path(path);
            let (title, msg_count, project_path) = Self::scan_summary(path);

            let mtime = fs::metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from);

            summaries.push(SessionSummary {
                source: SourceKind::Codex,
                external_session_id: session_id,
                title,
                project_name: None,
                project_path,
                source_path: Some(path.clone()),
                started_at: date.or(mtime),
                updated_at: mtime,
                message_count: msg_count,
            });
        }

        // Sort by updated_at desc
        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }

    async fn load_session(&self, summary: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        // Find the rollout file for this session
        let files = self.find_session_files();
        let path = files
            .iter()
            .find(|p| Self::session_id_from_path(p) == summary.external_session_id)
            .cloned()
            .or_else(|| summary.source_path.clone())
            .ok_or_else(|| {
                anyhow::anyhow!("Codex session file not found: {}", summary.external_session_id)
            })?;

        let (messages, title, project_path) = Self::parse_session_file(&path)?;

        let mut usage = SessionUsage::default();
        let mut has_usage = false;
        for msg in &messages {
            if let Some(u) = &msg.usage {
                usage.add_message_usage(u);
                has_usage = true;
            }
        }

        let started_at = Self::date_from_path(&path).or_else(|| {
            fs::metadata(&path)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from)
        });
        let updated_at = messages.last().and_then(|m| m.created_at);

        Ok(NormalizedSession {
            source: SourceKind::Codex,
            external_session_id: summary.external_session_id.clone(),
            title: title.or_else(|| summary.title.clone()),
            project_name: None,
            project_path: project_path.or_else(|| summary.project_path.clone()),
            source_path: Some(path),
            started_at,
            updated_at,
            model: None,
            messages,
            usage: if has_usage { Some(usage) } else { None },
            metadata: HashMap::new(),
        })
    }

    async fn health_check(&self) -> ProviderHealth {
        if !self.base_dir.exists() {
            return ProviderHealth::NotFound {
                message: format!("~/.codex not found at {}", self.base_dir.display()),
            };
        }
        ProviderHealth::Ok
    }
}

// ===== Helpers (adapted from AICoder Session Viewer) =====

#[derive(PartialEq, Clone, Copy)]
enum UserMsgSource {
    Unknown,
    Legacy,
    ItemCompleted,
}

impl UserMsgSource {
    fn accept(&mut self, candidate: UserMsgSource) -> bool {
        if *self == UserMsgSource::Unknown {
            *self = candidate;
        }
        *self == candidate
    }
}

fn is_tool_item(payload_type: &str) -> bool {
    matches!(
        payload_type,
        "function_call"
            | "function_call_output"
            | "custom_tool_call"
            | "custom_tool_call_output"
            | "web_search_call"
    )
}

fn collect_item_text(item: &serde_json::Value) -> String {
    let Some(arr) = item.get("content").and_then(|c| c.as_array()) else {
        return String::new();
    };
    arr.iter()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate_title(s: &str, max_chars: usize) -> String {
    let first_line = s.lines().next().unwrap_or(s).trim();
    if first_line.chars().count() <= max_chars {
        first_line.to_string()
    } else {
        let truncated: String = first_line.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}

fn parse_assistant_content_blocks(payload: &serde_json::Value) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();
    if let Some(content_arr) = payload.get("content").and_then(|c| c.as_array()) {
        for block in content_arr {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match block_type {
                "output_text" | "text" => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        if !text.is_empty() {
                            blocks.push(ContentBlock::Text {
                                text: text.to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    blocks
}

fn parse_user_image_blocks(payload: &serde_json::Value) -> Vec<ContentBlock> {
    let mut images = Vec::new();
    let Some(content_arr) = payload.get("content").and_then(|c| c.as_array()) else {
        return images;
    };
    for block in content_arr {
        let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if block_type != "input_image" {
            continue;
        }
        let Some(url) = block.get("image_url").and_then(|u| u.as_str()) else {
            continue;
        };
        let (source, media_type) = if let Some(rest) = url.strip_prefix("data:") {
            if let Some((meta, data)) = rest.split_once(',') {
                let mt = meta.split(';').next().map(|s| s.to_string());
                (data.to_string(), mt)
            } else {
                (url.to_string(), None)
            }
        } else {
            (url.to_string(), None)
        };
        images.push(ContentBlock::Image { source, media_type });
    }
    images
}

fn extract_tool_output(payload: &serde_json::Value) -> String {
    let Some(output) = payload.get("output") else {
        return String::new();
    };
    if let Some(s) = output.as_str() {
        return s.to_string();
    }
    if let Some(arr) = output.as_array() {
        let parts: Vec<&str> = arr
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect();
        if !parts.is_empty() {
            return parts.concat();
        }
    }
    serde_json::to_string_pretty(output).unwrap_or_default()
}

fn push_user_message(
    messages: &mut Vec<NormalizedMessage>,
    msg_index: &mut usize,
    title: &mut Option<String>,
    pending_images: &mut Vec<ContentBlock>,
    text: String,
    timestamp: Option<DateTime<Utc>>,
) {
    if title.is_none() && !text.is_empty() {
        *title = Some(truncate_title(&text, 80));
    }
    let has_text = !text.is_empty();
    let has_images = !pending_images.is_empty();
    if !has_text && !has_images {
        return;
    }
    let mut blocks = Vec::new();
    if has_text {
        blocks.push(ContentBlock::Text { text });
    }
    blocks.append(pending_images);

    messages.push(NormalizedMessage {
        external_id: format!("codex-{}", msg_index),
        parent_id: None,
        role: MessageRole::User,
        created_at: timestamp,
        model: None,
        blocks,
        usage: None,
        metadata: HashMap::new(),
    });
    *msg_index += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn parse_fixture(name: &str, jsonl: &str) -> (Vec<NormalizedMessage>, Option<String>) {
        let dir = tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, jsonl).unwrap();
        let (messages, title, _) = CodexProvider::parse_session_file(&path).unwrap();
        (messages, title)
    }

    #[test]
    fn parses_new_format_item_completed() {
        let jsonl = r###"
{"timestamp":"2026-08-18T02:44:22.916Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md\nignored"}]}}
{"timestamp":"2026-08-18T02:44:22.948Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","content":[{"type":"text","text":"How do I fix this bug?"}]}}}
{"timestamp":"2026-08-18T02:44:27.671Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Let me help you"}]}}
{"timestamp":"2026-08-18T02:44:29.871Z","type":"response_item","payload":{"type":"custom_tool_call","call_id":"call_1","name":"exec","input":"const r = await tools.exec_command({cmd:\"ls\"});"}}
{"timestamp":"2026-08-18T02:44:30.560Z","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"call_1","output":[{"type":"input_text","text":"Script done\n"},{"type":"input_text","text":"a.txt\n"}]}}
"###;
        let (messages, title) = parse_fixture("codex-new.jsonl", jsonl);
        let users: Vec<_> = messages.iter().filter(|m| m.role == MessageRole::User).collect();
        assert_eq!(users.len(), 1);
        assert!(matches!(&users[0].blocks[0], ContentBlock::Text { text } if text.contains("fix this bug")));
        assert_eq!(title.as_deref(), Some("How do I fix this bug?"));

        let tool_result = messages.iter().flat_map(|m| &m.blocks).find_map(|b| match b {
            ContentBlock::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        });
        assert_eq!(tool_result.as_deref(), Some("Script done\na.txt\n"));
    }

    #[test]
    fn parses_legacy_user_message() {
        let jsonl = r#"
{"timestamp":"2026-08-01T00:42:49.000Z","type":"event_msg","payload":{"type":"user_message","message":"Old format question"}}
{"timestamp":"2026-08-01T00:42:50.000Z","type":"response_item","payload":{"type":"function_call","call_id":"call_2","name":"shell","arguments":"{\"cmd\":\"ls\"}"}}
{"timestamp":"2026-08-01T00:42:51.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_2","output":"a.txt\n"}}
"#;
        let (messages, title) = parse_fixture("codex-legacy.jsonl", jsonl);
        let users: Vec<_> = messages.iter().filter(|m| m.role == MessageRole::User).collect();
        assert_eq!(users.len(), 1);
        assert_eq!(title.as_deref(), Some("Old format question"));

        let result = messages.iter().flat_map(|m| &m.blocks).find_map(|b| match b {
            ContentBlock::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        });
        assert_eq!(result.as_deref(), Some("a.txt\n"));
    }
}
