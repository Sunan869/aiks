// CI lint baseline: pre-existing Clippy debt; remove allowances incrementally.
#![allow(clippy::type_complexity)]

/// V3 Pipeline Worker — background task that processes sessions through stages
///
/// Stages: DISCOVERED → PARSED → CLEANED → LLM_CHUNKED → AI_EXTRACTED
///       → EMBED_CHUNKED → EMBEDDED → INDEXED → READY
///
/// Runs automatically when new sessions are discovered via sync.
use std::{collections::HashMap, sync::Arc, time::Duration};

use tokio::sync::{mpsc, watch, Mutex, Semaphore};
use tokio::task::{JoinHandle, JoinSet};

use super::input::{PipelineInputSource, SnapshotInput};
use tracing::{error, info, warn};

use crate::ai::{config::AiModelConfig, ModelService};
use crate::config::ContentConfig;
use crate::indexing::{SessionIndexInput, SessionIndexService};
use crate::model::SourceKind;
use crate::pipeline::ai_stage::AiStage;
use crate::pipeline::cleaner::clean_messages;
use crate::pipeline::embedding_client::EmbeddingConfig;
use crate::pipeline::job_repo::{FailureDisposition, PipelineJobRepo};
use crate::pipeline::repo::PipelineRepo;
use crate::pipeline::session_chunker::{chunk_for_llm, save_chunks_guarded};
use crate::providers::{ProviderRegistry, SessionProvider, SessionSummary};
use crate::renderer::MarkdownRenderer;
use crate::service::SupersededRevision;
use crate::storage::StateDb;
use anyhow::Context;

#[derive(Debug, Clone)]
pub struct PipelineJob {
    pub pipeline_run_id: String,
    pub session_id: i64,
    pub session_external_id: String,
    pub source: String,
    pub session_title: Option<String>,
    pub project_name: Option<String>,
}

type SessionSummaryMap = HashMap<String, SessionSummary>;
type SourceSnapshot = Arc<Mutex<Option<SessionSummaryMap>>>;

/// Shared discovery snapshots for one active durable-queue drain cycle.
///
/// A source gets its own mutex so Claude/Codex/Gemini/OpenCode discovery can
/// still proceed independently. Same-source jobs serialize only while the
/// snapshot is first populated or refreshed after an ID miss.
#[derive(Default)]
struct ProviderDiscoveryCache {
    sources: Mutex<HashMap<SourceKind, SourceSnapshot>>,
}

impl ProviderDiscoveryCache {
    async fn slot(&self, source: SourceKind) -> SourceSnapshot {
        let mut sources = self.sources.lock().await;
        Arc::clone(
            sources
                .entry(source)
                .or_insert_with(|| Arc::new(Mutex::new(None))),
        )
    }

    async fn clear(&self) {
        self.sources.lock().await.clear();
    }
}

async fn resolve_session_summary(
    provider: &dyn SessionProvider,
    cache: &ProviderDiscoveryCache,
    source: SourceKind,
    external_session_id: &str,
) -> anyhow::Result<SessionSummary> {
    let slot = cache.slot(source).await;
    let mut snapshot = slot.lock().await;

    if let Some(existing) = snapshot.as_ref() {
        if let Some(summary) = existing.get(external_session_id) {
            return Ok(summary.clone());
        }
    }

    // No snapshot yet, or the requested ID was not in the current snapshot.
    // Refresh exactly once while holding this source's lock so concurrent jobs
    // do not all perform the same full provider scan.
    let report = provider.discover_report().await?;
    if !report.complete {
        warn!(
            source = source.as_str(),
            diagnostics = report.diagnostics.len(),
            "Incomplete provider snapshot; preserving valid pipeline sessions"
        );
    }
    let discovered = report.sessions;
    let refreshed: SessionSummaryMap = discovered
        .into_iter()
        .map(|summary| (summary.external_session_id.clone(), summary))
        .collect();
    let resolved = refreshed.get(external_session_id).cloned();
    *snapshot = Some(refreshed);

    resolved.ok_or_else(|| anyhow::anyhow!("Session not found: {}", external_session_id))
}

/// Durable pipeline worker.
///
/// Job payloads live in SQLite (`pipeline_job`). The in-memory channel is only
/// a capacity-1 wake-up signal, so producers cannot create an unbounded memory
/// backlog. A periodic poll guarantees persisted work is still discovered if a
/// wake signal is coalesced or lost during restart.
pub struct PipelineWorker {
    db: Arc<StateDb>,
    wake_tx: mpsc::Sender<()>,
    snapshot_only: bool,
    stop: watch::Sender<Option<tokio::time::Instant>>,
    supervisor: Mutex<Option<JoinHandle<anyhow::Result<()>>>>,
}

struct WorkerControl {
    wake_tx: mpsc::Sender<()>,
    semaphore: Arc<Semaphore>,
    stop_rx: watch::Receiver<Option<tokio::time::Instant>>,
}

impl PipelineWorker {
    pub fn start(
        db: Arc<StateDb>,
        registry: Arc<ProviderRegistry>,
        ai_config: AiModelConfig,
        embedding_config: EmbeddingConfig,
    ) -> Self {
        let limit = ai_config.max_concurrent.max(1);
        Self::start_with_limit(db, registry, ai_config, embedding_config, limit)
    }

    pub fn start_with_limit(
        db: Arc<StateDb>,
        registry: Arc<ProviderRegistry>,
        ai_config: AiModelConfig,
        embedding_config: EmbeddingConfig,
        max_concurrent: usize,
    ) -> Self {
        Self::start_with_input(
            db,
            PipelineInputSource::LegacyProviders(registry),
            ai_config,
            embedding_config,
            max_concurrent,
        )
    }

    /// Service mode has no ProviderRegistry and cannot scan employee files.
    pub fn start_from_snapshots(
        db: Arc<StateDb>,
        ai_config: AiModelConfig,
        embedding_config: EmbeddingConfig,
    ) -> Self {
        let limit = ai_config.max_concurrent.max(1);
        Self::start_with_input(
            db,
            PipelineInputSource::PersistedSnapshots,
            ai_config,
            embedding_config,
            limit,
        )
    }

    fn start_with_input(
        db: Arc<StateDb>,
        input: PipelineInputSource,
        ai_config: AiModelConfig,
        embedding_config: EmbeddingConfig,
        max_concurrent: usize,
    ) -> Self {
        let (wake_tx, wake_rx) = mpsc::channel(1);
        let (stop, stop_rx) = watch::channel(None);
        let control = WorkerControl {
            wake_tx: wake_tx.clone(),
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
            stop_rx,
        };
        let snapshot_only = input.is_snapshot();
        let supervisor = tokio::spawn(run_worker(
            wake_rx,
            db.clone(),
            input,
            ai_config,
            embedding_config,
            control,
        ));
        let worker = Self {
            db,
            wake_tx,
            snapshot_only,
            stop,
            supervisor: Mutex::new(Some(supervisor)),
        };
        worker.wake();
        worker
    }

    pub fn wake(&self) {
        let _ = self.wake_tx.try_send(());
    }

    /// Legacy submit remains compatible; service ingestion commits its own
    /// snapshot/run/job/receipt transaction before waking this worker.
    pub fn submit(&self, job: PipelineJob) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.snapshot_only,
            "Snapshot jobs require atomic ingestion"
        );
        PipelineJobRepo::new(&self.db).enqueue(&job)?;
        self.wake();
        Ok(())
    }

    /// Stop claiming work, drain tasks until the deadline, then cancel and join
    /// any remaining children. Cancelling this future does not detach the handle.
    pub async fn shutdown(&self, grace: Duration) -> anyhow::Result<()> {
        let deadline = tokio::time::Instant::now() + grace;
        self.stop.send_if_modified(|current| {
            if current.is_none() {
                *current = Some(deadline);
                true
            } else {
                false
            }
        });
        let mut guard = self.supervisor.lock().await;
        let Some(handle) = guard.as_mut() else {
            return Ok(());
        };
        let result = handle.await;
        *guard = None;
        result?
    }
}

impl Drop for PipelineWorker {
    fn drop(&mut self) {
        self.stop.send_replace(Some(tokio::time::Instant::now()));
    }
}

/// Seed the durable queue for legacy pipeline runs that pre-date pipeline_job,
/// and reset interrupted pipeline observability. Existing durable rows — even
/// terminal failures — are not silently resurrected on every restart.
pub fn recover_interrupted_runs(db: &Arc<StateDb>, worker: &PipelineWorker) -> usize {
    let pipeline_repo = PipelineRepo::new(db);
    let job_repo = PipelineJobRepo::new(db);

    let _ = pipeline_repo.requeue_processing_runs();
    let _ = job_repo.recover_expired_leases();

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

    let mut seeded = 0;
    for (pipeline_run_id, session_id, source, session_external_id, session_title, project_name) in
        rows
    {
        match job_repo.has_any_job(&source, &session_external_id) {
            Ok(true) => continue,
            Err(e) => {
                warn!(source, session_external_id, error = %e, "[PIPELINE] Durable recovery lookup failed");
                continue;
            }
            Ok(false) => {}
        }

        if let Err(e) = worker.submit(PipelineJob {
            pipeline_run_id,
            session_id,
            session_external_id,
            source,
            session_title,
            project_name,
        }) {
            warn!(error = %e, "[PIPELINE] Failed to seed durable recovery job");
        } else {
            seeded += 1;
        }
    }

    if seeded > 0 {
        info!(
            "[PIPELINE] Seeded {} legacy run(s) into durable queue",
            seeded
        );
    }
    seeded
}

async fn run_worker(
    mut wake_rx: mpsc::Receiver<()>,
    db: Arc<StateDb>,
    input: PipelineInputSource,
    ai_config: AiModelConfig,
    embedding_config: EmbeddingConfig,
    control: WorkerControl,
) -> anyhow::Result<()> {
    let WorkerControl {
        wake_tx,
        semaphore,
        mut stop_rx,
    } = control;
    let mut tasks = JoinSet::new();
    info!("[PIPELINE] Durable worker started");
    let discovery_cache = Arc::new(ProviderDiscoveryCache::default());
    let mut poll = tokio::time::interval(Duration::from_millis(500));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    'supervisor: loop {
        if stop_rx.borrow().is_some() {
            break;
        }
        tokio::select! {
            biased;
            _ = stop_rx.changed() => break,
            completed = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(Err(error)) = completed {
                    warn!(error = %error, "[PIPELINE] Task stopped unexpectedly");
                }
            }
            signal = wake_rx.recv() => {
                if signal.is_none() {
                    break;
                }
            }
            _ = poll.tick() => {}
        }

        loop {
            if stop_rx.borrow().is_some() {
                break 'supervisor;
            }
            let permit = match Arc::clone(&semaphore).try_acquire_owned() {
                Ok(permit) => permit,
                Err(tokio::sync::TryAcquireError::NoPermits) => break,
                Err(tokio::sync::TryAcquireError::Closed) => break 'supervisor,
            };

            let repo = PipelineJobRepo::new(&db);
            let next = if input.is_snapshot() {
                repo.claim_next_snapshot()
            } else {
                repo.claim_next()
            };
            let claimed = match next {
                Ok(Some(claimed)) => claimed,
                Ok(None) => {
                    drop(permit);
                    // No claimable row does not mean idle while tasks are
                    // loading or awaiting a model. Keep their discovery cycle.
                    // Once both are drained, the next cycle must refresh.
                    if tasks.is_empty() {
                        discovery_cache.clear().await;
                    }
                    break;
                }
                Err(e) => {
                    drop(permit);
                    warn!(error = %e, "[PIPELINE] Failed to claim durable job");
                    break;
                }
            };

            let task_db = Arc::clone(&db);
            let task_input = input.clone();
            let task_ai = ai_config.clone();
            let task_embedding = embedding_config.clone();
            let task_wake = wake_tx.clone();
            let task_discovery_cache = Arc::clone(&discovery_cache);

            tasks.spawn(async move {
                let durable_job_id = claimed.durable_job_id.clone();
                let attempt = claimed.attempt;
                let job = claimed.job;

                // Lease renewal shares the child future, so aborting a task
                // cannot leave a detached heartbeat holding the database open.
                let execution = run_pipeline(
                    &task_db, &task_input, &task_discovery_cache,
                    &task_ai, &task_embedding, &job,
                );
                tokio::pin!(execution);
                let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
                heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                heartbeat.tick().await;
                let result = loop {
                    tokio::select! {
                        result = &mut execution => break result,
                        _ = heartbeat.tick() => {
                            match PipelineJobRepo::new(&task_db).renew_lease(&durable_job_id) {
                                Ok(true) => {}
                                Ok(false) => break Err(anyhow::anyhow!("Pipeline lease was lost")),
                                Err(error) => break Err(error),
                            }
                        }
                    }
                };

                let durable_repo = PipelineJobRepo::new(&task_db);
                let result = result.and_then(|()| durable_repo.mark_succeeded(&durable_job_id));
                match result {
                    Ok(()) => {}
                    Err(e) if e.downcast_ref::<SupersededRevision>().is_some() => {
                        if let Err(error) = durable_repo.mark_superseded(&durable_job_id) {
                            error!(job_id = %durable_job_id, error = %error,
                                "[PIPELINE] Failed to persist superseded revision");
                        }
                    }
                    Err(e) => {
                        error!(
                            run_id = %job.pipeline_run_id,
                            session_id = job.session_id,
                            attempt,
                            error = %e,
                            "[PIPELINE] Job attempt failed"
                        );

                        // Keep pipeline_run's accurate stage failure. run_pipeline
                        // normally records it; this is a safety net for unexpected
                        // errors outside a stage boundary.
                        let pipeline_repo = PipelineRepo::new(&task_db);
                        let already_failed = pipeline_repo
                            .get_run_detail(&job.pipeline_run_id)
                            .ok()
                            .flatten()
                            .map(|d| d.status == "FAILED")
                            .unwrap_or(false);
                        if !already_failed {
                            let stage = extract_failed_stage(&e.to_string());
                            let _ = pipeline_repo.mark_failed(
                                &job.pipeline_run_id,
                                stage,
                                &e.to_string(),
                            );
                        }

                        match durable_repo.mark_failed(&durable_job_id, &e.to_string()) {
                            Ok(FailureDisposition::Retry) => {
                                info!(job_id = %durable_job_id, attempt, "[PIPELINE] Durable job scheduled for retry");
                            }
                            Ok(FailureDisposition::Terminal) => {
                                warn!(job_id = %durable_job_id, attempt, "[PIPELINE] Durable job exhausted retry budget");
                            }
                            Err(mark_err) => {
                                error!(job_id = %durable_job_id, error = %mark_err, "[PIPELINE] Failed to persist retry state");
                            }
                        }
                    }
                }

                drop(permit);
                let _ = task_wake.try_send(());
            });
        }
    }

    let deadline = (*stop_rx.borrow()).unwrap_or_else(tokio::time::Instant::now);
    let drained = tokio::time::timeout_at(deadline, async {
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result {
                warn!(error = %error, "[PIPELINE] Task stopped during shutdown");
            }
        }
    })
    .await;
    if drained.is_err() {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        anyhow::bail!("Pipeline shutdown deadline reached; unfinished leases remain recoverable");
    }
    info!("[PIPELINE] Durable worker stopped");
    Ok(())
}

/// R13: map a run_pipeline error message back to its pipeline stage.
/// run_pipeline prefixes stage errors with the stage name.
fn extract_failed_stage(error: &str) -> &'static str {
    const STAGES: [&str; 9] = [
        "PARSED",
        "CLEANED",
        "SESSION_INDEX",
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
    db: &Arc<StateDb>,
    input: &PipelineInputSource,
    discovery_cache: &ProviderDiscoveryCache,
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

    let mut revision_fence = None;
    let session = match input {
        PipelineInputSource::LegacyProviders(registry) => {
            // Find source kind
            let source_kind = match crate::model::SourceKind::from_str(&job.source) {
                Some(k) => k,
                None => fail_stage!("PARSED", format!("Unknown source: {}", job.source)),
            };

            // Load session from provider
            let provider = match registry.get(source_kind) {
                Some(p) => p,
                None => fail_stage!(
                    "PARSED",
                    format!("Provider not found for {:?}", source_kind)
                ),
            };

            let summary = match resolve_session_summary(
                provider,
                discovery_cache,
                source_kind,
                &job.session_external_id,
            )
            .await
            {
                Ok(summary) => summary,
                Err(e) => fail_stage!("PARSED", format!("Discover/resolve failed: {}", e)),
            };

            let session = match provider.load_session(&summary).await {
                Ok(s) => s,
                Err(e) => fail_stage!("PARSED", format!("Load session failed: {}", e)),
            };

            session
        }
        PipelineInputSource::PersistedSnapshots => match SnapshotInput::load_for_run(db, run_id) {
            Ok((session, fence)) => {
                fence.check_current(db)?;
                revision_fence = Some(fence);
                session
            }
            Err(error) => fail_stage!("PARSED", error),
        },
    };
    // A later upload may change source_session metadata while an older run
    // is leased. Its processing input must retain its own snapshot metadata.
    let mut effective_job = job.clone();
    if input.is_snapshot() {
        effective_job.session_title = session.title.clone();
        effective_job.project_name = session.project_name.clone();
    }
    let job = &effective_job;

    repo.record_stage(
        run_id,
        "PARSED",
        "SUCCESS",
        None,
        Some(session.messages.len() as i32),
        None,
        Some(&serde_json::json!({"message_count": session.messages.len()})),
        None,
    )?;
    info!("[PARSE] {} messages loaded", session.messages.len());

    // ── Stage 2: CLEAN ────────────────────────────────────────────────────────
    repo.update_status(run_id, "PROCESSING", Some("CLEANED"), None, None)?;

    let clean_result = clean_messages(session.messages.clone());

    repo.record_stage(
        run_id,
        "CLEANED",
        "SUCCESS",
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
    info!(
        "[CLEAN] {} → {} messages",
        clean_result.original_count, clean_result.cleaned_count
    );

    if clean_result.cleaned_count == 0 {
        repo.mark_finished_guarded(run_id, "RAW_ONLY", revision_fence.as_ref())?;
        return Ok(());
    }

    // Session search is independent from AI extraction and from canonical
    // Knowledge indexing. Keep the cleaned session searchable even when AI is
    // disabled; semantic embedding failures degrade inside SessionIndexService.
    let mut searchable_session = session.clone();
    searchable_session.messages = clean_result.messages.clone();
    let normalized_text =
        MarkdownRenderer::new(ContentConfig::default(), true).render(&searchable_session);
    let models = Arc::new(ModelService::new(
        ai_config.clone(),
        embedding_config.clone(),
    )?);
    SessionIndexService::new(Arc::clone(db), models)
        .index_session_guarded(
            SessionIndexInput {
                session_id: job.session_id,
                external_id: job.session_external_id.clone(),
                source: job.source.clone(),
                title: job.session_title.clone().or_else(|| session.title.clone()),
                normalized_text,
            },
            revision_fence.as_ref(),
        )
        .await
        .context("SESSION_INDEX")?;

    // ── Stage 3: LLM CHUNK ───────────────────────────────────────────────────
    repo.update_status(run_id, "PROCESSING", Some("LLM_CHUNKED"), None, None)?;

    let chunk_result = chunk_for_llm(job.session_id, &clean_result.messages);
    save_chunks_guarded(db, &chunk_result.chunks, revision_fence.as_ref())?;

    repo.record_stage(
        run_id,
        "LLM_CHUNKED",
        "SUCCESS",
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
        repo.mark_finished_guarded(run_id, "RAW_ONLY", revision_fence.as_ref())?;
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

    let item_count = match ai_stage
        .run_guarded(
            db,
            run_id,
            job.session_id,
            job.session_title.as_deref(),
            job.project_name.as_deref(),
            revision_fence.as_ref(),
        )
        .await
    {
        Ok(n) => n,
        Err(e) if e.downcast_ref::<SupersededRevision>().is_some() => return Err(e),
        Err(e) => {
            let msg = format!("AI extraction failed: {}", e);
            warn!(run_id, error = %e, "[AI] Extraction failed");
            repo.mark_failed(run_id, "AI_EXTRACTED", &msg)?;
            return Err(anyhow::anyhow!(msg));
        }
    };

    if item_count == 0 {
        repo.mark_finished_guarded(run_id, "RAW_ONLY", revision_fence.as_ref())?;
        return Ok(());
    }

    // ── Stage 5-7: CANONICAL INDEX DEFERRED ───────────────────────────────────
    // Pipeline extraction only produces knowledge candidates. FTS/chunk/vector
    // indexing belongs to the canonical SiYuan document after publication, so
    // this worker must never create a second knowledge index from SQLite text.
    repo.update_status(run_id, "PROCESSING", Some("EMBED_CHUNKED"), None, None)?;
    let deferred_index = serde_json::json!({
        "reason": "canonical SiYuan indexing occurs after publication"
    });
    repo.record_stage(
        run_id,
        "EMBED_CHUNKED",
        "SKIPPED",
        None,
        None,
        None,
        Some(&deferred_index),
        None,
    )?;

    repo.update_status(run_id, "PROCESSING", Some("EMBEDDED"), None, None)?;
    repo.record_stage(
        run_id,
        "EMBEDDED",
        "SKIPPED",
        None,
        None,
        None,
        Some(&deferred_index),
        None,
    )?;

    repo.update_status(run_id, "PROCESSING", Some("INDEXED"), None, None)?;
    repo.record_stage(
        run_id,
        "INDEXED",
        "SKIPPED",
        None,
        None,
        None,
        Some(&deferred_index),
        None,
    )?;

    // ── DONE ──────────────────────────────────────────────────────────────────
    repo.mark_finished_guarded(run_id, "READY", revision_fence.as_ref())?;
    info!(
        session_id = job.session_id,
        run_id = %run_id,
        items = item_count,
        "[PIPELINE] Job READY"
    );

    Ok(())
}
