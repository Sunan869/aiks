//! Read-only projection for the local sync and AI task center.
//! Do not introduce a second state machine or task queue.
use crate::storage::StateDb;
use rusqlite::params;
use serde::{Deserialize, Serialize};

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
    pub job_error: Option<String>,
    pub stage_latency_ms: Option<i64>,
    pub last_task_update: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCenterStats {
    pub total_sessions: i64,
    pub pending: i64,
    pub running: i64,
    pub cancelled: i64,
    pub sync_issues: i64,
    pub ai_issues: i64,
}

pub struct TaskCenterRepo<'a> {
    db: &'a StateDb,
}

impl<'a> TaskCenterRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    /// Global counts independent of the most recent task list limit.
    pub fn stats(&self) -> anyhow::Result<TaskCenterStats> {
        let conn = self.db.conn();
        let result = conn.query_row(
            "SELECT
                (SELECT COUNT(*) FROM source_session),
                (SELECT COUNT(*) FROM pipeline_job pj WHERE pj.status = 'PENDING'
                 AND pj.generation = (SELECT MAX(p.generation) FROM pipeline_job p WHERE p.session_id = pj.session_id)),
                (SELECT COUNT(*) FROM pipeline_job pj WHERE pj.status = 'RUNNING'
                 AND pj.generation = (SELECT MAX(p.generation) FROM pipeline_job p WHERE p.session_id = pj.session_id)),
                (SELECT COUNT(*) FROM pipeline_job pj WHERE pj.status = 'CANCELLED'
                 AND pj.generation = (SELECT MAX(p.generation) FROM pipeline_job p WHERE p.session_id = pj.session_id)),
                (SELECT COUNT(*) FROM sync_target WHERE sink = 'siyuan'
                 AND (status LIKE 'FAILED%' OR status = 'CONFLICT')),
                (SELECT COUNT(*) FROM source_session ss WHERE
                    (EXISTS (
                        SELECT 1 FROM pipeline_job pj WHERE pj.session_id = ss.id
                        AND pj.status = 'FAILED'
                        AND pj.generation = (
                            SELECT MAX(p.generation) FROM pipeline_job p WHERE p.session_id = ss.id
                        )
                    ) OR (
                        NOT EXISTS (
                            SELECT 1 FROM pipeline_job pj WHERE pj.session_id = ss.id
                        ) AND EXISTS (
                            SELECT 1 FROM pipeline_run pr WHERE pr.id = (
                                SELECT p.id FROM pipeline_run p
                                WHERE p.session_id = ss.id AND p.pipeline_version = 'v3'
                                ORDER BY p.updated_at DESC, p.created_at DESC, p.rowid DESC
                                LIMIT 1
                            ) AND pr.status = 'FAILED'
                        )
                    )))",
            [],
            |row| {
                Ok(TaskCenterStats {
                    total_sessions: row.get(0)?,
                    pending: row.get(1)?,
                    running: row.get(2)?,
                    cancelled: row.get(3)?,
                    sync_issues: row.get(4)?,
                    ai_issues: row.get(5)?,
                })
            },
        )?;
        Ok(result)
    }

    /// Bounded list of canonical sessions and their independent persisted states.
    /// No raw Session content, prompts, endpoints, or API keys are returned.
    pub fn list_recent(&self, limit: usize) -> anyhow::Result<Vec<TaskCenterEntry>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT ss.id, ss.source, ss.external_session_id, ss.title,
                    st.status, st.last_error,
                    CASE WHEN pj.status = 'PENDING' THEN 'DISCOVERED'
                         WHEN pj.status = 'RUNNING' THEN 'PROCESSING'
                         WHEN pj.status = 'CANCELLED' THEN 'CANCELLED'
                         ELSE pr.status END,
                    CASE WHEN pj.status IN ('PENDING', 'RUNNING', 'CANCELLED') THEN NULL
                         ELSE pr.current_stage END,
                    CASE WHEN pj.status IN ('PENDING', 'RUNNING', 'DONE', 'CANCELLED') THEN NULL
                         ELSE pr.error_message END,
                    pj.status, pj.attempt,
                    CASE WHEN pj.status IN ('PENDING', 'RUNNING', 'DONE', 'CANCELLED') THEN NULL
                         ELSE pj.last_error END,
                    (SELECT ps.latency_ms FROM pipeline_stage_run ps
                     WHERE ps.pipeline_run_id = pr.id AND ps.latency_ms IS NOT NULL
                     ORDER BY ps.finished_at DESC, ps.rowid DESC LIMIT 1),
                    MAX(COALESCE(pj.updated_at, ''),
                        COALESCE(pr.updated_at, ''), COALESCE(ss.updated_at, ''))
             FROM source_session ss
             LEFT JOIN sync_target st ON st.session_id = ss.id AND st.sink = 'siyuan'
             LEFT JOIN pipeline_job pj ON pj.id = (
                SELECT p.id FROM pipeline_job p
                WHERE p.session_id = ss.id
                ORDER BY p.generation DESC, p.created_at DESC LIMIT 1
             )
             LEFT JOIN pipeline_run pr ON pr.id = COALESCE(
                pj.pipeline_run_id,
                (SELECT p.id FROM pipeline_run p
                 WHERE p.session_id = ss.id AND p.pipeline_version = 'v3'
                 ORDER BY p.updated_at DESC, p.created_at DESC, p.rowid DESC
                 LIMIT 1)
             ) AND pr.session_id = ss.id AND pr.pipeline_version = 'v3'
             ORDER BY MAX(COALESCE(pj.updated_at, ''),
                          COALESCE(pr.updated_at, ''), COALESCE(ss.updated_at, '')) DESC,
                      ss.id DESC LIMIT ?1",
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
                job_error: row.get(11)?,
                stage_latency_ms: row.get(12)?,
                last_task_update: row.get(13)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_count_all_sessions_outside_recent_window() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        for i in 0..5 {
            db.conn()
                .execute(
                    "INSERT INTO source_session
                 (source, external_session_id, last_seen_at, created_at, updated_at)
                 VALUES ('codex', ?1, 'now', 'now', 'now')",
                    params![format!("s-{i}")],
                )
                .unwrap();
        }
        let repo = TaskCenterRepo::new(&db);
        assert_eq!(repo.list_recent(1).unwrap().len(), 1);
        assert_eq!(repo.stats().unwrap().total_sessions, 5);
    }

    #[test]
    fn recent_tasks_are_unique_and_follow_latest_activity_not_session_id() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        for (external, updated) in [
            ("older-session", "2026-10-01T00:00:00Z"),
            ("newer-session", "2026-10-03T00:00:00Z"),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO source_session
                     (source, external_session_id, last_seen_at, created_at, updated_at)
                     VALUES ('codex', ?1, ?2, ?2, ?2)",
                    params![external, updated],
                )
                .unwrap();
        }
        let older_id: i64 = db
            .conn()
            .query_row(
                "SELECT id FROM source_session WHERE external_session_id = 'older-session'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        for (run, status, updated) in [
            ("old-run", "FAILED", "2026-10-02T00:00:00Z"),
            ("current-run", "READY", "2026-10-04T00:00:00Z"),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO pipeline_run
                     (id, session_id, pipeline_version, status, created_at, updated_at)
                     VALUES (?1, ?2, 'v3', ?3, ?4, ?4)",
                    params![run, older_id, status, updated],
                )
                .unwrap();
        }
        let recent = TaskCenterRepo::new(&db).list_recent(1).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].external_session_id, "older-session");
        assert_eq!(recent[0].pipeline_status.as_deref(), Some("READY"));
        assert_eq!(
            recent[0].last_task_update.as_deref(),
            Some("2026-10-04T00:00:00Z")
        );
        let all = TaskCenterRepo::new(&db).list_recent(10).unwrap();
        assert_eq!(
            all.len(),
            2,
            "multiple pipeline runs must not duplicate a session"
        );
    }

    #[test]
    fn latest_generation_supersedes_old_failure_in_statistics() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let id: i64 = db
            .conn()
            .query_row(
                "INSERT INTO source_session
                 (source, external_session_id, last_seen_at, created_at, updated_at)
                 VALUES ('codex', 'generation-stats', 'now', 'now', 'now') RETURNING id",
                [],
                |row| row.get(0),
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO pipeline_run
                 (id, session_id, status, pipeline_version, created_at, updated_at)
                 VALUES ('run-stats', ?1, 'FAILED', 'v3', 'now', 'now')",
                params![id],
            )
            .unwrap();
        let repo = TaskCenterRepo::new(&db);
        assert_eq!(repo.stats().unwrap().ai_issues, 1);
        db.conn()
            .execute(
                "INSERT INTO pipeline_job
                 (id, source, external_session_id, generation, status,
                  created_at, updated_at, session_id, pipeline_run_id)
                 VALUES ('old-failure', 'codex', 'generation-stats', 1, 'FAILED',
                         'now', 'now', ?1, 'run-stats')",
                params![id],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO pipeline_job
                 (id, source, external_session_id, generation, status,
                  created_at, updated_at, session_id, pipeline_run_id)
                 VALUES ('new-queue', 'codex', 'generation-stats', 2, 'PENDING',
                         'now', 'now', ?1, 'run-stats')",
                params![id],
            )
            .unwrap();
        let stats = repo.stats().unwrap();
        assert_eq!(stats.ai_issues, 0);
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.total_sessions, 1);
    }

    #[test]
    fn global_stats_ignore_superseded_running_jobs_and_old_pipeline_failures() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let id: i64 = db
            .conn()
            .query_row(
                "INSERT INTO source_session
                 (source, external_session_id, last_seen_at, created_at, updated_at)
                 VALUES ('codex', 'historical-errors', 'now', 'now', 'now') RETURNING id",
                [],
                |row| row.get(0),
            )
            .unwrap();
        for (run, status, updated) in [
            ("previous-failure", "FAILED", "2026-10-01T00:00:00Z"),
            ("latest-success", "READY", "2026-10-02T00:00:00Z"),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO pipeline_run
                     (id, session_id, pipeline_version, status, created_at, updated_at)
                     VALUES (?1, ?2, 'v3', ?3, ?4, ?4)",
                    params![run, id, status, updated],
                )
                .unwrap();
        }
        let tasks = TaskCenterRepo::new(&db);
        assert_eq!(tasks.stats().unwrap().ai_issues, 0);
        for (job, generation, status) in
            [("stale-running", 1, "RUNNING"), ("latest-done", 2, "DONE")]
        {
            db.conn()
                .execute(
                    "INSERT INTO pipeline_job
                     (id, source, external_session_id, generation, status,
                      created_at, updated_at, session_id, pipeline_run_id)
                     VALUES (?1, 'codex', 'historical-errors', ?2, ?3,
                             'now', 'now', ?4, 'latest-success')",
                    params![job, generation, status, id],
                )
                .unwrap();
        }
        assert_eq!(tasks.stats().unwrap().running, 0);
        assert_eq!(tasks.stats().unwrap().ai_issues, 0);
        assert_eq!(
            tasks.list_recent(5).unwrap()[0].job_status.as_deref(),
            Some("DONE")
        );
    }

    #[test]
    fn latest_queued_generation_hides_stale_failure_details() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let id: i64 = db
            .conn()
            .query_row(
                "INSERT INTO source_session
                 (source, external_session_id, last_seen_at, created_at, updated_at)
                 VALUES ('codex', 'stale-errors', 'now', 'now', 'now') RETURNING id",
                [],
                |row| row.get(0),
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO pipeline_run
                 (id, session_id, status, pipeline_version, error_message, current_stage,
                  created_at, updated_at)
                 VALUES ('run-stale', ?1, 'FAILED', 'v3', 'old failure', 'AI_EXTRACTED',
                         'now', 'now')",
                params![id],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO pipeline_job
                 (id, source, external_session_id, generation, status, last_error,
                  created_at, updated_at, session_id, pipeline_run_id)
                 VALUES ('job-old', 'codex', 'stale-errors', 1, 'FAILED', 'old job failure',
                         'now', 'now', ?1, 'run-stale')",
                params![id],
            )
            .unwrap();
        let repo = TaskCenterRepo::new(&db);
        assert_eq!(
            repo.list_recent(1).unwrap()[0].pipeline_error.as_deref(),
            Some("old failure")
        );
        db.conn()
            .execute(
                "INSERT INTO pipeline_job
                 (id, source, external_session_id, generation, status,
                  created_at, updated_at, session_id, pipeline_run_id)
                 VALUES ('job-new', 'codex', 'stale-errors', 2, 'PENDING',
                         'now', 'now', ?1, 'run-stale')",
                params![id],
            )
            .unwrap();
        let entry = repo.list_recent(1).unwrap().remove(0);
        assert_eq!(entry.job_status.as_deref(), Some("PENDING"));
        assert_eq!(entry.pipeline_status.as_deref(), Some("DISCOVERED"));
        assert!(entry.pipeline_error.is_none());
        assert!(entry.job_error.is_none());
        assert!(entry.current_stage.is_none());
        db.conn()
            .execute(
                "UPDATE pipeline_job SET status = 'RUNNING' WHERE id = 'job-new'",
                [],
            )
            .unwrap();
        let running = repo.list_recent(1).unwrap().remove(0);
        assert_eq!(running.job_status.as_deref(), Some("RUNNING"));
        assert_eq!(running.pipeline_status.as_deref(), Some("PROCESSING"));
        assert!(running.pipeline_error.is_none());
        assert!(running.current_stage.is_none());
        db.conn()
            .execute(
                "UPDATE pipeline_job SET status = 'DONE' WHERE id = 'job-new'",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "UPDATE pipeline_run SET status = 'READY', error_message = NULL
                 WHERE id = 'run-stale'",
                [],
            )
            .unwrap();
        let done = repo.list_recent(1).unwrap().remove(0);
        assert_eq!(done.job_status.as_deref(), Some("DONE"));
        assert_eq!(done.pipeline_status.as_deref(), Some("READY"));
        assert!(done.pipeline_error.is_none());
    }

    #[test]
    fn cancelling_a_queued_job_updates_projection_without_changing_pipeline_history() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let id: i64 = db
            .conn()
            .query_row(
                "INSERT INTO source_session
             (source, external_session_id, last_seen_at, created_at, updated_at)
             VALUES ('codex', 'cancel-test', 'now', 'now', 'now') RETURNING id",
                [],
                |row| row.get(0),
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO pipeline_run
             (id, session_id, status, pipeline_version, created_at, updated_at)
             VALUES ('cancel-run', ?1, 'DISCOVERED', 'v3', 'now', 'now')",
                params![id],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO pipeline_job
             (id, source, external_session_id, generation, status,
              created_at, updated_at, session_id, pipeline_run_id)
             VALUES ('cancel-job', 'codex', 'cancel-test', 1, 'PENDING',
                     'now', 'now', ?1, 'cancel-run')",
                params![id],
            )
            .unwrap();
        assert!(crate::pipeline::job_repo::PipelineJobRepo::new(&db)
            .cancel_pending_for_session(id)
            .unwrap());
        let row = TaskCenterRepo::new(&db).list_recent(1).unwrap().remove(0);
        assert_eq!(row.job_status.as_deref(), Some("CANCELLED"));
        assert_eq!(row.pipeline_status.as_deref(), Some("CANCELLED"));
        assert!(row.current_stage.is_none());
        assert_eq!(TaskCenterRepo::new(&db).stats().unwrap().cancelled, 1);
        let original_run: String = db
            .conn()
            .query_row(
                "SELECT status FROM pipeline_run WHERE id = 'cancel-run'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(original_run, "DISCOVERED");
    }

    #[test]
    fn empty_database_has_no_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        assert!(TaskCenterRepo::new(&db).list_recent(25).unwrap().is_empty());
    }

    #[test]
    fn displays_sync_failure_and_pipeline_status_independently() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let id = db
            .conn()
            .query_row(
                "INSERT INTO source_session
                 (source, external_session_id, title, last_seen_at, created_at, updated_at)
                 VALUES ('codex', 'rollout-error', 'Failed session', 'now', 'now', 'now')
                 RETURNING id",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO sync_target (session_id, sink, status, last_error)
                 VALUES (?1, 'siyuan', 'FAILED_RETRYABLE', 'remote unavailable')",
                params![id],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO pipeline_run
                 (id, session_id, pipeline_version, status, current_stage,
                  error_message, created_at, updated_at)
                 VALUES ('run-1', ?1, 'v3', 'PROCESSING', 'AI_EXTRACTED',
                         NULL, 'now', 'now')",
                params![id],
            )
            .unwrap();
        let rows = TaskCenterRepo::new(&db).list_recent(10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].sync_status.as_deref(), Some("FAILED_RETRYABLE"));
        assert_eq!(rows[0].sync_error.as_deref(), Some("remote unavailable"));
        assert_eq!(rows[0].pipeline_status.as_deref(), Some("PROCESSING"));
        assert_eq!(rows[0].current_stage.as_deref(), Some("AI_EXTRACTED"));
        assert!(rows[0].job_status.is_none());
    }

    #[test]
    fn sessions_without_sync_or_ai_still_appear() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        db.conn()
            .execute(
                "INSERT INTO source_session
             (source, external_session_id, title, last_seen_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4, ?4)",
                params!["codex", "rollout-demo", "Example", "2026-10-09"],
            )
            .unwrap();
        let rows = TaskCenterRepo::new(&db).list_recent(20).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, "codex");
        assert!(rows[0].sync_status.is_none());
        assert!(rows[0].pipeline_status.is_none());
    }
}
