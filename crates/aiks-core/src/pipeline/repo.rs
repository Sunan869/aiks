/// Pipeline repository — read/write pipeline_run and pipeline_stage_run tables
use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

use crate::model::pipeline::{PipelineRun, StageRun};
use crate::storage::StateDb;
use crate::pipeline::status::{PipelineStatus, StageStatus};

pub struct PipelineRepo<'a> {
    db: &'a StateDb,
}

impl<'a> PipelineRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    /// Create or update a pipeline run for a session
    pub fn upsert_pipeline_run(
        &self,
        session_id: i64,
        source_hash: Option<&str>,
        pipeline_version: &str,
    ) -> anyhow::Result<String> {
        let conn = self.db.conn();
        let now = Utc::now().to_rfc3339();

        // Check if run exists
        let existing: Option<String> = conn.query_row(
            "SELECT id FROM pipeline_run WHERE session_id = ?1 AND pipeline_version = ?2",
            params![session_id, pipeline_version],
            |row| row.get(0),
        ).ok();

        if let Some(id) = existing {
            conn.execute(
                "UPDATE pipeline_run SET source_hash = ?1, status = 'DISCOVERED', updated_at = ?2 WHERE id = ?3",
                params![source_hash, now, id],
            )?;
            Ok(id)
        } else {
            let id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO pipeline_run (id, session_id, status, pipeline_version, source_hash, created_at, updated_at)
                 VALUES (?1, ?2, 'DISCOVERED', ?3, ?4, ?5, ?5)",
                params![id, session_id, pipeline_version, source_hash, now],
            )?;
            Ok(id)
        }
    }

    /// Update pipeline run status and current stage
    pub fn update_status(
        &self,
        run_id: &str,
        status: &str,
        current_stage: Option<&str>,
        error_stage: Option<&str>,
        error_message: Option<&str>,
    ) -> anyhow::Result<()> {
        let conn = self.db.conn();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE pipeline_run SET status = ?1, current_stage = ?2, error_stage = ?3, error_message = ?4, updated_at = ?5
             WHERE id = ?6",
            params![status, current_stage, error_stage, error_message, now, run_id],
        )?;
        Ok(())
    }

    /// Mark pipeline run as started
    pub fn mark_started(&self, run_id: &str) -> anyhow::Result<()> {
        let conn = self.db.conn();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE pipeline_run SET started_at = ?1, status = 'PROCESSING', updated_at = ?1 WHERE id = ?2",
            params![now, run_id],
        )?;
        Ok(())
    }

    /// R13: crash recovery — reset runs stuck in PROCESSING (left behind by a
    /// dead process) back to DISCOVERED so the worker can resubmit them.
    pub fn requeue_processing_runs(&self) -> anyhow::Result<usize> {
        let conn = self.db.conn();
        let now = Utc::now().to_rfc3339();
        let n = conn.execute(
            "UPDATE pipeline_run SET status = 'DISCOVERED', updated_at = ?1
             WHERE status = 'PROCESSING' AND pipeline_version = 'v3'",
            params![now],
        )?;
        if n > 0 {
            tracing::info!("[PIPELINE] Requeued {} run(s) stuck in PROCESSING", n);
        }
        Ok(n)
    }

    /// Mark pipeline run as finished
    pub fn mark_finished(&self, run_id: &str, status: &str) -> anyhow::Result<()> {
        let conn = self.db.conn();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE pipeline_run SET finished_at = ?1, status = ?2, updated_at = ?1 WHERE id = ?3",
            params![now, status, run_id],
        )?;
        Ok(())
    }

    /// Record a stage run
    pub fn record_stage(
        &self,
        pipeline_run_id: &str,
        stage: &str,
        status: &str,
        input_count: Option<i32>,
        output_count: Option<i32>,
        latency_ms: Option<i64>,
        detail: Option<&serde_json::Value>,
        error_message: Option<&str>,
    ) -> anyhow::Result<String> {
        let conn = self.db.conn();
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let detail_json = detail.map(|d| serde_json::to_string(d).unwrap_or_default());

        conn.execute(
            "INSERT OR REPLACE INTO pipeline_stage_run
             (id, pipeline_run_id, stage, status, started_at, finished_at, input_count, output_count, latency_ms, detail_json, error_message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id, pipeline_run_id, stage, status, now,
                input_count, output_count, latency_ms,
                detail_json, error_message
            ],
        )?;
        Ok(id)
    }

    /// Mark a stage as failed and set pipeline to FAILED state
    pub fn mark_failed(
        &self,
        run_id: &str,
        stage: &str,
        error: &str,
    ) -> anyhow::Result<()> {
        self.update_status(run_id, "FAILED", Some(stage), Some(stage), Some(error))?;
        tracing::warn!(run_id, stage, error, "[PIPELINE] Stage failed");
        Ok(())
    }

    /// Get all pipeline runs with summary
    pub fn list_runs(&self, limit: usize) -> anyhow::Result<Vec<PipelineStatus>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT pr.id, pr.session_id, ss.title, ss.source, pr.status, pr.current_stage,
                    pr.pipeline_version, pr.started_at, pr.finished_at, pr.error_stage, pr.error_message
             FROM pipeline_run pr
             JOIN source_session ss ON ss.id = pr.session_id
             ORDER BY pr.updated_at DESC
             LIMIT ?1"
        )?;

        let runs = stmt.query_map(params![limit as i64], |row| {
            Ok(PipelineStatus {
                run_id: row.get(0)?,
                session_id: row.get(1)?,
                session_title: row.get(2)?,
                source: row.get(3)?,
                status: row.get(4)?,
                current_stage: row.get(5)?,
                pipeline_version: row.get(6)?,
                started_at: row.get(7)?,
                finished_at: row.get(8)?,
                error_stage: row.get(9)?,
                error_message: row.get(10)?,
                stage_runs: vec![],
                knowledge_count: 0,
            })
        })?;

        let mut result = Vec::new();
        for run in runs {
            result.push(run?);
        }
        Ok(result)
    }

    /// Get a specific pipeline run with stage detail
    pub fn get_run_detail(&self, run_id: &str) -> anyhow::Result<Option<PipelineStatus>> {
        let conn = self.db.conn();

        let run = conn.query_row(
            "SELECT pr.id, pr.session_id, ss.title, ss.source, pr.status, pr.current_stage,
                    pr.pipeline_version, pr.started_at, pr.finished_at, pr.error_stage, pr.error_message
             FROM pipeline_run pr
             JOIN source_session ss ON ss.id = pr.session_id
             WHERE pr.id = ?1",
            params![run_id],
            |row| {
                Ok(PipelineStatus {
                    run_id: row.get(0)?,
                    session_id: row.get(1)?,
                    session_title: row.get(2)?,
                    source: row.get(3)?,
                    status: row.get(4)?,
                    current_stage: row.get(5)?,
                    pipeline_version: row.get(6)?,
                    started_at: row.get(7)?,
                    finished_at: row.get(8)?,
                    error_stage: row.get(9)?,
                    error_message: row.get(10)?,
                    stage_runs: vec![],
                    knowledge_count: 0,
                })
            },
        );

        match run {
            Ok(mut status) => {
                // Load stage runs
                let mut stage_stmt = conn.prepare(
                    "SELECT stage, status, input_count, output_count, latency_ms, detail_json, error_message
                     FROM pipeline_stage_run WHERE pipeline_run_id = ?1 ORDER BY rowid"
                )?;
                let stages = stage_stmt.query_map(params![run_id], |row| {
                    let detail_str: Option<String> = row.get(5)?;
                    Ok(StageStatus {
                        stage: row.get(0)?,
                        status: row.get(1)?,
                        input_count: row.get(2)?,
                        output_count: row.get(3)?,
                        latency_ms: row.get(4)?,
                        error_message: row.get(6)?,
                        detail: detail_str
                            .as_deref()
                            .and_then(|s| serde_json::from_str(s).ok()),
                    })
                })?;
                for s in stages {
                    status.stage_runs.push(s?);
                }

                // Count knowledge items
                let kc: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM knowledge_item WHERE source_session_id = ?1",
                    params![status.session_id],
                    |row| row.get(0),
                ).unwrap_or(0);
                status.knowledge_count = kc as usize;

                Ok(Some(status))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get pipeline stats summary
    pub fn get_stats(&self) -> anyhow::Result<crate::model::pipeline::PipelineStats> {
        let conn = self.db.conn();

        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pipeline_run", [], |row| row.get(0)
        ).unwrap_or(0);

        let processing: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pipeline_run WHERE status = 'PROCESSING'", [], |row| row.get(0)
        ).unwrap_or(0);

        let ready: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pipeline_run WHERE status = 'READY'", [], |row| row.get(0)
        ).unwrap_or(0);

        let raw_only: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pipeline_run WHERE status = 'RAW_ONLY'", [], |row| row.get(0)
        ).unwrap_or(0);

        let failed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pipeline_run WHERE status = 'FAILED'", [], |row| row.get(0)
        ).unwrap_or(0);

        let knowledge_items: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_item", [], |row| row.get(0)
        ).unwrap_or(0);

        let knowledge_chunks: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_chunk", [], |row| row.get(0)
        ).unwrap_or(0);

        let embeddings: i64 = conn.query_row(
            "SELECT COUNT(*) FROM embedding_record", [], |row| row.get(0)
        ).unwrap_or(0);

        Ok(crate::model::pipeline::PipelineStats {
            total: total as usize,
            processing: processing as usize,
            ready: ready as usize,
            raw_only: raw_only as usize,
            failed: failed as usize,
            knowledge_items: knowledge_items as usize,
            knowledge_chunks: knowledge_chunks as usize,
            embeddings: embeddings as usize,
        })
    }
}
