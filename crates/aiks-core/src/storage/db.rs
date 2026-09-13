use std::fs;
use std::path::Path;

use anyhow::Context;
use rusqlite::{Connection, OpenFlags};

const SCHEMA_SQL: &str = include_str!("../../migrations/001_init.sql");

/// AIKS state database.
///
/// Tracks source sessions, sync targets, file states, and sync runs.
pub struct StateDb {
    conn: Connection,
}

// Safety: StateDb wraps a single SQLite connection.
// rusqlite::Connection is not Send by default because it holds a raw pointer,
// but SQLite with SERIALIZED threading mode (bundled default) is safe to use
// from any thread as long as access is serialized — which we guarantee by
// wrapping StateDb in a Mutex at the AiksEngine level.
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

        let db = Self { conn };
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
        Ok(Self { conn })
    }

    fn run_migrations(&self) -> anyhow::Result<()> {
        self.conn
            .execute_batch(SCHEMA_SQL)
            .context("run migrations")?;
        Ok(())
    }

    /// Get a reference to the underlying connection.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Get a mutable reference to the underlying connection.
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
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
        if session_ids.is_empty() {
            if let Some(src) = source {
                self.conn.execute(
                    "UPDATE source_session SET content_hash = NULL, updated_at = ?2 WHERE source = ?1",
                    rusqlite::params![src, now],
                )?;
            } else {
                self.conn.execute(
                    "UPDATE source_session SET content_hash = NULL, updated_at = ?1",
                    rusqlite::params![now],
                )?;
            }
        } else {
            for id in session_ids {
                self.conn.execute(
                    "UPDATE source_session SET content_hash = NULL, updated_at = ?3 WHERE source = ?1 AND external_session_id = ?2",
                    rusqlite::params![source.unwrap_or(""), id, now],
                )?;
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

        let count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('source_session','sync_target','source_file_state','sync_run')",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 4);
    }

    #[test]
    fn open_twice_is_idempotent() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        StateDb::open(&db_path).unwrap();
        // Should not fail on second open (IF NOT EXISTS in schema)
        StateDb::open(&db_path).unwrap();
    }
}
