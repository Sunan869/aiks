//! WorkBuddy session provider.
//!
//! WorkBuddy keeps session metadata in `workbuddy.db` and transcript events
//! under `projects/**/*.jsonl`. This provider is strictly read-only with
//! respect to the WorkBuddy data root.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};

use crate::model::{NormalizedSession, SourceKind};
use crate::providers::{ProviderHealth, SessionProvider, SessionSummary};

const PARSER_VERSION: &str = "workbuddy-jsonl-v1";

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

    fn project_name_from_cwd(cwd: &str) -> Option<String> {
        let trimmed = cwd.trim_end_matches(|c| c == '/' || c == '\\');
        trimmed
            .rsplit(|c| c == '/' || c == '\\')
            .find(|segment| !segment.is_empty())
            .map(str::to_owned)
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

        let sql = format!(
            "SELECT id, cwd, title, custom_title, created_at, updated_at, \
                    {status_expr}, {mode_expr}, {last_activity_expr}, \
                    {permission_mode_expr}, {is_playground_expr}, {model_expr} \
             FROM sessions \
             WHERE deleted_at IS NULL \
             ORDER BY COALESCE(last_activity_at, updated_at, created_at) DESC"
        );

        // If last_activity_at is absent, the ORDER BY must not reference a
        // non-existent column even though the SELECT uses a NULL alias.
        let sql = if columns.contains("last_activity_at") {
            sql
        } else {
            sql.replace(
                "ORDER BY COALESCE(last_activity_at, updated_at, created_at) DESC",
                "ORDER BY COALESCE(updated_at, created_at) DESC",
            )
        };

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
                source_path: None,
                started_at: Self::millis_to_datetime(created_at),
                updated_at: Self::millis_to_datetime(last_activity_at.unwrap_or(updated_at)),
                message_count: 0,
            })
        })?;

        Ok(rows.filter_map(Result::ok).collect())
    }

    async fn load_session(&self, _summary: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        anyhow::bail!(
            "WorkBuddy transcript loading is not available until transcript parsing is enabled"
        )
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
