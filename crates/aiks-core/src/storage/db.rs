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
const SCHEMA_V9_SQL: &str = include_str!("../../migrations/008_v41_siyuan_content_source.sql");
const SCHEMA_V10_SQL: &str = include_str!("../../migrations/009_v42_knowledge_index.sql");

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

        // V9 is deliberately additive. V4 content columns remain in place as
        // migration snapshots/cache while SiYuan becomes the canonical content
        // store. Each ALTER is guarded so upgrading an existing V4 database is
        // safe and opening the same database repeatedly remains idempotent.
        Self::ensure_column(
            &conn,
            "knowledge_item",
            "siyuan_doc_id",
            "ALTER TABLE knowledge_item ADD COLUMN siyuan_doc_id TEXT;",
        )?;
        Self::ensure_column(
            &conn,
            "knowledge_item",
            "generated_hash",
            "ALTER TABLE knowledge_item ADD COLUMN generated_hash TEXT;",
        )?;
        Self::ensure_column(
            &conn,
            "knowledge_item",
            "current_remote_hash",
            "ALTER TABLE knowledge_item ADD COLUMN current_remote_hash TEXT;",
        )?;
        Self::ensure_column(
            &conn,
            "knowledge_item",
            "migration_status",
            "ALTER TABLE knowledge_item ADD COLUMN migration_status TEXT NOT NULL DEFAULT 'pending';",
        )?;
        Self::ensure_column(
            &conn,
            "source_session",
            "siyuan_doc_id",
            "ALTER TABLE source_session ADD COLUMN siyuan_doc_id TEXT;",
        )?;
        conn.execute_batch(SCHEMA_V9_SQL)
            .context("run V9 SiYuan content source migrations")?;

        // V10 adds explicit per-document indexing state. All ALTER statements
        // are guarded so databases created by earlier V4 builds upgrade safely
        // and repeated opens remain idempotent.
        Self::ensure_column(
            &conn,
            "knowledge_item",
            "index_status",
            "ALTER TABLE knowledge_item ADD COLUMN index_status TEXT NOT NULL DEFAULT 'pending';",
        )?;
        Self::ensure_column(
            &conn,
            "knowledge_item",
            "indexed_hash",
            "ALTER TABLE knowledge_item ADD COLUMN indexed_hash TEXT;",
        )?;
        Self::ensure_column(
            &conn,
            "knowledge_item",
            "indexed_at",
            "ALTER TABLE knowledge_item ADD COLUMN indexed_at TEXT;",
        )?;
        Self::ensure_column(
            &conn,
            "knowledge_item",
            "embedding_model",
            "ALTER TABLE knowledge_item ADD COLUMN embedding_model TEXT;",
        )?;
        Self::ensure_column(
            &conn,
            "knowledge_item",
            "embedding_dimensions",
            "ALTER TABLE knowledge_item ADD COLUMN embedding_dimensions INTEGER;",
        )?;
        Self::ensure_column(
            &conn,
            "knowledge_item",
            "index_chunk_count",
            "ALTER TABLE knowledge_item ADD COLUMN index_chunk_count INTEGER NOT NULL DEFAULT 0;",
        )?;
        Self::ensure_column(
            &conn,
            "knowledge_item",
            "last_index_error",
            "ALTER TABLE knowledge_item ADD COLUMN last_index_error TEXT;",
        )?;
        conn.execute_batch(SCHEMA_V10_SQL)
            .context("run V10 knowledge index lifecycle migrations")?;

        Ok(())
    }

    fn ensure_column(
        conn: &Connection,
        table: &str,
        column: &str,
        alter_sql: &str,
    ) -> anyhow::Result<()> {
        let sql = format!(
            "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = ?1",
            table.replace('\'', "''")
        );
        let exists: i64 = conn
            .query_row(&sql, [column], |row| row.get(0))
            .with_context(|| format!("check {table}.{column}"))?;
        if exists == 0 {
            conn.execute_batch(alter_sql)
                .with_context(|| format!("add {table}.{column}"))?;
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

        let v41_columns: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('knowledge_item') WHERE name IN ('siyuan_doc_id','generated_hash','current_remote_hash','migration_status')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(v41_columns, 4);
    }

    #[test]
    fn open_twice_is_idempotent() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        StateDb::open(&db_path).unwrap();
        StateDb::open(&db_path).unwrap();
    }
}
