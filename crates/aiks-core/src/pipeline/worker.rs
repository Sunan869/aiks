/// V3 Pipeline Worker — background task that processes sessions through stages
///
/// Stages: DISCOVERED → PARSED → CLEANED → LLM_CHUNKED → AI_EXTRACTED
///       → EMBED_CHUNKED → EMBEDDED → INDEXED → READY
///
/// Runs automatically when new sessions are discovered via sync.
use std::sync::Arc;

use tokio::sync::{mpsc, Semaphore};
use tracing::{error, info, warn};

use crate::ai::config::AiModelConfig;
use crate::pipeline::ai_stage::AiStage;
use crate::pipeline::cleaner::clean_messages;
use crate::pipeline::embedding_client::EmbeddingConfig;
use crate::pipeline::embedding_stage::EmbeddingStage;
use crate::pipeline::repo::PipelineRepo;
use crate::pipeline::session_chunker::{chunk_for_llm, save_chunks};
use crate::providers::ProviderRegistry;
use crate::storage::StateDb;

#[derive(Debug, Clone)]
pub struct PipelineJob {
    pub pipeline_run_id: String,
    pub session_id: i64,
    pub session_external_id: String,
    pub source: String,
    pub session_title: Option<String>,
    pub project_name: Option<String>,
}

pub struct PipelineWorker {
    tx: mpsc::UnboundedSender<PipelineJob>,
}

impl PipelineWorker {
    /// Start the background pipeline worker with default concurrency (max_concurrent from ai_config)
    pub fn start(
        db: Arc<StateDb>,
        registry: Arc<ProviderRegistry>,
        ai_config: AiModelConfig,
        embedding_config: EmbeddingConfig,
    ) -> Self {
        let max_concurrent = ai_config.max_concurrent.max(1);
        Self::start_with_limit(db, registry, ai_config, embedding_config, max_concurrent)
    }

    /// Start with an explicit concurrency limit (B13)
    pub fn start_with_limit(
        db: Arc<StateDb>,
        registry: Arc<ProviderRegistry>,
        ai_config: AiModelConfig,
        embedding_config: EmbeddingConfig,
        max_concurrent: usize,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let semaphore = Arc::new(Semaphore::new(max_concurrent.max(1)));

        tokio::spawn(async move {
            run_worker(rx, db, registry, ai_config, embedding_config, semaphore).await;
        });

        Self { tx }
    }

    /// Submit a job to the pipeline
    pub fn submit(&self, job: PipelineJob) {
        let _ = self.tx.send(job);
    }
}

/// R13: crash recovery — requeue pipeline runs left in PROCESSING by a
/// previous process and submit them to the current worker. Runs found in
/// DISCOVERED (never processed) are submitted as well.
pub fn recover_interrupted_runs(
    db: &Arc<StateDb>,
    worker: &PipelineWorker,
) -> usize {
    let repo = PipelineRepo::new(db);

    // Step 1: requeue runs stuck in PROCESSING (previous process died mid-run)
    let _ = repo.requeue_processing_runs();

    // Step 2: submit every run that still needs processing
    let rows: Vec<(String, i64, String, String, Option<String>, Option<String>)> = {
        let conn = db.conn();
        let result = conn
            .prepare(
                "SELECT pr.id, pr.session_id, ss.source, ss.external_session_id, ss.title, ss.project_name
                 FROM pipeline_run pr
                 JOIN source_session ss ON ss.id = pr.session_id
                 WHERE pr.pipeline_version = 'v3'
                   AND pr.status IN ('DISCOVERED', 'FAILED')
                 ORDER BY pr.updated_at ASC",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
            });
        match result {
            Ok(rows) => rows,
            Err(e) => {
                warn!(error = %e, "[PIPELINE] Recovery query failed");
                return 0;
            }
        }
    };

    let count = rows.len();
    for (pipeline_run_id, session_id, source, session_external_id, session_title, project_name) in rows {
        worker.submit(PipelineJob {
            pipeline_run_id,
            session_id,
            session_external_id,
            source,
            session_title,
            project_name,
        });
    }
    if count > 0 {
        info!("[PIPELINE] Recovered {} interrupted/pending run(s)", count);
    }
    count
}

async fn run_worker(
    mut rx: mpsc::UnboundedReceiver<PipelineJob>,
    db: Arc<StateDb>,
    registry: Arc<ProviderRegistry>,
    ai_config: AiModelConfig,
    embedding_config: EmbeddingConfig,
    semaphore: Arc<Semaphore>,  // B13: concurrency limit
) {
    info!("[PIPELINE] Worker started");

    // R13: per-session in-flight dedup — a session already being processed is
    // not re-spawned (the run row stays DISCOVERED and the periodic backfill
    // picks it up later). Prevents duplicate work and racing writes.
    let in_flight: Arc<tokio::sync::Mutex<std::collections::HashSet<i64>>> =
        Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new()));

    while let Some(job) = rx.recv().await {
        // R13: dedup — skip if this session is already queued/running.
        {
            let mut set = in_flight.lock().await;
            if !set.insert(job.session_id) {
                warn!(
                    session_id = job.session_id,
                    run_id = %job.pipeline_run_id,
                    "[PIPELINE] Session already in flight — job skipped (will be picked up by backfill)"
                );
                continue;
            }
        }

        let db = Arc::clone(&db);
        let registry = Arc::clone(&registry);
        let ai = ai_config.clone();
        let emb = embedding_config.clone();
        let sem = Arc::clone(&semaphore);
        let inflight = Arc::clone(&in_flight);

        // B13 + R13: acquire the permit BEFORE spawning — at most
        // `max_concurrent` tasks exist at any moment, so the backlog cannot
        // pile up as unbounded waiting tasks.
        let permit = match sem.acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                inflight.lock().await.remove(&job.session_id);
                break;
            }
        };

        tokio::spawn(async move {
            if let Err(e) = run_pipeline(&db, &registry, &ai, &emb, &job).await {
                error!(
                    run_id = %job.pipeline_run_id,
                    session_id = job.session_id,
                    error = %e,
                    "[PIPELINE] Job failed"
                );
                // B12 + R13: safety-net mark_failed — but do NOT overwrite the
                // accurate failure stage already recorded by run_pipeline.
                let repo = PipelineRepo::new(&db);
                let already_failed = repo
                    .get_run_detail(&job.pipeline_run_id)
                    .ok()
                    .flatten()
                    .map(|d| d.status == "FAILED")
                    .unwrap_or(false);
                if !already_failed {
                    let stage = extract_failed_stage(&e.to_string());
                    let _ = repo.mark_failed(&job.pipeline_run_id, stage, &e.to_string());
                }
            }
            // Release: remove from in-flight set + drop the semaphore permit.
            inflight.lock().await.remove(&job.session_id);
            drop(permit);
        });
    }

    info!("[PIPELINE] Worker stopped");
}

/// R13: map a run_pipeline error message back to its pipeline stage.
/// run_pipeline prefixes stage errors with the stage name.
fn extract_failed_stage(error: &str) -> &'static str {
    const STAGES: [&str; 8] = [
        "PARSED",
        "CLEANED",
        "LLM_CHUNKED",
        "AI_EXTRACTED",
        "EMBED_CHUNKED",
        "EMBEDDED",
        "INDEXED",
        "DISCOVERED",
    ];
    for stage in STAGES {
        if error.starts_with(stage) || error.contains(&format!("{}: ", stage)) {
            return stage;
        }
    }
    "UNKNOWN"
}

async fn run_pipeline(
    db: &StateDb,
    registry: &ProviderRegistry,
    ai_config: &AiModelConfig,
    embedding_config: &EmbeddingConfig,
    job: &PipelineJob,
) -> anyhow::Result<()> {
    let repo = PipelineRepo::new(db);
    let run_id = &job.pipeline_run_id;

    info!(
        session_id = job.session_id,
        source = %job.source,
        run_id = %run_id,
        "[PIPELINE] Starting job"
    );

    repo.mark_started(run_id)?;

    // B12: Macro to fail a stage and return the error
    macro_rules! fail_stage {
        ($stage:expr, $err:expr) => {{
            let err_str = $err.to_string();
            let _ = repo.mark_failed(run_id, $stage, &err_str);
            return Err(anyhow::anyhow!("{}: {}", $stage, err_str));
        }};
    }

    // ── Stage 1: PARSE ────────────────────────────────────────────────────────
    repo.update_status(run_id, "PROCESSING", Some("PARSED"), None, None)?;

    // Find source kind
    let source_kind = match crate::model::SourceKind::from_str(&job.source) {
        Some(k) => k,
        None => fail_stage!("PARSED", format!("Unknown source: {}", job.source)),
    };

    // Load session from provider
    let provider = match registry.get(source_kind) {
        Some(p) => p,
        None => fail_stage!("PARSED", format!("Provider not found for {:?}", source_kind)),
    };

    let summaries = match provider.discover_sessions().await {
        Ok(s) => s,
        Err(e) => fail_stage!("PARSED", format!("Discover failed: {}", e)),
    };

    let summary = match summaries.into_iter().find(|s| s.external_session_id == job.session_external_id) {
        Some(s) => s,
        None => fail_stage!("PARSED", format!("Session not found: {}", job.session_external_id)),
    };

    let session = match provider.load_session(&summary).await {
        Ok(s) => s,
        Err(e) => fail_stage!("PARSED", format!("Load session failed: {}", e)),
    };

    repo.record_stage(
        run_id, "PARSED", "SUCCESS",
        None, Some(session.messages.len() as i32), None,
        Some(&serde_json::json!({"message_count": session.messages.len()})),
        None,
    )?;
    info!("[PARSE] {} messages loaded", session.messages.len());

    // ── Stage 2: CLEAN ────────────────────────────────────────────────────────
    repo.update_status(run_id, "PROCESSING", Some("CLEANED"), None, None)?;

    let clean_result = clean_messages(session.messages.clone());

    repo.record_stage(
        run_id, "CLEANED", "SUCCESS",
        Some(clean_result.original_count as i32),
        Some(clean_result.cleaned_count as i32),
        None,
        Some(&serde_json::json!({
            "original": clean_result.original_count,
            "cleaned": clean_result.cleaned_count,
            "removed": clean_result.removed_count
        })),
        None,
    )?;
    info!("[CLEAN] {} → {} messages", clean_result.original_count, clean_result.cleaned_count);

    if clean_result.cleaned_count == 0 {
        repo.mark_finished(run_id, "RAW_ONLY")?;
        return Ok(());
    }

    // ── Stage 3: LLM CHUNK ───────────────────────────────────────────────────
    repo.update_status(run_id, "PROCESSING", Some("LLM_CHUNKED"), None, None)?;

    let chunk_result = chunk_for_llm(job.session_id, &clean_result.messages);
    save_chunks(db, &chunk_result.chunks)?;

    repo.record_stage(
        run_id, "LLM_CHUNKED", "SUCCESS",
        Some(clean_result.cleaned_count as i32),
        Some(chunk_result.chunks.len() as i32),
        None,
        Some(&serde_json::json!({
            "chunks": chunk_result.chunks.len(),
            "messages_per_chunk": clean_result.cleaned_count / chunk_result.chunks.len().max(1)
        })),
        None,
    )?;
    info!("[CHUNK] {} LLM chunks", chunk_result.chunks.len());

    // ── Stage 4: AI EXTRACT ──────────────────────────────────────────────────
    if !ai_config.enabled {
        info!("[AI] AI disabled, marking RAW_ONLY");
        repo.mark_finished(run_id, "RAW_ONLY")?;
        return Ok(());
    }

    repo.update_status(run_id, "PROCESSING", Some("AI_EXTRACTED"), None, None)?;

    let ai_stage = match AiStage::new(ai_config.clone()) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("AI init failed: {}", e);
            repo.mark_failed(run_id, "AI_EXTRACTED", &msg)?;
            return Err(anyhow::anyhow!(msg));
        }
    };

    let item_count = match ai_stage.run(
        db,
        run_id,
        job.session_id,
        job.session_title.as_deref(),
        job.project_name.as_deref(),
    ).await {
        Ok(n) => n,
        Err(e) => {
            let msg = format!("AI extraction failed: {}", e);
            warn!(run_id, error = %e, "[AI] Extraction failed");
            repo.mark_failed(run_id, "AI_EXTRACTED", &msg)?;
            return Err(anyhow::anyhow!(msg));
        }
    };

    if item_count == 0 {
        repo.mark_finished(run_id, "RAW_ONLY")?;
        return Ok(());
    }

    // ── Stage 5: KNOWLEDGE SPLIT (EMBED_CHUNK) ────────────────────────────────
    repo.update_status(run_id, "PROCESSING", Some("EMBED_CHUNKED"), None, None)?;

    let target_tokens = if embedding_config.enabled { embedding_config.chunk_target_tokens } else { 800 };
    let overlap_tokens = if embedding_config.enabled { embedding_config.chunk_overlap_tokens } else { 120 };

    if let Err(e) = EmbeddingStage::chunk_knowledge(db, run_id, job.session_id, target_tokens, overlap_tokens) {
        warn!(error = %e, "[EMBED_CHUNK] Failed, continuing without chunks");
    }

    // ── Stage 6: EMBED ────────────────────────────────────────────────────────
    if embedding_config.enabled && !embedding_config.base_url.is_empty() {
        repo.update_status(run_id, "PROCESSING", Some("EMBEDDED"), None, None)?;

        match EmbeddingStage::new(embedding_config.clone()) {
            Ok(stage) => {
                if let Err(e) = stage.embed_knowledge(db, run_id, job.session_id).await {
                    warn!(error = %e, "[EMBED] Embedding failed, marking INDEXED anyway");
                }
            }
            Err(e) => warn!(error = %e, "[EMBED] Stage init failed"),
        }

        // Index stage (placeholder — vector index is sqlite BLOB)
        repo.record_stage(
            run_id, "INDEXED", "SUCCESS",
            None, None, None,
            Some(&serde_json::json!({"type": "sqlite-blob"})),
            None,
        )?;
    } else {
        // Skip embedding stages
        repo.record_stage(run_id, "EMBEDDED", "SKIPPED", None, None, None,
            Some(&serde_json::json!({"reason": "embedding not configured"})), None)?;
        repo.record_stage(run_id, "INDEXED", "SKIPPED", None, None, None,
            Some(&serde_json::json!({"reason": "embedding not configured"})), None)?;
    }

    // ── DONE ──────────────────────────────────────────────────────────────────
    repo.mark_finished(run_id, "READY")?;
    info!(
        session_id = job.session_id,
        run_id = %run_id,
        items = item_count,
        "[PIPELINE] Job READY"
    );

    Ok(())
}
