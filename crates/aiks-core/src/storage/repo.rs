use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::storage::StateDb;

// ===== SyncStatus =====

/// Synchronization status for a session → sink target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SyncStatus {
    /// Never synced yet
    New,
    /// Queued for sync
    Pending,
    /// Successfully synced
    Synced,
    /// Content changed, needs update
    Updated,
    /// Source unchanged, no sync needed
    Unchanged,
    /// Sync failed but can be retried
    FailedRetryable,
    /// Sync failed permanently
    FailedPermanent,
    /// Manual edit detected in target - do not overwrite without --overwrite
    Conflict,
    /// Source session is missing
    MissingSource,
}

impl SyncStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncStatus::New => "NEW",
            SyncStatus::Pending => "PENDING",
            SyncStatus::Synced => "SYNCED",
            SyncStatus::Updated => "UPDATED",
            SyncStatus::Unchanged => "UNCHANGED",
            SyncStatus::FailedRetryable => "FAILED_RETRYABLE",
            SyncStatus::FailedPermanent => "FAILED_PERMANENT",
            SyncStatus::Conflict => "CONFLICT",
            SyncStatus::MissingSource => "MISSING_SOURCE",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "NEW" => Some(SyncStatus::New),
            "PENDING" => Some(SyncStatus::Pending),
            "SYNCED" => Some(SyncStatus::Synced),
            "UPDATED" => Some(SyncStatus::Updated),
            "UNCHANGED" => Some(SyncStatus::Unchanged),
            "FAILED_RETRYABLE" => Some(SyncStatus::FailedRetryable),
            "FAILED_PERMANENT" => Some(SyncStatus::FailedPermanent),
            "CONFLICT" => Some(SyncStatus::Conflict),
            "MISSING_SOURCE" => Some(SyncStatus::MissingSource),
            _ => None,
        }
    }
}

// ===== SourceSession =====

#[derive(Debug, Clone)]
pub struct SourceSession {
    pub id: i64,
    pub source: String,
    pub external_session_id: String,
    pub source_path: Option<String>,
    pub project_path: Option<String>,
    pub project_name: Option<String>,
    pub title: Option<String>,
    pub source_updated_at: Option<String>,
    pub content_hash: Option<String>,
    pub parser_version: Option<String>,
    pub last_seen_at: String,
    pub is_missing: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub struct SourceSessionRepo<'a> {
    db: &'a StateDb,
}

impl<'a> SourceSessionRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    pub fn upsert(
        &self,
        source: &str,
        external_session_id: &str,
        source_path: Option<&str>,
        project_path: Option<&str>,
        project_name: Option<&str>,
        title: Option<&str>,
        source_updated_at: Option<&str>,
        content_hash: Option<&str>,
        parser_version: Option<&str>,
    ) -> anyhow::Result<i64> {
        let now = Utc::now().to_rfc3339();
        self.db.conn().execute(
            "INSERT INTO source_session
             (source, external_session_id, source_path, project_path, project_name, title,
              source_updated_at, content_hash, parser_version, last_seen_at, is_missing,
              created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?10, ?10)
             ON CONFLICT(source, external_session_id) DO UPDATE SET
               source_path = excluded.source_path,
               project_path = excluded.project_path,
               project_name = excluded.project_name,
               title = excluded.title,
               source_updated_at = excluded.source_updated_at,
               content_hash = excluded.content_hash,
               parser_version = excluded.parser_version,
               last_seen_at = excluded.last_seen_at,
               is_missing = 0,
               updated_at = excluded.updated_at",
            params![
                source, external_session_id, source_path, project_path, project_name,
                title, source_updated_at, content_hash, parser_version, now
            ],
        )?;

        let id: i64 = self.db.conn().query_row(
            "SELECT id FROM source_session WHERE source = ?1 AND external_session_id = ?2",
            params![source, external_session_id],
            |row| row.get(0),
        )?;

        Ok(id)
    }

    pub fn find_by_source_and_id(
        &self,
        source: &str,
        external_session_id: &str,
    ) -> anyhow::Result<Option<SourceSession>> {
        let result = self.db.conn().query_row(
            "SELECT id, source, external_session_id, source_path, project_path, project_name,
                    title, source_updated_at, content_hash, parser_version,
                    last_seen_at, is_missing, created_at, updated_at
             FROM source_session WHERE source = ?1 AND external_session_id = ?2",
            params![source, external_session_id],
            |row| {
                Ok(SourceSession {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    external_session_id: row.get(2)?,
                    source_path: row.get(3)?,
                    project_path: row.get(4)?,
                    project_name: row.get(5)?,
                    title: row.get(6)?,
                    source_updated_at: row.get(7)?,
                    content_hash: row.get(8)?,
                    parser_version: row.get(9)?,
                    last_seen_at: row.get(10)?,
                    is_missing: row.get::<_, i64>(11)? != 0,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            },
        );

        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn mark_missing(&self, source: &str, external_session_id: &str) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        self.db.conn().execute(
            "UPDATE source_session SET is_missing = 1, updated_at = ?3
             WHERE source = ?1 AND external_session_id = ?2",
            params![source, external_session_id, now],
        )?;
        Ok(())
    }

    pub fn list_all(&self) -> anyhow::Result<Vec<SourceSession>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, source, external_session_id, source_path, project_path, project_name,
                    title, source_updated_at, content_hash, parser_version,
                    last_seen_at, is_missing, created_at, updated_at
             FROM source_session ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SourceSession {
                id: row.get(0)?,
                source: row.get(1)?,
                external_session_id: row.get(2)?,
                source_path: row.get(3)?,
                project_path: row.get(4)?,
                project_name: row.get(5)?,
                title: row.get(6)?,
                source_updated_at: row.get(7)?,
                content_hash: row.get(8)?,
                parser_version: row.get(9)?,
                last_seen_at: row.get(10)?,
                is_missing: row.get::<_, i64>(11)? != 0,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
            })
        })?;
        let result: Vec<SourceSession> = rows.filter_map(|r| r.ok()).collect();
        Ok(result)
    }
}

// ===== SyncTarget =====

#[derive(Debug, Clone)]
pub struct SyncTarget {
    pub id: i64,
    pub session_id: i64,
    pub sink: String,
    pub target_id: Option<String>,
    pub target_path: Option<String>,
    pub synced_hash: Option<String>,
    pub target_hash: Option<String>,
    pub last_synced_at: Option<String>,
    pub status: SyncStatus,
    pub last_error: Option<String>,
    pub retry_count: i32,
}

pub struct SyncTargetRepo<'a> {
    db: &'a StateDb,
}

impl<'a> SyncTargetRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    pub fn upsert_pending(&self, session_id: i64, sink: &str) -> anyhow::Result<()> {
        self.db.conn().execute(
            "INSERT INTO sync_target (session_id, sink, status)
             VALUES (?1, ?2, 'PENDING')
             ON CONFLICT(session_id, sink) DO NOTHING",
            params![session_id, sink],
        )?;
        Ok(())
    }

    /// R05: record the remote document id/path as soon as it exists, without
    /// changing the sync status. This makes retries resume with UPDATE instead
    /// of creating orphan duplicate documents when a later step fails.
    pub fn record_target_doc(
        &self,
        session_id: i64,
        sink: &str,
        target_id: &str,
        target_path: &str,
    ) -> anyhow::Result<()> {
        self.db.conn().execute(
            "UPDATE sync_target SET target_id = ?3, target_path = ?4
             WHERE session_id = ?1 AND sink = ?2",
            params![session_id, sink, target_id, target_path],
        )?;
        Ok(())
    }

    pub fn find(&self, session_id: i64, sink: &str) -> anyhow::Result<Option<SyncTarget>> {
        let result = self.db.conn().query_row(
            "SELECT id, session_id, sink, target_id, target_path, synced_hash, target_hash,
                    last_synced_at, status, last_error, retry_count
             FROM sync_target WHERE session_id = ?1 AND sink = ?2",
            params![session_id, sink],
            |row| {
                let status_str: String = row.get(8)?;
                Ok(SyncTarget {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    sink: row.get(2)?,
                    target_id: row.get(3)?,
                    target_path: row.get(4)?,
                    synced_hash: row.get(5)?,
                    target_hash: row.get(6)?,
                    last_synced_at: row.get(7)?,
                    status: SyncStatus::from_str(&status_str).unwrap_or(SyncStatus::Pending),
                    last_error: row.get(9)?,
                    retry_count: row.get(10)?,
                })
            },
        );

        match result {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn mark_synced(
        &self,
        session_id: i64,
        sink: &str,
        target_id: &str,
        target_path: &str,
        synced_hash: &str,
        target_hash: &str,
    ) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        self.db.conn().execute(
            "UPDATE sync_target
             SET status = 'SYNCED', target_id = ?3, target_path = ?4,
                 synced_hash = ?5, target_hash = ?6, last_synced_at = ?7,
                 last_error = NULL, retry_count = 0
             WHERE session_id = ?1 AND sink = ?2",
            params![session_id, sink, target_id, target_path, synced_hash, target_hash, now],
        )?;
        Ok(())
    }

    pub fn mark_conflict(&self, session_id: i64, sink: &str) -> anyhow::Result<()> {
        self.db.conn().execute(
            "UPDATE sync_target SET status = 'CONFLICT' WHERE session_id = ?1 AND sink = ?2",
            params![session_id, sink],
        )?;
        Ok(())
    }

    pub fn mark_failed(
        &self,
        session_id: i64,
        sink: &str,
        error: &str,
        retryable: bool,
    ) -> anyhow::Result<()> {
        let status = if retryable { "FAILED_RETRYABLE" } else { "FAILED_PERMANENT" };
        self.db.conn().execute(
            "UPDATE sync_target
             SET status = ?3, last_error = ?4, retry_count = retry_count + 1
             WHERE session_id = ?1 AND sink = ?2",
            params![session_id, sink, status, error],
        )?;
        Ok(())
    }

    pub fn list_pending(&self, sink: &str) -> anyhow::Result<Vec<SyncTarget>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, sink, target_id, target_path, synced_hash, target_hash,
                    last_synced_at, status, last_error, retry_count
             FROM sync_target
             WHERE sink = ?1 AND status IN ('PENDING', 'FAILED_RETRYABLE', 'NEW')",
        )?;
        let rows = stmt.query_map([sink], |row| {
            let status_str: String = row.get(8)?;
            Ok(SyncTarget {
                id: row.get(0)?,
                session_id: row.get(1)?,
                sink: row.get(2)?,
                target_id: row.get(3)?,
                target_path: row.get(4)?,
                synced_hash: row.get(5)?,
                target_hash: row.get(6)?,
                last_synced_at: row.get(7)?,
                status: SyncStatus::from_str(&status_str).unwrap_or(SyncStatus::Pending),
                last_error: row.get(9)?,
                retry_count: row.get(10)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }
}

// ===== SourceFileState =====

#[derive(Debug, Clone)]
pub struct SourceFileState {
    pub path: String,
    pub source: String,
    pub file_size: Option<i64>,
    pub modified_at: Option<String>,
    pub last_offset: Option<i64>,
    pub file_hash: Option<String>,
    pub parser_version: Option<String>,
    pub updated_at: String,
}

pub struct SourceFileStateRepo<'a> {
    db: &'a StateDb,
}

impl<'a> SourceFileStateRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    pub fn upsert(
        &self,
        path: &str,
        source: &str,
        file_size: Option<i64>,
        modified_at: Option<&str>,
        last_offset: Option<i64>,
        file_hash: Option<&str>,
        parser_version: Option<&str>,
    ) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        self.db.conn().execute(
            "INSERT INTO source_file_state
             (path, source, file_size, modified_at, last_offset, file_hash, parser_version, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(path) DO UPDATE SET
               file_size = excluded.file_size,
               modified_at = excluded.modified_at,
               last_offset = excluded.last_offset,
               file_hash = excluded.file_hash,
               parser_version = excluded.parser_version,
               updated_at = excluded.updated_at",
            params![path, source, file_size, modified_at, last_offset, file_hash, parser_version, now],
        )?;
        Ok(())
    }

    pub fn find(&self, path: &str) -> anyhow::Result<Option<SourceFileState>> {
        let result = self.db.conn().query_row(
            "SELECT path, source, file_size, modified_at, last_offset, file_hash, parser_version, updated_at
             FROM source_file_state WHERE path = ?1",
            [path],
            |row| {
                Ok(SourceFileState {
                    path: row.get(0)?,
                    source: row.get(1)?,
                    file_size: row.get(2)?,
                    modified_at: row.get(3)?,
                    last_offset: row.get(4)?,
                    file_hash: row.get(5)?,
                    parser_version: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        );

        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

// ===== SyncRun =====

#[derive(Debug, Clone)]
pub struct SyncRun {
    pub id: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub trigger_type: Option<String>,
    pub discovered: i64,
    pub changed: i64,
    pub synced: i64,
    pub failed: i64,
}

pub struct SyncRunRepo<'a> {
    db: &'a StateDb,
}

impl<'a> SyncRunRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    pub fn start(&self, trigger_type: &str) -> anyhow::Result<i64> {
        let now = Utc::now().to_rfc3339();
        self.db.conn().execute(
            "INSERT INTO sync_run (started_at, trigger_type, discovered, changed, synced, failed)
             VALUES (?1, ?2, 0, 0, 0, 0)",
            params![now, trigger_type],
        )?;
        Ok(self.db.conn().last_insert_rowid())
    }

    pub fn finish(
        &self,
        run_id: i64,
        discovered: i64,
        changed: i64,
        synced: i64,
        failed: i64,
    ) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        self.db.conn().execute(
            "UPDATE sync_run SET finished_at = ?2, discovered = ?3, changed = ?4,
             synced = ?5, failed = ?6 WHERE id = ?1",
            params![run_id, now, discovered, changed, synced, failed],
        )?;
        Ok(())
    }

    pub fn last_run(&self) -> anyhow::Result<Option<SyncRun>> {
        let result = self.db.conn().query_row(
            "SELECT id, started_at, finished_at, trigger_type, discovered, changed, synced, failed
             FROM sync_run ORDER BY id DESC LIMIT 1",
            [],
            |row| {
                Ok(SyncRun {
                    id: row.get(0)?,
                    started_at: row.get(1)?,
                    finished_at: row.get(2)?,
                    trigger_type: row.get(3)?,
                    discovered: row.get(4)?,
                    changed: row.get(5)?,
                    synced: row.get(6)?,
                    failed: row.get(7)?,
                })
            },
        );
        match result {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StateDb;
    use tempfile::tempdir;

    fn make_db() -> (tempfile::TempDir, StateDb) {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("test.db")).unwrap();
        (dir, db)
    }

    #[test]
    fn upsert_source_session() {
        let (_dir, db) = make_db();
        let repo = SourceSessionRepo::new(&db);

        let id1 = repo
            .upsert("claude_code", "sess-1", Some("/path/to/session.jsonl"), None, None, Some("Hello"), None, Some("abc123"), Some("claude-v1"))
            .unwrap();

        let found = repo.find_by_source_and_id("claude_code", "sess-1").unwrap().unwrap();
        assert_eq!(found.id, id1);
        assert_eq!(found.content_hash.as_deref(), Some("abc123"));

        // Upsert again with different hash
        repo.upsert("claude_code", "sess-1", Some("/path/to/session.jsonl"), None, None, Some("Hello updated"), None, Some("def456"), Some("claude-v1"))
            .unwrap();

        let found2 = repo.find_by_source_and_id("claude_code", "sess-1").unwrap().unwrap();
        assert_eq!(found2.content_hash.as_deref(), Some("def456"));
        assert_eq!(found2.id, id1); // Same row updated
    }

    #[test]
    fn sync_target_lifecycle() {
        let (_dir, db) = make_db();
        let session_repo = SourceSessionRepo::new(&db);
        let target_repo = SyncTargetRepo::new(&db);

        let session_id = session_repo
            .upsert("claude_code", "sess-1", None, None, None, None, None, None, None)
            .unwrap();

        target_repo.upsert_pending(session_id, "siyuan").unwrap();

        let target = target_repo.find(session_id, "siyuan").unwrap().unwrap();
        assert_eq!(target.status, SyncStatus::Pending);

        target_repo
            .mark_synced(session_id, "siyuan", "doc-id", "/path", "hash-1", "target-hash-1")
            .unwrap();

        let target = target_repo.find(session_id, "siyuan").unwrap().unwrap();
        assert_eq!(target.status, SyncStatus::Synced);
        assert_eq!(target.synced_hash.as_deref(), Some("hash-1"));
    }

    #[test]
    fn sync_run_lifecycle() {
        let (_dir, db) = make_db();
        let repo = SyncRunRepo::new(&db);

        let run_id = repo.start("manual").unwrap();
        repo.finish(run_id, 10, 5, 4, 1).unwrap();

        let last = repo.last_run().unwrap().unwrap();
        assert_eq!(last.id, run_id);
        assert_eq!(last.discovered, 10);
        assert_eq!(last.synced, 4);
        assert!(last.finished_at.is_some());
    }
}
