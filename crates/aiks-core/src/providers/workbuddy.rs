//! WorkBuddy session provider.
//!
//! WorkBuddy keeps session metadata in `workbuddy.db` and transcript events
//! under `projects/**/*.jsonl`. This provider is strictly read-only with
//! respect to the WorkBuddy data root.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use walkdir::WalkDir;

use crate::model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind};
use crate::providers::{ProviderHealth, SessionProvider, SessionSummary};

const PARSER_VERSION: &str = "workbuddy-jsonl-v2";

const REQUIRED_SESSION_COLUMNS: &[&str] = &[
    "id",
    "cwd",
    "title",
    "custom_title",
    "created_at",
    "updated_at",
    "deleted_at",
];

pub struct WorkBuddyProvider {
    root: PathBuf,
    db_path: PathBuf,
}

impl WorkBuddyProvider {
    pub fn new(path_override: Option<PathBuf>) -> anyhow::Result<Self> {
        let root = match path_override {
            Some(path) => path,
            None => Self::default_root()?,
        };
        let db_path = root.join("workbuddy.db");
        Ok(Self { root, db_path })
    }

    fn default_root() -> anyhow::Result<PathBuf> {
        if let Ok(path) = std::env::var("WORKBUDDY_CONFIG_DIR") {
            let path = path.trim();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }

        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot locate home directory"))?;
        let home_root = home.join(".workbuddy");
        if home_root.join("workbuddy.db").is_file() {
            return Ok(home_root);
        }

        #[cfg(windows)]
        {
            if let Some(root) = Self::find_windows_root() {
                return Ok(root);
            }
        }

        // Keep construction side-effect free even when WorkBuddy is absent;
        // health_check reports NotFound with the concrete default location.
        Ok(home_root)
    }

    #[cfg(windows)]
    fn find_windows_root() -> Option<PathBuf> {
        if let Ok(program_data) = std::env::var("ProgramData") {
            let users_root = PathBuf::from(program_data).join("WorkBuddy").join("users");
            if let Some(root) = Self::find_nested_workbuddy_root(&users_root) {
                return Some(root);
            }
        }

        if let Ok(system_drive) = std::env::var("SystemDrive") {
            let env_root = PathBuf::from(system_drive).join("WorkBuddy-env");
            if let Some(root) = Self::find_nested_workbuddy_root(&env_root) {
                return Some(root);
            }
        }

        None
    }

    #[cfg(windows)]
    fn find_nested_workbuddy_root(parent: &std::path::Path) -> Option<PathBuf> {
        let entries = std::fs::read_dir(parent).ok()?;
        for entry in entries.flatten() {
            let candidate = entry.path().join(".workbuddy");
            if candidate.join("workbuddy.db").is_file() {
                return Some(candidate);
            }
        }
        None
    }

    fn open_readonly(&self) -> anyhow::Result<Connection> {
        let conn = Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(conn)
    }

    fn session_columns(conn: &Connection) -> anyhow::Result<HashSet<String>> {
        let mut stmt = conn.prepare("PRAGMA table_info(sessions)")?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(columns)
    }

    fn validate_schema(columns: &HashSet<String>) -> anyhow::Result<()> {
        let missing: Vec<&str> = REQUIRED_SESSION_COLUMNS
            .iter()
            .copied()
            .filter(|column| !columns.contains(*column))
            .collect();
        if !missing.is_empty() {
            anyhow::bail!(
                "WorkBuddy sessions table is missing required columns: {}",
                missing.join(", ")
            );
        }
        Ok(())
    }

    fn optional_expr(columns: &HashSet<String>, name: &str) -> String {
        if columns.contains(name) {
            name.to_string()
        } else {
            format!("NULL AS {name}")
        }
    }

    fn millis_to_datetime(ms: i64) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp_millis(ms)
    }

    fn latest_update(updated_at: i64, last_activity_at: Option<i64>) -> Option<DateTime<Utc>> {
        Self::millis_to_datetime(updated_at.max(last_activity_at.unwrap_or(updated_at)))
    }

    fn project_name_from_cwd(cwd: &str) -> Option<String> {
        let trimmed = cwd.trim_end_matches(['/', '\\']);
        trimmed
            .rsplit(['/', '\\'])
            .find(|segment| !segment.is_empty())
            .map(str::to_owned)
    }

    fn transcript_index(&self) -> HashMap<String, PathBuf> {
        let projects_dir = self.root.join("projects");
        let Ok(metadata) = std::fs::symlink_metadata(&projects_dir) else {
            return HashMap::new();
        };
        if !metadata.is_dir() {
            return HashMap::new();
        }

        let mut index = HashMap::new();
        for entry in WalkDir::new(projects_dir)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file()
                || entry.path().extension().and_then(|value| value.to_str()) != Some("jsonl")
            {
                continue;
            }

            let Ok(path) = self.checked_transcript_path(entry.path()) else {
                continue;
            };
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let content = String::from_utf8_lossy(&bytes);
            for line in content.lines() {
                let Ok(event) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                if let Some(session_id) = Self::event_session_id(&event) {
                    index
                        .entry(session_id.to_string())
                        .or_insert_with(|| entry.path().to_path_buf());
                }
            }
        }

        index
    }

    fn checked_transcript_path(&self, path: &std::path::Path) -> anyhow::Result<PathBuf> {
        let projects_dir = self.root.join("projects");
        if !std::fs::symlink_metadata(&projects_dir)?.is_dir() {
            anyhow::bail!("WorkBuddy projects root must be a real directory");
        }

        let projects_dir = projects_dir.canonicalize()?;
        let path = path.canonicalize()?;
        if !path.starts_with(&projects_dir)
            || path.extension().and_then(|value| value.to_str()) != Some("jsonl")
            || !path.is_file()
        {
            anyhow::bail!("WorkBuddy transcript must be a JSONL file inside projects");
        }
        Ok(path)
    }

    fn event_session_id(event: &Value) -> Option<&str> {
        event
            .get("sessionId")
            .and_then(Value::as_str)
            .or_else(|| event.get("session_id").and_then(Value::as_str))
    }

    fn event_external_id(event: &Value, index: usize) -> String {
        event
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("workbuddy-{index}"))
    }

    fn event_parent_id(event: &Value) -> Option<String> {
        event
            .get("parentId")
            .or_else(|| event.get("parent_id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    fn event_timestamp(event: &Value) -> Option<DateTime<Utc>> {
        event
            .get("timestamp")
            .and_then(Value::as_i64)
            .and_then(Self::millis_to_datetime)
    }

    fn event_model(event: &Value) -> Option<String> {
        event
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    fn extract_user_query(text: &str) -> String {
        const OPEN: &str = "<user_query>";
        const CLOSE: &str = "</user_query>";

        if let Some(start) = text.find(OPEN) {
            let body_start = start + OPEN.len();
            if let Some(end) = text[body_start..].find(CLOSE) {
                return text[body_start..body_start + end].trim().to_owned();
            }
        }

        text.trim().to_owned()
    }

    fn text_block(text: &str, role: MessageRole) -> ContentBlock {
        let text = if role == MessageRole::User {
            Self::extract_user_query(text)
        } else {
            text.trim().to_owned()
        };
        ContentBlock::Text { text }
    }

    fn message_blocks(event: &Value, role: MessageRole) -> Vec<ContentBlock> {
        let Some(content) = event.get("content") else {
            return vec![ContentBlock::Unknown { raw: event.clone() }];
        };

        match content {
            Value::String(text) => vec![Self::text_block(text, role)],
            Value::Array(items) => {
                let mut blocks = Vec::new();
                for item in items {
                    let kind = item.get("type").and_then(Value::as_str);
                    let text = item.get("text").and_then(Value::as_str);
                    if matches!(kind, Some("text" | "input_text" | "output_text")) {
                        if let Some(text) = text {
                            blocks.push(Self::text_block(text, role));
                            continue;
                        }
                    }
                    blocks.push(ContentBlock::Unknown { raw: item.clone() });
                }

                if blocks.is_empty() {
                    blocks.push(ContentBlock::Unknown { raw: event.clone() });
                }
                blocks
            }
            _ => vec![ContentBlock::Unknown {
                raw: content.clone(),
            }],
        }
    }

    fn event_metadata(event_type: &str) -> HashMap<String, Value> {
        let mut metadata = HashMap::new();
        metadata.insert(
            "event_type".to_string(),
            Value::String(event_type.to_string()),
        );
        metadata
    }

    fn parse_event(event: &Value, index: usize) -> Option<NormalizedMessage> {
        let event_type = event.get("type").and_then(Value::as_str)?;
        if event_type == "ai-title" {
            return None;
        }

        let external_id = Self::event_external_id(event, index);
        let parent_id = Self::event_parent_id(event);
        let created_at = Self::event_timestamp(event);
        let model = Self::event_model(event);
        let metadata = Self::event_metadata(event_type);

        let (role, blocks) = match event_type {
            "message" => {
                let role = event
                    .get("role")
                    .and_then(Value::as_str)
                    .map(MessageRole::from_str)
                    .unwrap_or(MessageRole::Unknown);
                let blocks = Self::message_blocks(event, role);
                (role, blocks)
            }
            "reasoning" => {
                let text = event
                    .get("text")
                    .and_then(Value::as_str)
                    .or_else(|| event.get("content").and_then(Value::as_str));
                let block = match text {
                    Some(text) => ContentBlock::Thinking {
                        text: text.trim().to_owned(),
                    },
                    None => ContentBlock::Unknown { raw: event.clone() },
                };
                (MessageRole::Assistant, vec![block])
            }
            "function_call" => {
                let name = event
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_owned();
                let input = event
                    .get("arguments")
                    .or_else(|| event.get("input"))
                    .cloned()
                    .unwrap_or(Value::Null);
                (
                    MessageRole::Assistant,
                    vec![ContentBlock::ToolCall {
                        id: event.get("id").and_then(Value::as_str).map(str::to_owned),
                        name,
                        input,
                    }],
                )
            }
            "function_call_result" => {
                let content = match event.get("content") {
                    Some(Value::String(value)) => value.clone(),
                    Some(value) => value.to_string(),
                    None => String::new(),
                };
                let is_error = event
                    .get("is_error")
                    .or_else(|| event.get("isError"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                (
                    MessageRole::Tool,
                    vec![ContentBlock::ToolResult {
                        id: event.get("id").and_then(Value::as_str).map(str::to_owned),
                        content,
                        is_error,
                    }],
                )
            }
            _ => (
                MessageRole::Unknown,
                vec![ContentBlock::Unknown { raw: event.clone() }],
            ),
        };

        Some(NormalizedMessage {
            external_id,
            parent_id,
            role,
            created_at,
            model,
            blocks,
            usage: None,
            metadata,
        })
    }

    fn parse_transcript(
        &self,
        path: &std::path::Path,
        expected_session_id: &str,
    ) -> anyhow::Result<(Vec<NormalizedMessage>, usize)> {
        let path = self.checked_transcript_path(path)?;
        let bytes = std::fs::read(path)?;
        let content = String::from_utf8_lossy(&bytes);
        let mut messages = Vec::new();
        let mut malformed_lines = 0;
        let mut matched_events = 0;

        for (index, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(event) = serde_json::from_str::<Value>(line) else {
                malformed_lines += 1;
                continue;
            };
            if Self::event_session_id(&event) != Some(expected_session_id) {
                continue;
            }
            matched_events += 1;
            if let Some(message) = Self::parse_event(&event, index) {
                messages.push(message);
            }
        }

        if matched_events == 0 {
            anyhow::bail!("WorkBuddy transcript has no events for the requested session");
        }
        if malformed_lines > 0 {
            tracing::warn!(malformed_lines, "Skipped malformed WorkBuddy transcript lines");
        }
        Ok((messages, malformed_lines))
    }

    fn not_found_message(&self) -> String {
        format!("WorkBuddy database not found at {}", self.db_path.display())
    }
}

#[async_trait]
impl SessionProvider for WorkBuddyProvider {
    fn source(&self) -> SourceKind {
        SourceKind::WorkBuddy
    }

    fn parser_version(&self) -> &'static str {
        PARSER_VERSION
    }

    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        let conn = self.open_readonly()?;
        let columns = Self::session_columns(&conn)?;
        Self::validate_schema(&columns)?;

        let status_expr = Self::optional_expr(&columns, "status");
        let mode_expr = Self::optional_expr(&columns, "mode");
        let last_activity_expr = Self::optional_expr(&columns, "last_activity_at");
        let permission_mode_expr = Self::optional_expr(&columns, "permission_mode");
        let is_playground_expr = Self::optional_expr(&columns, "is_playground");
        let model_expr = Self::optional_expr(&columns, "model");
        let updated_expr = if columns.contains("last_activity_at") {
            "MAX(updated_at, COALESCE(last_activity_at, updated_at))"
        } else {
            "updated_at"
        };

        let sql = format!(
            "SELECT id, cwd, title, custom_title, created_at, updated_at, \
                    {status_expr}, {mode_expr}, {last_activity_expr}, \
                    {permission_mode_expr}, {is_playground_expr}, {model_expr} \
             FROM sessions \
             WHERE deleted_at IS NULL \
             ORDER BY {updated_expr} DESC"
        );

        let transcript_paths = self.transcript_index();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let cwd: String = row.get(1)?;
            let title: Option<String> = row.get(2)?;
            let custom_title: Option<String> = row.get(3)?;
            let created_at: i64 = row.get(4)?;
            let updated_at: i64 = row.get(5)?;
            let _status: Option<String> = row.get(6)?;
            let _mode: Option<String> = row.get(7)?;
            let last_activity_at: Option<i64> = row.get(8)?;
            let _permission_mode: Option<String> = row.get(9)?;
            let _is_playground: Option<i64> = row.get(10)?;
            let _model: Option<String> = row.get(11)?;

            let preferred_title = custom_title
                .filter(|value| !value.trim().is_empty())
                .or_else(|| title.filter(|value| !value.trim().is_empty()));
            let source_path = transcript_paths.get(&id).cloned();

            Ok(SessionSummary {
                source: SourceKind::WorkBuddy,
                external_session_id: id,
                title: preferred_title,
                project_name: Self::project_name_from_cwd(&cwd),
                project_path: if cwd.trim().is_empty() {
                    None
                } else {
                    Some(cwd)
                },
                source_path,
                started_at: Self::millis_to_datetime(created_at),
                updated_at: Self::latest_update(updated_at, last_activity_at),
                message_count: 0,
            })
        })?;

        Ok(rows.filter_map(Result::ok).collect())
    }

    async fn load_session(&self, summary: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        let conn = self.open_readonly()?;
        let columns = Self::session_columns(&conn)?;
        Self::validate_schema(&columns)?;

        let status_expr = Self::optional_expr(&columns, "status");
        let mode_expr = Self::optional_expr(&columns, "mode");
        let last_activity_expr = Self::optional_expr(&columns, "last_activity_at");
        let permission_mode_expr = Self::optional_expr(&columns, "permission_mode");
        let is_playground_expr = Self::optional_expr(&columns, "is_playground");
        let model_expr = Self::optional_expr(&columns, "model");

        let sql = format!(
            "SELECT cwd, title, custom_title, created_at, updated_at, \
                    {status_expr}, {mode_expr}, {last_activity_expr}, \
                    {permission_mode_expr}, {is_playground_expr}, {model_expr} \
             FROM sessions \
             WHERE id = ?1 AND deleted_at IS NULL"
        );

        let (
            cwd,
            title,
            custom_title,
            created_at,
            updated_at,
            status,
            mode,
            last_activity_at,
            permission_mode,
            is_playground,
            model,
        ): (
            String,
            Option<String>,
            Option<String>,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<i64>,
            Option<String>,
        ) = conn.query_row(
            &sql,
            rusqlite::params![&summary.external_session_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                ))
            },
        )?;

        let title = custom_title
            .filter(|value| !value.trim().is_empty())
            .or_else(|| title.filter(|value| !value.trim().is_empty()));

        let source_path = summary
            .source_path
            .clone()
            .or_else(|| {
                self.transcript_index()
                    .get(&summary.external_session_id)
                    .cloned()
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "WorkBuddy transcript not found for session {}",
                    summary.external_session_id
                )
            })?;

        let (messages, malformed_lines) =
            self.parse_transcript(&source_path, &summary.external_session_id)?;

        let mut metadata = HashMap::new();
        if malformed_lines > 0 {
            metadata.insert(
                "parse_warnings".to_string(),
                serde_json::json!({ "malformed_lines": malformed_lines }),
            );
        }
        if let Some(value) = status {
            metadata.insert("status".to_string(), Value::String(value));
        }
        if let Some(value) = mode {
            metadata.insert("mode".to_string(), Value::String(value));
        }
        if let Some(value) = permission_mode {
            metadata.insert("permission_mode".to_string(), Value::String(value));
        }
        if let Some(value) = is_playground {
            metadata.insert("is_playground".to_string(), Value::Bool(value != 0));
        }
        if let Some(value) = last_activity_at {
            metadata.insert("last_activity_at".to_string(), Value::Number(value.into()));
        }

        Ok(NormalizedSession {
            source: SourceKind::WorkBuddy,
            external_session_id: summary.external_session_id.clone(),
            title,
            project_name: Self::project_name_from_cwd(&cwd),
            project_path: if cwd.trim().is_empty() {
                None
            } else {
                Some(cwd)
            },
            source_path: Some(source_path),
            started_at: Self::millis_to_datetime(created_at),
            updated_at: Self::latest_update(updated_at, last_activity_at),
            model,
            messages,
            usage: None,
            metadata,
        })
    }

    async fn health_check(&self) -> ProviderHealth {
        if !self.root.is_dir() || !self.db_path.is_file() {
            return ProviderHealth::NotFound {
                message: self.not_found_message(),
            };
        }

        let conn = match self.open_readonly() {
            Ok(conn) => conn,
            Err(error) => {
                return ProviderHealth::Error {
                    message: format!("Failed to open WorkBuddy database read-only: {error}"),
                };
            }
        };

        let columns = match Self::session_columns(&conn) {
            Ok(columns) => columns,
            Err(error) => {
                return ProviderHealth::Error {
                    message: format!("Failed to inspect WorkBuddy sessions schema: {error}"),
                };
            }
        };

        match Self::validate_schema(&columns) {
            Ok(()) => ProviderHealth::Ok,
            Err(error) => ProviderHealth::Error {
                message: error.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WorkBuddyProvider;

    #[test]
    fn project_name_handles_windows_and_posix_paths() {
        assert_eq!(
            WorkBuddyProvider::project_name_from_cwd(r"C:\src\demo"),
            Some("demo".to_string())
        );
        assert_eq!(
            WorkBuddyProvider::project_name_from_cwd("/home/user/demo/"),
            Some("demo".to_string())
        );
    }
}
