/// OpenCode session provider.
///
/// Session storage: ~/.local/share/opencode/opencode.db (SQLite, read-only)
///
/// Schema (actual, from live DB inspection):
/// - session: id, project_id, directory, title, version, time_created, time_updated, model, ...
/// - message: id, session_id, time_created, data (JSON: {role, time, modelID, providerID, ...})
/// - part: id, message_id, session_id, time_created, data (JSON: {type, ...})
///   - type="text": {type, text}
///   - type="tool": {type, callID, tool, state: {status, input, output, raw}}
///   - type="reasoning": {type, text}
///   - type="file": {type, mime, url}
///   - type="step-start"|"step-finish"|"compaction"|"patch": (skip)
/// - project: id, worktree, name, time_created, time_updated
///
/// Derived from: AICoder Session Viewer (MIT)
/// Original: https://github.com/seastart/aicoder-session-viewer  
/// Commit: b750098594c3bb969d5039a9f2e44a283c38af9b
/// (Extended with actual live DB schema analysis)
use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};

use crate::model::{
    ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SessionUsage, SourceKind,
};
use crate::providers::{ProviderHealth, SessionSummary};

const PARSER_VERSION: &str = "opencode-sqlite-v1";

pub struct OpenCodeProvider {
    db_path: PathBuf,
}

impl OpenCodeProvider {
    /// Default path on different platforms:
    /// - Linux/macOS: ~/.local/share/opencode/opencode.db
    /// - Windows: ~/.local/share/opencode/opencode.db (OpenCode uses Unix-style on Windows too)
    pub fn default_path() -> anyhow::Result<PathBuf> {
        // Try platform data dir first
        if let Some(data_dir) = dirs::data_local_dir() {
            let p = data_dir.join("opencode").join("opencode.db");
            if p.exists() {
                return Ok(p);
            }
        }

        // Try home/.local/share (Linux/macOS style, also works on Windows for OpenCode)
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot locate home directory"))?;
        let linux_path = home.join(".local").join("share").join("opencode").join("opencode.db");
        if linux_path.exists() {
            return Ok(linux_path);
        }

        // Return the Linux path even if it doesn't exist (will get caught by health_check)
        Ok(linux_path)
    }

    pub fn new(path_override: Option<PathBuf>) -> anyhow::Result<Self> {
        let db_path = match path_override {
            Some(p) => p,
            None => Self::default_path()?,
        };
        Ok(Self { db_path })
    }

    /// Open the database in read-only mode, WAL-aware.
    fn open_readonly(&self) -> anyhow::Result<Connection> {
        let conn = Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        // WAL mode: set journal_mode to WAL for consistent reads, but don't write
        // We just set a busy timeout to handle concurrent access
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(conn)
    }

    fn millis_to_datetime(ms: i64) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp_millis(ms)
    }

    /// Parse a part's data JSON into a ContentBlock.
    fn parse_part_data(data_str: &str) -> Option<ContentBlock> {
        let val: serde_json::Value = serde_json::from_str(data_str).ok()?;
        let part_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match part_type {
            "text" => {
                let text = val.get("text").and_then(|t| t.as_str()).unwrap_or("");
                if text.is_empty() {
                    return None;
                }
                Some(ContentBlock::Text { text: text.to_string() })
            }

            "tool" => {
                let name = val
                    .get("tool")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let id = val
                    .get("callID")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string());
                let state = val.get("state");
                let input = state
                    .and_then(|s| s.get("input"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let status = state.and_then(|s| s.get("status")).and_then(|s| s.as_str()).unwrap_or("pending");

                if status == "pending" || status == "running" {
                    // Tool call in progress - render as ToolCall
                    return Some(ContentBlock::ToolCall { id, name, input });
                }

                // Completed tool - render as ToolCall + ToolResult
                // We only return the tool call block here; ToolResult is handled separately
                // by looking at the output in state
                let output = state.and_then(|s| s.get("output"));
                if let Some(output) = output {
                    let content = if let Some(s) = output.as_str() {
                        s.to_string()
                    } else {
                        serde_json::to_string_pretty(output).unwrap_or_default()
                    };
                    // Return both as separate blocks (caller collects both)
                    // Actually we need to return them as a combined representation.
                    // Let's use a pair approach - just return the ToolCall with context
                    // The ToolResult will be a separate entry
                    let _ = content; // Will be used in block_pairs
                }

                Some(ContentBlock::ToolCall { id, name, input })
            }

            "reasoning" => {
                let text = val.get("text").and_then(|t| t.as_str()).unwrap_or("");
                if text.is_empty() {
                    return None;
                }
                Some(ContentBlock::Thinking { text: text.to_string() })
            }

            "file" => {
                let mime = val.get("mime").and_then(|m| m.as_str()).unwrap_or("");
                if !mime.starts_with("image/") {
                    return None;
                }
                let url = val.get("url").and_then(|u| u.as_str()).unwrap_or("");
                if url.is_empty() {
                    return None;
                }
                let (source, media_type) = if let Some(rest) = url.strip_prefix("data:") {
                    if let Some((meta, data)) = rest.split_once(',') {
                        let mt = meta.split(';').next().map(|s| s.to_string());
                        (data.to_string(), mt)
                    } else {
                        (url.to_string(), Some(mime.to_string()))
                    }
                } else {
                    (url.to_string(), Some(mime.to_string()))
                };
                Some(ContentBlock::Image { source, media_type })
            }

            // Skip these control types
            "step-start" | "step-finish" | "compaction" | "patch" => None,

            _ => {
                // Unknown type - try text fallback
                if let Some(text) = val.get("text").and_then(|t| t.as_str()) {
                    if !text.is_empty() {
                        return Some(ContentBlock::Text { text: text.to_string() });
                    }
                }
                // Preserve unknown as Unknown
                Some(ContentBlock::Unknown { raw: val })
            }
        }
    }

    /// Parse part data into potentially multiple blocks (tool call + tool result).
    fn parse_part_data_multi(data_str: &str) -> Vec<ContentBlock> {
        let Ok(val) = serde_json::from_str::<serde_json::Value>(data_str) else {
            return Vec::new();
        };
        let part_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");

        if part_type == "tool" {
            let name = val
                .get("tool")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let id = val
                .get("callID")
                .and_then(|i| i.as_str())
                .map(|s| s.to_string());
            let state = val.get("state");
            let input = state
                .and_then(|s| s.get("input"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let status = state
                .and_then(|s| s.get("status"))
                .and_then(|s| s.as_str())
                .unwrap_or("pending");

            let mut blocks = vec![ContentBlock::ToolCall {
                id: id.clone(),
                name,
                input,
            }];

            if status == "completed" || status == "error" {
                let output = state.and_then(|s| s.get("output"));
                let content = output
                    .map(|o| {
                        if let Some(s) = o.as_str() {
                            s.to_string()
                        } else {
                            serde_json::to_string_pretty(o).unwrap_or_default()
                        }
                    })
                    .unwrap_or_default();
                blocks.push(ContentBlock::ToolResult {
                    id,
                    content,
                    is_error: status == "error",
                });
            }
            return blocks;
        }

        // For all other types, use the single-block parser
        Self::parse_part_data(data_str)
            .map(|b| vec![b])
            .unwrap_or_default()
    }

    /// Check if the required tables exist in the database.
    fn check_schema(conn: &Connection) -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('session','message','part')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count >= 3)
        .unwrap_or(false)
    }
}

#[async_trait]
impl super::SessionProvider for OpenCodeProvider {
    fn source(&self) -> SourceKind {
        SourceKind::OpenCode
    }

    fn parser_version(&self) -> &'static str {
        PARSER_VERSION
    }

    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        let conn = self.open_readonly()?;

        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.directory, s.time_created, s.time_updated, s.model,
                    (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id) as msg_count,
                    p.worktree, p.name
             FROM session s
             LEFT JOIN project p ON p.id = s.project_id
             WHERE s.time_archived IS NULL
             ORDER BY COALESCE(s.time_updated, s.time_created) DESC",
        )?;

        let summaries = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
                let directory: Option<String> = row.get(2)?;
                let time_created: i64 = row.get(3)?;
                let time_updated: Option<i64> = row.get(4)?;
                let model: Option<String> = row.get(5)?;
                let msg_count: usize = row.get(6)?;
                let worktree: Option<String> = row.get(7)?;
                let project_name: Option<String> = row.get(8)?;

                let project_path = directory.or(worktree);

                Ok(SessionSummary {
                    source: SourceKind::OpenCode,
                    external_session_id: id,
                    title: if title.is_empty() { None } else { Some(title) },
                    project_name,
                    project_path,
                    source_path: None,
                    started_at: Self::millis_to_datetime(time_created),
                    updated_at: time_updated.and_then(Self::millis_to_datetime),
                    message_count: msg_count,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(summaries)
    }

    async fn load_session(&self, summary: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        let conn = self.open_readonly()?;
        let session_id = &summary.external_session_id;

        // Load session info
        let (title, directory, time_created, time_updated, model) = conn.query_row(
            "SELECT s.title, s.directory, s.time_created, s.time_updated, s.model
             FROM session s WHERE s.id = ?1",
            [session_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )?;

        // Load all parts grouped by message_id (avoid N+1)
        let mut parts_map: HashMap<String, Vec<String>> = HashMap::new();
        {
            let mut part_stmt = conn.prepare(
                "SELECT message_id, data FROM part
                 WHERE session_id = ?1 ORDER BY time_created ASC",
            )?;
            let part_rows = part_stmt.query_map([session_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in part_rows.flatten() {
                parts_map.entry(row.0).or_default().push(row.1);
            }
        }

        // Load messages
        let mut msg_stmt = conn.prepare(
            "SELECT id, data, time_created FROM message
             WHERE session_id = ?1 ORDER BY time_created ASC",
        )?;

        let messages: Vec<NormalizedMessage> = msg_stmt
            .query_map([session_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .map(|(msg_id, data_str, time_created)| {
                let msg_data: serde_json::Value =
                    serde_json::from_str(&data_str).unwrap_or_default();

                let role_str = msg_data
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("user");
                let role = MessageRole::from_str(role_str);

                let model_id = msg_data
                    .get("modelID")
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string());

                let parent_id = msg_data
                    .get("parentID")
                    .and_then(|p| p.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());

                // Collect blocks from parts
                let blocks: Vec<ContentBlock> = parts_map
                    .get(&msg_id)
                    .map(|parts| {
                        parts
                            .iter()
                            .flat_map(|data| Self::parse_part_data_multi(data))
                            .collect()
                    })
                    .unwrap_or_default();

                NormalizedMessage {
                    external_id: msg_id,
                    parent_id,
                    role,
                    created_at: Self::millis_to_datetime(time_created),
                    model: model_id,
                    blocks,
                    usage: None, // OpenCode doesn't store usage per-message in parts
                    metadata: HashMap::new(),
                }
            })
            .collect();

        let project_path = directory.or_else(|| summary.project_path.clone());

        Ok(NormalizedSession {
            source: SourceKind::OpenCode,
            external_session_id: session_id.clone(),
            title: title.filter(|t| !t.is_empty()).or_else(|| summary.title.clone()),
            project_name: summary.project_name.clone(),
            project_path,
            source_path: Some(self.db_path.clone()),
            started_at: Self::millis_to_datetime(time_created),
            updated_at: time_updated.and_then(Self::millis_to_datetime),
            model,
            messages,
            usage: None,
            metadata: HashMap::new(),
        })
    }

    async fn health_check(&self) -> ProviderHealth {
        if !self.db_path.exists() {
            return ProviderHealth::NotFound {
                message: format!(
                    "OpenCode database not found at {}",
                    self.db_path.display()
                ),
            };
        }

        match self.open_readonly() {
            Ok(conn) => {
                if Self::check_schema(&conn) {
                    ProviderHealth::Ok
                } else {
                    ProviderHealth::Error {
                        message: "OpenCode DB schema not recognized (missing session/message/part tables)".to_string(),
                    }
                }
            }
            Err(e) => ProviderHealth::Error {
                message: format!("Cannot open OpenCode DB: {}", e),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::SessionProvider;
    use rusqlite::Connection;
    use tempfile::tempdir;

    fn create_test_db(dir: &std::path::Path) -> PathBuf {
        let db_path = dir.join("opencode.db");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute_batch(
            "CREATE TABLE project (
                id TEXT PRIMARY KEY,
                worktree TEXT NOT NULL,
                name TEXT,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL,
                sandboxes TEXT NOT NULL DEFAULT '[]'
            );
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                directory TEXT,
                title TEXT,
                version TEXT NOT NULL DEFAULT '0',
                time_created INTEGER NOT NULL,
                time_updated INTEGER,
                model TEXT,
                time_archived INTEGER
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE part (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL,
                data TEXT NOT NULL
            );",
        )
        .unwrap();

        // Insert test data
        conn.execute(
            "INSERT INTO project VALUES ('proj-1', '/home/user/myproject', 'MyProject', 1700000000000, 1700000000000, '[]')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO session VALUES ('sess-1', 'proj-1', '/home/user/myproject', 'Test Session', '1', 1700000000000, 1700000001000, 'gemini-3-pro', NULL)",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO message VALUES ('msg-1', 'sess-1', 1700000000000, 1700000000000, '{\"role\":\"user\",\"modelID\":null}')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO message VALUES ('msg-2', 'sess-1', 1700000001000, 1700000001000, '{\"role\":\"assistant\",\"modelID\":\"gemini-3-pro\"}')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO part VALUES ('part-1', 'msg-1', 'sess-1', 1700000000000, 1700000000000, '{\"type\":\"text\",\"text\":\"How do I sort a list?\"}')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO part VALUES ('part-2', 'msg-2', 'sess-1', 1700000001000, 1700000001000, '{\"type\":\"text\",\"text\":\"Use sorted() or list.sort()\"}')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO part VALUES ('part-3', 'msg-2', 'sess-1', 1700000001000, 1700000001000, '{\"type\":\"tool\",\"callID\":\"call-1\",\"tool\":\"bash\",\"state\":{\"status\":\"completed\",\"input\":{\"command\":\"ls\"},\"output\":\"file.txt\\n\"}}')",
            [],
        )
        .unwrap();

        db_path
    }

    #[tokio::test]
    async fn discover_and_load_session() {
        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path());

        let provider = OpenCodeProvider::new(Some(db_path)).unwrap();

        let summaries = provider.discover_sessions().await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].external_session_id, "sess-1");
        assert_eq!(summaries[0].title.as_deref(), Some("Test Session"));

        let session = provider.load_session(&summaries[0]).await.unwrap();
        assert_eq!(session.external_session_id, "sess-1");
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, MessageRole::User);
        assert_eq!(session.messages[1].role, MessageRole::Assistant);

        // Check blocks
        assert!(matches!(&session.messages[0].blocks[0], ContentBlock::Text { text } if text.contains("sort")));

        // Check tool call + result
        let has_tool_call = session.messages[1].blocks.iter().any(|b| matches!(b, ContentBlock::ToolCall { name, .. } if name == "bash"));
        let has_tool_result = session.messages[1].blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { content, .. } if content.contains("file.txt")));
        assert!(has_tool_call, "Should have tool call");
        assert!(has_tool_result, "Should have tool result");
    }

    #[tokio::test]
    async fn health_check_ok() {
        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path());
        let provider = OpenCodeProvider::new(Some(db_path)).unwrap();
        assert!(provider.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn health_check_not_found() {
        let provider = OpenCodeProvider::new(Some(PathBuf::from("/nonexistent/opencode.db"))).unwrap();
        assert!(!provider.health_check().await.is_ok());
    }
}
