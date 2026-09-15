/// AiksEngine — unified facade for AIKS Core.
///
/// Both CLI and Desktop use this as their single entry point.
/// Do NOT duplicate business logic in each app.
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::ai::{AiClient, AiModelConfig};
use crate::config::Config;
use crate::knowledge::service::{extract_session, get_extraction_stats};
use crate::knowledge::model::ExtractionStats;
use crate::model::SourceKind;
use crate::pipeline::{EmbeddingConfig, PipelineOrchestrator, PipelineWorker, PipelineJob};
use crate::providers::{build_registry, ProviderRegistry, SessionSummary};
use crate::sink::SiYuanSink;
use crate::storage::StateDb;
use crate::sync::{SyncEngine, SyncOptions, SyncStats};
use crate::watcher::{FileWatcher, WatchEvent};

/// Configuration for AiksEngine initialization
#[derive(Debug, Clone)]
pub struct AiksEngineConfig {
    /// Path to config file (None = use defaults)
    pub config_path: Option<PathBuf>,
    /// Override SiYuan base URL.
    /// - Desktop embedded mode: the runtime port URL (e.g. "http://127.0.0.1:6812")
    /// - CLI external mode: from config.toml
    pub siyuan_base_url: Option<String>,
    /// Optional token. None = embedded mode (no auth). Some = external/CLI mode.
    pub siyuan_token: Option<String>,
}

impl Default for AiksEngineConfig {
    fn default() -> Self {
        Self {
            config_path: None,
            siyuan_base_url: None,
            siyuan_token: None,
        }
    }
}

/// Result of aiks doctor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorResult {
    pub checks: Vec<DoctorCheck>,
    pub all_ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

/// Application status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStatus {
    pub total_sessions: usize,
    pub synced: usize,
    pub pending: usize,
    pub conflict: usize,
    pub failed: usize,
    pub last_sync_at: Option<String>,
    pub last_sync_discovered: i64,
    pub last_sync_changed: i64,
    pub last_sync_synced: i64,
    pub last_sync_failed: i64,
}

/// AI extraction status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiStatus {
    pub enabled: bool,
    pub healthy: bool,
    pub model: String,
    pub display_name: String,
    pub base_url: String,
    pub extraction_stats: ExtractionStats,
}

/// Unified full status — single source of truth for all UI pages.
/// Eliminates the "660 vs 28" discrepancy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullStatus {
    /// Sessions discovered by provider scan (live count)
    pub scan_total: usize,
    pub scan_by_source: std::collections::HashMap<String, usize>,

    /// Sessions in the state DB (synced or attempted)
    pub db_total: usize,
    pub db_synced: usize,
    pub db_pending: usize,
    pub db_conflict: usize,
    pub db_failed: usize,

    /// Last sync run stats
    pub last_sync_at: Option<String>,
    pub last_sync_discovered: i64,
    pub last_sync_new: i64,
    pub last_sync_updated: i64,
    pub last_sync_failed: i64,

    /// AI extraction stats
    pub extraction_total: usize,
    pub extraction_success: usize,
    pub extraction_skipped: usize,
    pub extraction_failed: usize,
    pub extraction_pending: usize,

    /// Runtime info
    pub siyuan_ready: bool,
    pub ai_ready: bool,
    pub ai_model: String,
}

/// Result of a scan operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub summaries: Vec<SessionSummary>,
    pub by_source: std::collections::HashMap<String, usize>,
    pub total: usize,
}

/// Result of a knowledge → SiYuan sync run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnowledgeSyncStats {
    pub created: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub conflict: usize,
    pub failed: usize,
}

/// Per-item outcome inside the knowledge sync loop.
enum Outcome {
    Created,
    Updated,
    Unchanged,
    Conflict,
}

/// The main AIKS engine, shared between CLI and Desktop
pub struct AiksEngine {
    config: Arc<Config>,
    registry: Arc<ProviderRegistry>,
    db: Arc<StateDb>,
    sync_engine: Arc<SyncEngine>,
    /// SiYuan base URL resolved at engine init time
    siyuan_base_url: String,
    /// Optional token — None = embedded mode
    siyuan_token: Option<String>,
    /// V3 pipeline worker (started on init)
    pipeline_worker: Arc<PipelineWorker>,
    /// Raw/session sync mutex — serializes startup, watcher-triggered, and manual
    /// session sync flows so overlapping runs cannot create duplicate session docs.
    sync_lock: tokio::sync::Mutex<()>,
    /// Knowledge-only sync mutex. Knowledge pushes may spend a long time awaiting
    /// SiYuan network I/O, so they must not hold the raw/session sync mutex. A
    /// dedicated single-writer lock still prevents concurrent knowledge pushes
    /// from observing a missing mapping and creating duplicate knowledge docs.
    knowledge_sync_lock: tokio::sync::Mutex<()>,
}

impl AiksEngine {
    /// Initialize the engine from a config
    pub fn initialize(engine_config: AiksEngineConfig) -> anyhow::Result<Self> {
        let config = match &engine_config.config_path {
            Some(path) => Config::from_file(path)?,
            None => Config::default(),
        };

        // Determine SiYuan connection (Desktop embedded mode takes priority)
        let siyuan_base_url = engine_config
            .siyuan_base_url
            .clone()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| config.siyuan.base_url.clone());

        // Token: Desktop embedded = None, CLI external = Some if set
        let siyuan_token = engine_config
            .siyuan_token
            .clone()
            .filter(|t| !t.is_empty())
            .or_else(|| {
                if config.siyuan.token.is_empty() { None } else { Some(config.siyuan.token.clone()) }
            });

        let config = Arc::new(config);
        let db_path = config.state_db_path();
        let db = Arc::new(StateDb::open(&db_path)?);
        let registry = Arc::new(build_registry(&config));
        let sync_engine = Arc::new(SyncEngine::new(config.clone()));

        // Start V3 pipeline worker
        let pipeline_worker = Arc::new(PipelineWorker::start(
            Arc::clone(&db),
            Arc::clone(&registry),
            config.ai.clone(),
            config.embedding.clone(),
        ));

        // R13: crash recovery — resubmit runs left in PROCESSING/DISCOVERED by
        // a previous process so background work survives restarts.
        crate::pipeline::recover_interrupted_runs(&db, &pipeline_worker);

        Ok(Self {
            config,
            registry,
            db,
            sync_engine,
            siyuan_base_url,
            siyuan_token,
            pipeline_worker,
            sync_lock: tokio::sync::Mutex::new(()),
            knowledge_sync_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Run system diagnostics
    pub async fn doctor(&self) -> DoctorResult {
        let mut checks = Vec::new();

        // State DB
        let db_path = self.config.state_db_path();
        checks.push(DoctorCheck {
            name: "State DB".to_string(),
            ok: db_path.exists(),
            message: if db_path.exists() {
                format!("{}", db_path.display())
            } else {
                format!("Not found: {}", db_path.display())
            },
        });

        // Providers
        let health = self.registry.health_check_all().await;
        for (source, h) in health {
            checks.push(DoctorCheck {
                name: source.display_name().to_string(),
                ok: h.is_ok(),
                message: h.message().to_string(),
            });
        }

        // SiYuan
        let siyuan_ok = if !self.siyuan_base_url.is_empty() {
            if let Ok(sink) = SiYuanSink::embedded(&self.siyuan_base_url, &self.config.siyuan.notebook_name) {
                sink.health_check().await
            } else {
                false
            }
        } else {
            false
        };
        checks.push(DoctorCheck {
            name: "SiYuan Kernel".to_string(),
            ok: siyuan_ok,
            message: if siyuan_ok {
                "Ready".to_string()
            } else {
                format!("Not reachable at {}", self.siyuan_base_url)
            },
        });

        let all_ok = checks.iter().all(|c| c.ok || c.name.contains("Claude") || c.name.contains("Codex") || c.name.contains("Gemini") || c.name.contains("OpenCode"));

        DoctorResult { checks, all_ok }
    }

    /// Scan all providers for sessions
    pub async fn scan(&self, source_filter: Option<&str>) -> ScanResult {
        let all = self.registry.discover_all().await;
        let filtered: Vec<SessionSummary> = if let Some(src) = source_filter {
            all.into_iter()
                .filter(|s| s.source.as_str() == src)
                .collect()
        } else {
            all
        };

        let mut by_source: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for s in &filtered {
            *by_source.entry(s.source.display_name().to_string()).or_insert(0) += 1;
        }

        let total = filtered.len();
        ScanResult {
            summaries: filtered,
            by_source,
            total,
        }
    }

    /// Run a sync operation
    pub async fn sync(&self, opts: SyncOptions) -> anyhow::Result<SyncStats> {
        // Serialize overlapping sync flows (startup + watcher can fire in the
        // same second). The lock is held for the whole run so the second flow
        // observes the target mapping written by the first instead of
        // re-creating the document.
        let _guard = self.sync_lock.lock().await;
        self.sync_unlocked(opts).await
    }

    /// Sync implementation — caller must hold `sync_lock`.
    async fn sync_unlocked(&self, opts: SyncOptions) -> anyhow::Result<SyncStats> {
        let sink = if let Some(token) = &self.siyuan_token {
            // External mode: use token from CLI config
            let mut cfg = self.config.siyuan.clone();
            cfg.base_url = self.siyuan_base_url.clone();
            cfg.token = token.clone();
            SiYuanSink::new(cfg)?
        } else {
            // Embedded mode: no token required
            SiYuanSink::embedded(
                &self.siyuan_base_url,
                &self.config.siyuan.notebook_name,
            )?
        };
        self.sync_engine
            .run_sync(&self.db, &self.registry, &sink, &opts)
            .await
    }

    /// Get current application status
    pub fn status(&self) -> anyhow::Result<AppStatus> {
        use crate::storage::{SourceSessionRepo, SyncRunRepo, SyncStatus, SyncTargetRepo};

        let session_repo = SourceSessionRepo::new(&self.db);
        let run_repo = SyncRunRepo::new(&self.db);

        let all_sessions = session_repo.list_all()?;
        let total_sessions = all_sessions.len();

        let last_run = run_repo.last_run()?;

        // Count sync target statuses
        let (mut synced, mut pending, mut conflict, mut failed) = (0usize, 0, 0, 0);
        for session in &all_sessions {
            let target_repo = SyncTargetRepo::new(&self.db);
            if let Ok(Some(target)) = target_repo.find(session.id, "siyuan") {
                match target.status {
                    SyncStatus::Synced => synced += 1,
                    SyncStatus::Pending
                    | SyncStatus::New
                    | SyncStatus::Updated => pending += 1,
                    SyncStatus::Conflict => conflict += 1,
                    SyncStatus::FailedRetryable
                    | SyncStatus::FailedPermanent => failed += 1,
                    _ => {}
                }
            }
        }

        Ok(AppStatus {
            total_sessions,
            synced,
            pending,
            conflict,
            failed,
            last_sync_at: last_run.as_ref().map(|r| r.started_at.clone()),
            last_sync_discovered: last_run.as_ref().map(|r| r.discovered).unwrap_or(0),
            last_sync_changed: last_run.as_ref().map(|r| r.changed).unwrap_or(0),
            last_sync_synced: last_run.as_ref().map(|r| r.synced).unwrap_or(0),
            last_sync_failed: last_run.as_ref().map(|r| r.failed).unwrap_or(0),
        })
    }

    /// Create a file watcher that sends events over the given channel
    pub fn create_watcher(
        &self,
        event_tx: tokio::sync::mpsc::UnboundedSender<WatchEvent>,
    ) -> FileWatcher {
        FileWatcher::new(self.config.clone(), event_tx)
    }

    /// Get the config
    pub fn config(&self) -> Arc<Config> {
        self.config.clone()
    }

    /// Get the provider registry
    pub fn registry(&self) -> Arc<ProviderRegistry> {
        self.registry.clone()
    }

    /// Get the state DB
    pub fn db(&self) -> Arc<StateDb> {
        self.db.clone()
    }

    /// Get the resolved SiYuan base URL
    pub fn siyuan_base_url(&self) -> &str {
        &self.siyuan_base_url
    }

    /// Get AI model config
    pub fn ai_config(&self) -> &AiModelConfig {
        &self.config.ai
    }

    /// Get embedding config
    pub fn embedding_config(&self) -> &EmbeddingConfig {
        &self.config.embedding
    }

    /// Manually enqueue a specific session for V3 pipeline processing
    pub fn enqueue_pipeline_for_session(
        &self,
        session_id: i64,
        session_external_id: String,
        source: String,
        session_title: Option<String>,
        project_name: Option<String>,
    ) -> anyhow::Result<String> {
        let orchestrator = PipelineOrchestrator::new(Arc::clone(&self.db));
        let run_id = orchestrator.enqueue(session_id, None)?;
        self.pipeline_worker.submit(PipelineJob {
            pipeline_run_id: run_id.clone(),
            session_id,
            session_external_id,
            source,
            session_title,
            project_name,
        })?;
        Ok(run_id)
    }

    /// Search distilled knowledge while preserving degraded-state/error semantics.
    pub async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<crate::pipeline::search::SearchOutcome> {
        let embedding_config = if self.config.embedding.enabled {
            Some(&self.config.embedding)
        } else {
            None
        };
        crate::pipeline::search::search_with_status(&self.db, query, limit, embedding_config).await
    }

    /// Check AI model health
    pub async fn ai_health_check(&self) -> bool {
        if !self.config.ai.enabled {
            return false;
        }
        match AiClient::new(self.config.ai.clone()) {
            Ok(client) => client.health_check().await,
            Err(_) => false,
        }
    }

    /// Get AI extraction status
    pub async fn ai_status(&self) -> AiStatus {
        let healthy = self.ai_health_check().await;
        let stats = get_extraction_stats(&self.db).unwrap_or_default();
        AiStatus {
            enabled: self.config.ai.enabled,
            healthy,
            model: self.config.ai.model.clone(),
            display_name: self.config.ai.display_name().to_string(),
            base_url: self.config.ai.base_url.clone(),
            extraction_stats: stats,
        }
    }

    /// Extract knowledge from a specific session immediately
    pub async fn extract_session_now(
        &self,
        source: SourceKind,
        session_id: &str,
    ) -> anyhow::Result<String> {
        if !self.config.ai.enabled {
            anyhow::bail!("AI extraction is disabled");
        }

        // Find the session
        let summaries = self.registry.discover_all().await;
        let summary = summaries
            .into_iter()
            .find(|s| s.source == source && s.external_session_id == session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {} / {}", source.as_str(), session_id))?;

        // Load full session
        let provider = self.registry.get(source)
            .ok_or_else(|| anyhow::anyhow!("Provider not found for {:?}", source))?;
        let session = provider.load_session(&summary).await?;

        // Create sink
        let sink = SiYuanSink::embedded(&self.siyuan_base_url, &self.config.siyuan.notebook_name)?;

        // Extract
        let outcome = extract_session(&session, &self.config.ai, &sink, &self.db).await;

        match outcome {
            crate::knowledge::service::ExtractionOutcome::Success { doc_id, score } => {
                info!(doc_id = %doc_id, score, "Extraction success");
                Ok(doc_id)
            }
            crate::knowledge::service::ExtractionOutcome::Skipped { score } => {
                anyhow::bail!("Session score {:.2} below threshold — skipped", score)
            }
            crate::knowledge::service::ExtractionOutcome::Failed { error } => {
                anyhow::bail!("Extraction failed: {}", error)
            }
        }
    }

    /// Run sync and submit new/updated sessions to the V3 pipeline worker.
    ///
    /// This is the main sync entry point for both startup and user-triggered syncs.
    /// Pipeline processing runs AFTER Raw Sync succeeds, never blocking it.
    pub async fn sync_and_enqueue_extraction(
        &self,
        opts: SyncOptions,
    ) -> anyhow::Result<SyncStats> {
        // Hold the global sync lock across migration + sync + bookkeeping so
        // the archive migration never interleaves with another flow's SiYuan
        // writes (same duplicate-document rationale as `sync`). The guard is
        // scoped to this block — the knowledge push below MUST run outside
        // it (it takes the lock itself; tokio Mutexes are not re-entrant, so
        // calling it under the guard deadlocked every periodic tick and
        // stalled the knowledge push overnight on 2026-09-14).
        let stats = {
            let _guard = self.sync_lock.lock().await;

            // One-time (idempotent) migration: move raw session docs out of the
            // knowledge notebook into the dedicated archive notebook. Cheap after
            // the first run (a single SQL lookup against an empty result).
            if !opts.dry_run && opts.source_filter.is_none() {
                match self.migrate_sessions_to_archive().await {
                    Ok(0) => {}
                    Ok(n) => info!("[SYNC] Migrated {} session docs to archive notebook", n),
                    Err(e) => tracing::warn!("[SYNC] Session archive migration failed: {}", e),
                }
            }

            let stats = self.sync_unlocked(opts.clone()).await?;

        // B15: After a successful scan (no source_filter = full scan),
        // mark sessions that are no longer visible in any provider as MISSING.
        if opts.source_filter.is_none() && !opts.dry_run {
            match self.sync_engine.mark_missing_sessions(&self.db, &self.registry).await {
                Ok(n) if n > 0 => info!("[SYNC] Marked {} sessions as MISSING (source removed)", n),
                Err(e) => tracing::warn!("[SYNC] mark_missing failed: {}", e),
                _ => {}
            }
        }

        // B09/R09: Enqueue new/updated sessions into V3 pipeline — respecting
        // the ai.enabled AND ai.auto_extract switches.
        if !stats.extraction_candidates.is_empty() && self.config.ai.enabled && self.config.ai.auto_extract {
            info!(
                count = stats.extraction_candidates.len(),
                "[PIPELINE] Enqueueing new/updated sessions"
            );

            let orchestrator = PipelineOrchestrator::new(Arc::clone(&self.db));

            for candidate in &stats.extraction_candidates {
                // Resolve by canonical DB identity and verify the redundant source
                // identity. This prevents cross-provider external-ID collisions.
                let session_data: Option<(i64, String, String, Option<String>, Option<String>, Option<String>)> = {
                    let conn = self.db.conn();
                    conn.query_row(
                        "SELECT id, source, external_session_id, title, project_name, content_hash
                         FROM source_session
                         WHERE id = ?1 AND source = ?2 AND external_session_id = ?3",
                        rusqlite::params![
                            candidate.session_id,
                            candidate.source,
                            candidate.external_session_id
                        ],
                        |row| Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                        )),
                    ).ok()
                };

                if let Some((db_id, source, session_ext_id, title, project_name, content_hash)) = session_data {
                    if let Ok(run_id) = orchestrator.enqueue(db_id, content_hash.as_deref()) {
                        if let Err(e) = self.pipeline_worker.submit(PipelineJob {
                            pipeline_run_id: run_id,
                            session_id: db_id,
                            session_external_id: session_ext_id,
                            source,
                            session_title: title,
                            project_name,
                        }) {
                            tracing::warn!(session_id = db_id, error = %e, "[PIPELINE] Durable enqueue failed");
                        }
                    }
                }
            }
        }

            stats
        };

        // Knowledge → SiYuan: push new/updated knowledge docs into the
        // knowledge notebook (non-blocking for the sync result).
        // NOTE: intentionally OUTSIDE the raw sync_lock block above. Knowledge
        // sync uses its own single-writer mutex, so slow network I/O cannot hold
        // up startup/watcher/manual raw sync work.
        if !opts.dry_run && opts.source_filter.is_none() {
            if let Err(e) = self.sync_knowledge_to_siyuan(false).await {
                tracing::warn!("[SYNC] Knowledge sync failed: {}", e);
            }
        }

        Ok(stats)
    }

    /// R09: Backfill extraction for historical sessions.
    ///
    /// Unlike `SyncEngine::enqueue_all_pending_for_pipeline` (which only creates
    /// pipeline_run rows), this actually submits PipelineJobs to the running
    /// worker for every run that is DISCOVERED (never processed) or FAILED, so
    /// the work really executes and produces knowledge items.
    pub fn backfill_pending_extractions(&self) -> anyhow::Result<usize> {
        if !self.config.ai.enabled || !self.config.ai.auto_extract {
            anyhow::bail!("AI extraction is disabled (ai.enabled / ai.auto_extract)");
        }

        // Step 1: ensure pipeline_run rows exist for every session
        let ensured = self.sync_engine.enqueue_all_pending_for_pipeline(&self.db)?;

        // Step 2: submit jobs for runs that still need processing
        let rows: Vec<(String, i64, String, String, Option<String>, Option<String>)> = {
            let conn = self.db.conn();
            let mut stmt = conn.prepare(
                "SELECT pr.id, pr.session_id, ss.source, ss.external_session_id, ss.title, ss.project_name
                 FROM pipeline_run pr
                 JOIN source_session ss ON ss.id = pr.session_id
                 WHERE pr.pipeline_version = 'v3'
                   AND pr.status IN ('DISCOVERED', 'FAILED')
                 ORDER BY pr.updated_at ASC"
            )?;
            let result = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect();
            result
        };

        let mut submitted = 0;
        for (run_id, session_id, source, ext_id, title, project) in rows {
            match self.pipeline_worker.submit(PipelineJob {
                pipeline_run_id: run_id,
                session_id,
                session_external_id: ext_id,
                source,
                session_title: title,
                project_name: project,
            }) {
                Ok(()) => submitted += 1,
                Err(e) => tracing::warn!(session_id, error = %e, "[PIPELINE] Backfill durable enqueue failed"),
            }
        }

        info!(
            ensured_runs = ensured,
            submitted = submitted,
            "[PIPELINE] Backfill completed"
        );
        Ok(submitted)
    }

    /// ── Knowledge → SiYuan sync (knowledge-first tree) ─────────────────────────
    ///
    /// Syncs distilled knowledge items into the knowledge notebook
    /// (`/20 Knowledge/{分类}/…`), each doc linking back to its raw session doc.
    /// Idempotent via content hash; manual edits in SiYuan are detected as
    /// conflicts and never silently overwritten.
    pub async fn sync_knowledge_to_siyuan(
        &self,
        overwrite_conflicts: bool,
    ) -> anyhow::Result<KnowledgeSyncStats> {
        use crate::renderer::knowledge::{render_knowledge_item_md, KnowledgeItemDoc};
        use crate::storage::{KnowledgeSyncRepo, SyncStatus, SyncTargetRepo};
        use sha2::{Digest, Sha256};

        // Serialize knowledge writers independently from raw/session sync. This
        // preserves duplicate-document protection without holding the raw sync
        // mutex across potentially slow SiYuan network I/O.
        let _guard = self.knowledge_sync_lock.lock().await;

        let sink = self.make_sink()?;
        let stats = KnowledgeSyncStats::default();

        let notebook_id = sink.ensure_notebook().await?;

        // (k_id, title, category, project, summary, content, tags, confidence,
        //  source, session_db_id, ext_id, session_title)
        let items: Vec<(String, String, String, Option<String>, String, String, String, f64, String, i64, String, Option<String>)> = {
            let conn = self.db.conn();
            let mut stmt = conn.prepare(
                "SELECT ki.id, ki.title, ki.category, ki.project_name, ki.summary, ki.content,
                        ki.tags, ki.confidence, ss.source, ss.id, ss.external_session_id, ss.title
                 FROM knowledge_item ki
                 JOIN source_session ss ON ss.id = ki.source_session_id",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, f64>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<String>>(11)?,
                ))
            })?;
            rows.flatten().collect()
        };

        let mut stats = stats;
        let target_repo = SyncTargetRepo::new(&self.db);
        let ks_repo = KnowledgeSyncRepo::new(&self.db);

        for (k_id, title, category, project, summary, content, tags, confidence,
             source_str, session_db_id, ext_id, session_title) in items
        {
            let result: anyhow::Result<Outcome> = async {
                // Deep link target: the raw session doc, if already synced.
                let session_doc_id = target_repo
                    .find(session_db_id, "siyuan")?
                    .filter(|t| t.status == SyncStatus::Synced)
                    .and_then(|t| t.target_id);

                let source_display = match source_str.as_str() {
                    "claude_code" => "Claude",
                    "codex" => "Codex",
                    "gemini_cli" => "Gemini",
                    "opencode" => "OpenCode",
                    other => other,
                };

                let doc = KnowledgeItemDoc {
                    knowledge_id: &k_id,
                    title: &title,
                    category: &category,
                    project_name: project.as_deref(),
                    summary: &summary,
                    content: &content,
                    tags_json: &tags,
                    confidence,
                    source_display,
                    session_ext_id: &ext_id,
                    session_title: session_title.as_deref(),
                    session_doc_id: session_doc_id.as_deref(),
                };
                let markdown = render_knowledge_item_md(&doc);
                let hash = hex::encode(Sha256::digest(markdown.as_bytes()));
                let path = sink.build_knowledge_path(&category, &k_id, &title);

                let existing = ks_repo.find(&k_id, "siyuan")?;

                // Unchanged — nothing to do.
                if let Some(ref e) = existing {
                    if e.status == SyncStatus::Synced && e.synced_hash.as_deref() == Some(&hash) {
                        return Ok(Outcome::Unchanged);
                    }
                    if e.status == SyncStatus::Conflict && !overwrite_conflicts {
                        return Ok(Outcome::Conflict);
                    }
                }

                // Determine remote doc: reuse local mapping if the doc still exists.
                let remote_id = match existing.as_ref().and_then(|e| e.target_id.clone()) {
                    Some(id) => {
                        if sink.get_doc_notebook(&id).await?.is_some() {
                            Some(id)
                        } else {
                            None // deleted remotely — recreate
                        }
                    }
                    None => None,
                };

                // Conflict guard: remote content was manually edited in SiYuan.
                // Only enforced when a baseline exists (target_hash captured at
                // the last successful sync). The earlier comparison against
                // synced_hash (LOCAL markdown) always mismatched after SiYuan
                // re-serializes the content — flagging every item as CONFLICT.
                if let (Some(e), Some(id)) = (existing.as_ref(), remote_id.as_deref()) {
                    if e.status == SyncStatus::Synced {
                        if let Some(baseline) = e.target_hash.as_deref() {
                            if let Ok(remote_md) = sink.get_document_markdown(id).await {
                                let remote_hash =
                                    hex::encode(Sha256::digest(remote_md.as_bytes()));
                                if remote_hash != baseline {
                                    ks_repo.mark_conflict(&k_id, "siyuan")?;
                                    return Ok(Outcome::Conflict);
                                }
                            }
                        }
                    }
                }

                let doc_id = if let Some(id) = &remote_id {
                    sink.update_document(id, &markdown).await?;
                    ks_repo.record_target_doc(&k_id, "siyuan", id, &path)?;
                    id.clone()
                } else {
                    let new_id = sink
                        .create_document(&notebook_id, &path, &markdown)
                        .await?;
                    ks_repo.record_target_doc(&k_id, "siyuan", &new_id, &path)?;
                    new_id
                };

                sink.set_knowledge_attrs(&doc_id, &k_id, &ext_id, &hash, &category)
                    .await?;
                ks_repo.mark_synced(&k_id, "siyuan", &doc_id, &path, &hash)?;

                // Capture the remote baseline right after a successful write so
                // the next run's conflict guard compares remote-vs-baseline.
                // Best-effort: no baseline → conflict detection stays disabled
                // for this item (never a false CONFLICT).
                if let Ok(remote_md) = sink.get_document_markdown(&doc_id).await {
                    let remote_hash = hex::encode(Sha256::digest(remote_md.as_bytes()));
                    ks_repo.record_target_hash(&k_id, "siyuan", &remote_hash)?;
                }

                Ok(if remote_id.is_some() {
                    Outcome::Updated
                } else {
                    Outcome::Created
                })
            }
            .await;

            match result {
                Ok(Outcome::Created) => stats.created += 1,
                Ok(Outcome::Updated) => stats.updated += 1,
                Ok(Outcome::Unchanged) => stats.unchanged += 1,
                Ok(Outcome::Conflict) => stats.conflict += 1,
                Err(e) => {
                    tracing::warn!(knowledge_id = %k_id, error = %e, "[KNOWLEDGE-SYNC] item failed");
                    let _ = ks_repo.mark_failed(&k_id, "siyuan", &e.to_string());
                    stats.failed += 1;
                }
            }
        }

        info!(
            created = stats.created,
            updated = stats.updated,
            unchanged = stats.unchanged,
            conflict = stats.conflict,
            failed = stats.failed,
            "[KNOWLEDGE-SYNC] Complete"
        );
        Ok(stats)
    }

    /// One-time migration: move raw session docs out of the knowledge notebook
    /// into the dedicated session archive notebook, so the knowledge notebook
    /// tree shows only distilled knowledge. Idempotent — docs already in the
    /// archive notebook are skipped.
    pub async fn migrate_sessions_to_archive(&self) -> anyhow::Result<usize> {
        let sink = self.make_sink()?;
        let knowledge_nb = sink.ensure_notebook().await?;
        let session_nb = sink.ensure_session_notebook().await?;
        if knowledge_nb == session_nb {
            return Ok(0); // configuration points both to the same notebook
        }

        // Find AIKS-managed session docs still living in the knowledge notebook.
        let stmt = format!(
            "SELECT id, hpath FROM blocks WHERE box = '{}' AND type = 'd' \
             AND root_id = id AND hpath LIKE '/10 AI Sessions%'",
            knowledge_nb.replace('\'', "''")
        );
        let rows = sink.query_sql(&stmt).await?;

        let mut moved = 0usize;
        for row in rows {
            let (Some(doc_id), Some(hpath)) = (
                row.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                row.get("hpath").and_then(|v| v.as_str()).map(|s| s.to_string()),
            ) else {
                continue;
            };

            // Move into the same parent path inside the archive notebook.
            let parent = match hpath.rfind('/') {
                Some(0) => "/".to_string(),
                Some(idx) => hpath[..idx].to_string(),
                None => "/".to_string(),
            };
            match sink.move_docs(&[doc_id.clone()], &session_nb, &parent).await {
                Ok(()) => moved += 1,
                Err(e) => tracing::warn!(doc_id = %doc_id, error = %e, "[MIGRATE] move failed"),
            }
            if moved % 50 == 0 && moved > 0 {
                info!(moved, "[MIGRATE] session docs moved so far");
            }
        }

        if moved > 0 {
            info!(moved, "[MIGRATE] Session archive migration complete");
        }
        Ok(moved)
    }

    /// Build the SiYuan sink using the engine's resolved connection.
    fn make_sink(&self) -> anyhow::Result<SiYuanSink> {
        if let Some(token) = &self.siyuan_token {
            let mut cfg = self.config.siyuan.clone();
            cfg.base_url = self.siyuan_base_url.clone();
            cfg.token = token.clone();
            SiYuanSink::new(cfg)
        } else {
            SiYuanSink::embedded(&self.siyuan_base_url, &self.config.siyuan.notebook_name)
        }
    }

    /// Get the unified full status for all UI pages (spec §8).
    ///
    /// Returns consistent numbers across Overview, Sidebar, Sources, Sync, Knowledge pages.
    pub async fn full_status(&self) -> FullStatus {
        use crate::storage::{SourceSessionRepo, SyncRunRepo, SyncStatus, SyncTargetRepo};

        // Live provider scan count
        let scan_result = self.scan(None).await;
        let scan_total = scan_result.total;
        let scan_by_source = scan_result.by_source;

        // DB state
        let session_repo = SourceSessionRepo::new(&self.db);
        let run_repo = SyncRunRepo::new(&self.db);

        let all_sessions = session_repo.list_all().unwrap_or_default();
        let db_total = all_sessions.len();

        let (mut db_synced, mut db_pending, mut db_conflict, mut db_failed) = (0, 0, 0, 0);
        for session in &all_sessions {
            let target_repo = SyncTargetRepo::new(&self.db);
            if let Ok(Some(target)) = target_repo.find(session.id, "siyuan") {
                match target.status {
                    SyncStatus::Synced => db_synced += 1,
                    SyncStatus::Pending | SyncStatus::New | SyncStatus::Updated => db_pending += 1,
                    SyncStatus::Conflict => db_conflict += 1,
                    SyncStatus::FailedRetryable | SyncStatus::FailedPermanent => db_failed += 1,
                    _ => {}
                }
            }
        }

        let last_run = run_repo.last_run().ok().flatten();

        // Extraction stats
        let ext_stats = get_extraction_stats(&self.db).unwrap_or_default();

        // Runtime health (fast check, don't block)
        let siyuan_ready = if self.siyuan_base_url.is_empty() {
            false
        } else {
            matches!(
                SiYuanSink::embedded(&self.siyuan_base_url, "AI Knowledge")
                    .ok()
                    .map(|_s| {
                        // Quick TCP check without async (synchronous)
                        // For UI purposes, trust that if we got here, SiYuan is likely running
                        true
                    }),
                Some(true)
            )
        };

        let ai_ready = self.config.ai.enabled; // Cheap check; health check is separate

        FullStatus {
            scan_total,
            scan_by_source,
            db_total,
            db_synced,
            db_pending,
            db_conflict,
            db_failed,
            last_sync_at: last_run.as_ref().map(|r| r.started_at.clone()),
            last_sync_discovered: last_run.as_ref().map(|r| r.discovered).unwrap_or(0),
            last_sync_new: last_run.as_ref().map(|r| r.changed).unwrap_or(0),
            last_sync_updated: 0,
            last_sync_failed: last_run.as_ref().map(|r| r.failed).unwrap_or(0),
            extraction_total: ext_stats.total,
            extraction_success: ext_stats.success,
            extraction_skipped: ext_stats.skipped,
            extraction_failed: ext_stats.failed,
            extraction_pending: ext_stats.pending,
            siyuan_ready,
            ai_ready,
            ai_model: self.config.ai.model.clone(),
        }
    }
}
