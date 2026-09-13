use std::fs;
use std::path::Path;
use std::sync::Mutex;

use anyhow::Context;
use rusqlite::{Connection, OpenFlags};

const SCHEMA_V1_SQL: &str = include_str!("../../migrations/001_init.sql");
const SCHEMA_V3_SQL: &str = include_str!("../../migrations/002_v3_pipeline.sql");
const SCHEMA_V4_SQL: &str = include_str!("../../migrations/003_pipeline_job.sql");

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

        let conn = Connection::open(path)
            .with_context(|| format!("open state DB: {}", path.display()))?;

        // Enable WAL mode for better concurrent access
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        conn.execute_batch("PRAGMA synchronous=NORMAL;")?;

        let db = Self { conn: Mutex::new(conn) };
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
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn run_migrations(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("DB mutex poisoned");
        conn.execute_batch(SCHEMA_V1_SQL).context("run V1 migrations")?;
        conn.execute_batch(SCHEMA_V3_SQL).context("run V3 pipeline migrations")?;
        conn.execute_batch(SCHEMA_V4_SQL).context("run V4 pipeline job migrations")?;
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
                    // Reset by external_session_id regardless of source
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

        // V1 tables
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('source_session','sync_target','source_file_state','sync_run')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 4);

        // V3 pipeline tables
        let v3_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('pipeline_run','pipeline_stage_run','knowledge_item','knowledge_chunk','embedding_record','session_chunk')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(v3_count, 6);
    }

    #[test]
    fn open_twice_is_idempotent() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        StateDb::open(&db_path).unwrap();
        StateDb::open(&db_path).unwrap();
    }
}
