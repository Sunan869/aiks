use chrono::{Duration, Utc};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

use crate::storage::StateDb;

use super::worker::PipelineJob;

const MAX_ATTEMPTS: i64 = 3;
const LEASE_SECONDS: i64 = 30;
const RETRY_BACKOFF_SECONDS: [i64; 2] = [5, 30];

#[derive(Debug, Clone)]
pub struct ClaimedPipelineJob {
    pub durable_job_id: String,
    pub attempt: i64,
    pub job: PipelineJob,
}

#[derive(Debug, Clone)]
pub struct EnqueueResult {
    pub durable_job_id: String,
    pub inserted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureDisposition {
    Retry,
    Terminal,
}

pub struct PipelineJobRepo<'a> {
    db: &'a StateDb,
}

impl<'a> PipelineJobRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    /// Persist a logical pipeline job before waking the worker.
    ///
    /// Same source/session/hash submissions collapse while one is active.
    /// A newer hash supersedes older *pending* generations but never cancels a
    /// running generation; the new generation waits until the running one exits.
    pub fn enqueue(&self, job: &PipelineJob) -> anyhow::Result<EnqueueResult> {
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = Self::enqueue_in_tx(&tx, job)?;
        tx.commit()?;
        Ok(result)
    }

    /// Transactional primitive shared with atomic ingestion. Does not commit.
    pub fn enqueue_in_tx(
        tx: &Transaction<'_>,
        job: &PipelineJob,
    ) -> anyhow::Result<EnqueueResult> {
        Self::enqueue_with_identity_in_tx(tx, job, false)
    }

    /// A snapshot revision cannot reuse an older active job merely because
    /// its content hash returned to a previous value (A -> B -> A).
    pub fn enqueue_snapshot_in_tx(
        tx: &Transaction<'_>,
        job: &PipelineJob,
    ) -> anyhow::Result<EnqueueResult> {
        Self::enqueue_with_identity_in_tx(tx, job, true)
    }

    fn enqueue_with_identity_in_tx(
        tx: &Transaction<'_>,
        job: &PipelineJob,
        exact_run: bool,
    ) -> anyhow::Result<EnqueueResult> {
        let now = Utc::now().to_rfc3339();
        // Derive canonical identity rather than trusting caller duplicates.
        let (canonical_source, canonical_external_id, source_hash): (String, String, String) = tx
            .query_row(
                "SELECT source, external_session_id, COALESCE(content_hash, '')
                 FROM source_session WHERE id = ?1",
                params![job.session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("source_session not found: {}", job.session_id))?;
        let run_exists: i64 = tx.query_row(
            "SELECT COUNT(*) FROM pipeline_run WHERE id = ?1 AND session_id = ?2",
            params![job.pipeline_run_id, job.session_id],
            |row| row.get(0),
        )?;
        if run_exists != 1 {
            anyhow::bail!(
                "pipeline_run {} does not belong to session {}",
                job.pipeline_run_id,
                job.session_id
            );
        }
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM pipeline_job
                 WHERE session_id = ?1 AND source_hash = ?2
                   AND status IN ('PENDING', 'RUNNING')
                   AND (?3 = 0 OR pipeline_run_id = ?4)
                 ORDER BY generation DESC LIMIT 1",
                params![job.session_id, source_hash, exact_run, job.pipeline_run_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(durable_job_id) = existing {
            return Ok(EnqueueResult {
                durable_job_id,
                inserted: false,
            });
        }
        let generation: i64 = tx.query_row(
            "SELECT COALESCE(MAX(generation), 0) + 1 FROM pipeline_job
             WHERE session_id = ?1",
            params![job.session_id],
            |row| row.get(0),
        )?;
        // claim_next prevents simultaneous generations for the same session.
        tx.execute(
            "UPDATE pipeline_job
             SET status = 'SUPERSEDED', updated_at = ?1
             WHERE session_id = ?2 AND status = 'PENDING'",
            params![now, job.session_id],
        )?;
        let durable_job_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO pipeline_job
             (id, source, external_session_id, source_hash, generation, status,
              attempt, available_at, lease_until, last_error, created_at, updated_at,
              session_id, pipeline_run_id)
             VALUES (?1, ?2, ?3, ?4, ?5, 'PENDING', 0, ?6, NULL, NULL, ?6, ?6, ?7, ?8)",
            params![
                durable_job_id,
                canonical_source,
                canonical_external_id,
                source_hash,
                generation,
                now,
                job.session_id,
                job.pipeline_run_id
            ],
        )?;
        Ok(EnqueueResult {
            durable_job_id,
            inserted: true,
        })
    }

    /// Atomically claim one available job and attach its canonical identity.
    pub fn claim_next(&self) -> anyhow::Result<Option<ClaimedPipelineJob>> {
        let now_dt = Utc::now();
        let now = now_dt.to_rfc3339();
        let lease_until = (now_dt + Duration::seconds(LEASE_SECONDS)).to_rfc3339();
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE pipeline_job
             SET status = 'PENDING', lease_until = NULL, available_at = ?1, updated_at = ?1
             WHERE status = 'RUNNING' AND lease_until IS NOT NULL AND lease_until <= ?1",
            params![now],
        )?;
        let row = tx
            .query_row(
                "SELECT pj.id, pj.attempt,
                        pr.id, ss.id, ss.external_session_id, ss.source, ss.title, ss.project_name
                 FROM pipeline_job pj
                 JOIN source_session ss ON ss.id = pj.session_id
                 JOIN pipeline_run pr
                   ON pr.id = pj.pipeline_run_id AND pr.session_id = ss.id
                 WHERE pj.status = 'PENDING'
                   AND pj.session_id IS NOT NULL
                   AND pj.pipeline_run_id IS NOT NULL
                   AND (pj.available_at IS NULL OR pj.available_at <= ?1)
                   AND NOT EXISTS (
                       SELECT 1 FROM pipeline_job running
                        WHERE running.session_id = pj.session_id
                          AND running.status = 'RUNNING'
                   )
                 ORDER BY pj.created_at ASC, pj.generation ASC
                 LIMIT 1",
                params![now],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            durable_job_id,
            previous_attempt,
            pipeline_run_id,
            session_id,
            session_external_id,
            source,
            session_title,
            project_name,
        )) = row
        else {
            tx.commit()?;
            return Ok(None);
        };
        let attempt = previous_attempt + 1;
        let changed = tx.execute(
            "UPDATE pipeline_job
             SET status = 'RUNNING', attempt = ?1, lease_until = ?2, updated_at = ?3
             WHERE id = ?4 AND status = 'PENDING'",
            params![attempt, lease_until, now, durable_job_id],
        )?;
        if changed != 1 {
            tx.commit()?;
            return Ok(None);
        }
        tx.commit()?;
        Ok(Some(ClaimedPipelineJob {
            durable_job_id,
            attempt,
            job: PipelineJob {
                pipeline_run_id,
                session_id,
                session_external_id,
                source,
                session_title,
                project_name,
            },
        }))
    }

    pub fn renew_lease(&self, durable_job_id: &str) -> anyhow::Result<bool> {
        let now_dt = Utc::now();
        let lease_until = (now_dt + Duration::seconds(LEASE_SECONDS)).to_rfc3339();
        let now = now_dt.to_rfc3339();
        let conn = self.db.conn();
        let changed = conn.execute(
            "UPDATE pipeline_job SET lease_until = ?1, updated_at = ?2
             WHERE id = ?3 AND status = 'RUNNING'",
            params![lease_until, now, durable_job_id],
        )?;
        Ok(changed == 1)
    }

    pub fn mark_succeeded(&self, durable_job_id: &str) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn();
        conn.execute(
            "UPDATE pipeline_job
             SET status = 'DONE', lease_until = NULL, available_at = NULL,
                 last_error = NULL, updated_at = ?1
             WHERE id = ?2",
            params![now, durable_job_id],
        )?;
        Ok(())
    }

    pub fn mark_failed(
        &self,
        durable_job_id: &str,
        error: &str,
    ) -> anyhow::Result<FailureDisposition> {
        let now_dt = Utc::now();
        let now = now_dt.to_rfc3339();
        let conn = self.db.conn();
        let attempt: i64 = conn.query_row(
            "SELECT attempt FROM pipeline_job WHERE id = ?1",
            params![durable_job_id],
            |row| row.get(0),
        )?;
        if attempt >= MAX_ATTEMPTS {
            conn.execute(
                "UPDATE pipeline_job
                 SET status = 'FAILED', lease_until = NULL, available_at = NULL,
                     last_error = ?1, updated_at = ?2
                 WHERE id = ?3",
                params![error, now, durable_job_id],
            )?;
            return Ok(FailureDisposition::Terminal);
        }
        let backoff_index =
            (attempt.saturating_sub(1) as usize).min(RETRY_BACKOFF_SECONDS.len().saturating_sub(1));
        let available_at =
            (now_dt + Duration::seconds(RETRY_BACKOFF_SECONDS[backoff_index])).to_rfc3339();
        conn.execute(
            "UPDATE pipeline_job
             SET status = 'PENDING', lease_until = NULL, available_at = ?1,
                 last_error = ?2, updated_at = ?3
             WHERE id = ?4",
            params![available_at, error, now, durable_job_id],
        )?;
        Ok(FailureDisposition::Retry)
    }

    pub fn has_any_job(&self, source: &str, external_session_id: &str) -> anyhow::Result<bool> {
        let conn = self.db.conn();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pipeline_job WHERE source = ?1 AND external_session_id = ?2",
            params![source, external_session_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn recover_expired_leases(&self) -> anyhow::Result<usize> {
        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn();
        let changed = conn.execute(
            "UPDATE pipeline_job
             SET status = 'PENDING', lease_until = NULL, available_at = ?1, updated_at = ?1
             WHERE status = 'RUNNING' AND lease_until IS NOT NULL AND lease_until <= ?1",
            params![now],
        )?;
        Ok(changed)
    }
}
