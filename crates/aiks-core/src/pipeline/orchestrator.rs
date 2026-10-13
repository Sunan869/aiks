/// Pipeline Orchestrator — coordinates the multi-stage pipeline
///
/// V3 pipeline stages:
/// DISCOVERED → PARSED → NORMALIZED → CLEANED → LLM_CHUNKED
///           → AI_EXTRACTED → KNOWLEDGE_SPLIT → EMBED_CHUNKED
///           → EMBEDDED → INDEXED → READY
use std::sync::Arc;
use tracing::{info, warn};

use crate::model::pipeline::PipelineStats;
use crate::pipeline::repo::PipelineRepo;
use crate::pipeline::status::PipelineStatus;
use crate::storage::StateDb;

pub struct PipelineOrchestrator {
    db: Arc<StateDb>,
}

impl PipelineOrchestrator {
    pub fn new(db: Arc<StateDb>) -> Self {
        Self { db }
    }

    /// Enqueue a session for pipeline processing
    pub fn enqueue(&self, session_id: i64, source_hash: Option<&str>) -> anyhow::Result<String> {
        let repo = PipelineRepo::new(&self.db);
        let run_id = repo.upsert_pipeline_run(session_id, source_hash, "v3")?;
        info!(
            session_id,
            run_id = %run_id,
            "[PIPELINE] Session enqueued"
        );
        Ok(run_id)
    }

    /// Get all pipeline runs (list view)
    pub fn list_runs(&self, limit: usize) -> anyhow::Result<Vec<PipelineStatus>> {
        let repo = PipelineRepo::new(&self.db);
        repo.list_runs(limit)
    }

    /// Get a specific pipeline run with full detail
    pub fn get_run_detail(&self, run_id: &str) -> anyhow::Result<Option<PipelineStatus>> {
        let repo = PipelineRepo::new(&self.db);
        repo.get_run_detail(run_id)
    }

    /// Get pipeline processing stats
    pub fn get_stats(&self) -> anyhow::Result<PipelineStats> {
        let repo = PipelineRepo::new(&self.db);
        repo.get_stats()
    }

    /// Mark a stage as failed
    pub fn mark_failed(&self, run_id: &str, stage: &str, error: &str) -> anyhow::Result<()> {
        let repo = PipelineRepo::new(&self.db);
        repo.update_status(run_id, "FAILED", Some(stage), Some(stage), Some(error))?;
        warn!(run_id, stage, error, "[PIPELINE] Stage failed");
        Ok(())
    }

    /// Mark a pipeline run as completed (READY)
    pub fn mark_ready(&self, run_id: &str) -> anyhow::Result<()> {
        let repo = PipelineRepo::new(&self.db);
        repo.mark_finished(run_id, "READY")?;
        info!(run_id, "[PIPELINE] Run READY");
        Ok(())
    }

    /// Mark a pipeline run as RAW_ONLY (not worth extracting)
    pub fn mark_raw_only(&self, run_id: &str) -> anyhow::Result<()> {
        let repo = PipelineRepo::new(&self.db);
        repo.mark_finished(run_id, "RAW_ONLY")?;
        info!(run_id, "[PIPELINE] Run RAW_ONLY (not worth extracting)");
        Ok(())
    }
}
