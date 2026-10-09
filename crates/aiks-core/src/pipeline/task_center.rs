//! Read-only projection for the local sync and AI task center.
//! Do not introduce a second state machine or task queue.
use rusqlite::params;
use serde::{Deserialize, Serialize};
use crate::storage::StateDb;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCenterEntry {
    pub session_id: i64,
    pub source: String,
    pub external_session_id: String,
    pub title: Option<String>,
    pub sync_status: Option<String>,
    pub sync_error: Option<String>,
    pub pipeline_status: Option<String>,
    pub current_stage: Option<String>,
    pub pipeline_error: Option<String>,
    pub job_status: Option<String>,
    pub attempts: Option<i64>,
}

pub struct TaskCenterRepo<'a> {
    db: &'a StateDb,
}

impl<'a> TaskCenterRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    /// Bounded list of canonical sessions and their independent persisted states.
    /// No raw Session content, prompts, endpoints, or API keys are returned.
    pub fn list_recent(&self, limit: usize) -> anyhow::Result<Vec<TaskCenterEntry>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT ss.id, ss.source, ss.external_session_id, ss.title,
                    st.status, st.last_error,
                    pr.status, pr.current_stage, pr.error_message,
                    pj.status, pj.attempt
             FROM source_session ss
             LEFT JOIN sync_target st ON st.session_id = ss.id AND st.sink = 'siyuan'
             LEFT JOIN pipeline_run pr ON pr.session_id = ss.id
               AND pr.pipeline_version = 'v3'
             LEFT JOIN pipeline_job pj ON pj.id = (
                SELECT p.id FROM pipeline_job p
                WHERE p.session_id = ss.id
                ORDER BY p.generation DESC, p.created_at DESC LIMIT 1
             )
             ORDER BY ss.id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit.clamp(1, 500) as i64], |row| {
            Ok(TaskCenterEntry {
                session_id: row.get(0)?,
                source: row.get(1)?,
                external_session_id: row.get(2)?,
                title: row.get(3)?,
                sync_status: row.get(4)?,
                sync_error: row.get(5)?,
                pipeline_status: row.get(6)?,
                current_stage: row.get(7)?,
                pipeline_error: row.get(8)?,
                job_status: row.get(9)?,
                attempts: row.get(10)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_database_has_no_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        assert!(TaskCenterRepo::new(&db).list_recent(25).unwrap().is_empty());
    }

    #[test]
    fn sessions_without_sync_or_ai_still_appear() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        db.conn().execute(
            "INSERT INTO source_session
             (source, external_session_id, title, last_seen_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4, ?4)",
            params!["codex", "rollout-demo", "Example", "2026-10-09"],
        ).unwrap();
        let rows = TaskCenterRepo::new(&db).list_recent(20).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, "codex");
        assert!(rows[0].sync_status.is_none());
        assert!(rows[0].pipeline_status.is_none());
    }
}
