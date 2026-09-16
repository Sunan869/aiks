use std::fs;
use std::path::Path;
use std::sync::Mutex;

use anyhow::Context;
use rusqlite::{Connection, OpenFlags};

const SCHEMA_V1_SQL: &str = include_str!("../../migrations/001_init.sql");
const SCHEMA_V3_SQL: &str = include_str!("../../migrations/002_v3_pipeline.sql");
const SCHEMA_V4_SQL: &str = include_str!("../../migrations/003_pipeline_job.sql");
const SCHEMA_V5_SQL: &str = include_str!("../../migrations/004_knowledge_sync.sql");
const SCHEMA_V6_SQL: &str = include_str!("../../migrations/005_knowledge_baseline.sql");
const SCHEMA_V7_SQL: &str = include_str!("../../migrations/006_pipeline_job_identity.sql");
const SCHEMA_V8_SQL: &str = include_str!("../../migrations/007_v4_native_knowledge.sql");

/// AIKS state database.
///
/// Thread-safe: wraps rusqlite::Connection in a Mutex so concurrent access
/// from Tauri commands, PipelineWorker, and Sync tasks is safe.
///
/// Rule: never hold the Mutex lock while awaiting HTTP, AI, or SiYuan calls.
/// Always acquire the lock, do the DB work, release, then do the async I/O.
pub struct StateDb {
    conn: Mutex<Connection>,
}

// StateDb is now genuinely thread-safe: the Mutex serializes all access.
// Send is safe because Connection is Send; the Mutex ensures Sync.
unsafe impl Send for StateDb {}
unsafe impl Sync for StateDb {}

impl StateDb {
    /// Open (or create) the state database at the given path.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create state DB dir: {}", parent.display()))?;
        }

        let conn =
            Connection::open(path).with_context(|| format!("open state DB: {}", path.display()))?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        conn.execute_batch("PRAGMA synchronous=NORMAL;")?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.run_migrations()?;
        Ok(db)
    }

    /// Open the state database with specific flags.
    pub fn open_with_flags(path: &Path, flags: OpenFlags) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open_with_flags(path, flags)
            .with_context(|| format!("open state DB with flags: {}", path.display()))?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn run_migrations(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("DB mutex poisoned");
        conn.execute_batch(SCHEMA_V1_SQL)
            .context("run V1 migrations")?;
        conn.execute_batch(SCHEMA_V3_SQL)
            .context("run V3 pipeline migrations")?;
        conn.execute_batch(SCHEMA_V4_SQL)
            .context("run V4 pipeline job migrations")?;
        conn.execute_batch(SCHEMA_V5_SQL)
            .context("run V5 knowledge sync migrations")?;

        let has_target_hash: i64 = {
            let mut stmt = conn
                .prepare("SELECT COUNT(*) FROM pragma_table_info('knowledge_sync_target') WHERE name = 'target_hash'")
                .context("prepare V6 pragma check")?;
            stmt.query_row([], |row| row.get(0))
                .context("run V6 pragma check")?
        };
        if has_target_hash == 0 {
            conn.execute_batch(SCHEMA_V6_SQL)
                .context("run V6 knowledge baseline migrations")?;
        }

        let has_job_session_id: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('pipeline_job') WHERE name = 'session_id'",
                [],
                |row| row.get(0),
            )
            .context("run V7 session_id pragma check")?;
        if has_job_session_id == 0 {
            conn.execute_batch("ALTER TABLE pipeline_job ADD COLUMN session_id INTEGER;")
                .context("add pipeline_job.session_id")?;
        }

        let has_job_run_id: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('pipeline_job') WHERE name = 'pipeline_run_id'",
                [],
                |row| row.get(0),
            )
            .context("run V7 pipeline_run_id pragma check")?;
        if has_job_run_id == 0 {
            conn.execute_batch("ALTER TABLE pipeline_job ADD COLUMN pipeline_run_id TEXT;")
                .context("add pipeline_job.pipeline_run_id")?;
        }

        conn.execute_batch(SCHEMA_V7_SQL)
            .context("run V7 pipeline job identity migrations")?;

        // V8 turns knowledge_item into the canonical Native First entity. The
        // migration rebuilds the table to relax source_session_id to NULL, so
        // only run it when the first V4 metadata column is absent.
        let has_source_type: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('knowledge_item') WHERE name = 'source_type'",
                [],
                |row| row.get(0),
            )
            .context("run V8 knowledge source_type pragma check")?;
        if has_source_type == 0 {
            conn.execute_batch(SCHEMA_V8_SQL)
                .context("run V8 native knowledge workbench migrations")?;
        }

        Ok(())
    }

    /// Get a locked reference to the underlying connection.
    ///
    /// IMPORTANT: Do NOT hold this lock across await points or while calling
    /// AI/HTTP/SiYuan APIs. Acquire the lock, do the DB work, then drop it.
    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("DB mutex poisoned")
    }

    /// Return all source sessions (for CLI/engine use)
    pub fn get_all_sessions_raw(&self) -> anyhow::Result<Vec<crate::storage::SourceSession>> {
        crate::storage::SourceSessionRepo::new(self).list_all()
    }

    /// Reset content hashes so sessions will be re-synced
    pub fn reset_hashes_for_resync(
        &self,
        source: Option<&str>,
        session_ids: &[String],
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn();
        if session_ids.is_empty() {
            if let Some(src) = source {
                conn.execute(
                    "UPDATE source_session SET content_hash = NULL, updated_at = ?2 WHERE source = ?1",
                    rusqlite::params![src, now],
                )?;
            } else {
                conn.execute(
                    "UPDATE source_session SET content_hash = NULL, updated_at = ?1",
                    rusqlite::params![now],
                )?;
            }
        } else {
            let src = source.unwrap_or("");
            for id in session_ids {
                if src.is_empty() {
                    conn.execute(
                        "UPDATE source_session SET content_hash = NULL, updated_at = ?2 WHERE external_session_id = ?1",
                        rusqlite::params![id, now],
                    )?;
                } else {
                    conn.execute(
                        "UPDATE source_session SET content_hash = NULL, updated_at = ?3 WHERE source = ?1 AND external_session_id = ?2",
                        rusqlite::params![src, id, now],
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn open_creates_tables() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = StateDb::open(&db_path).unwrap();

        let conn = db.conn();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('source_session','sync_target','source_file_state','sync_run')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 4);

        let v3_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('pipeline_run','pipeline_stage_run','knowledge_item','knowledge_chunk','embedding_record','session_chunk')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(v3_count, 6);

        let v4_columns: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('knowledge_item') WHERE name IN ('source_type','managed_by','status','is_favorite')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(v4_columns, 4);
    }

    #[test]
    fn open_twice_is_idempotent() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        StateDb::open(&db_path).unwrap();
        StateDb::open(&db_path).unwrap();
    }
}
