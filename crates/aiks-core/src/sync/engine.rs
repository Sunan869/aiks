/// Sync Engine: orchestrates provider → hash → state → render → sink pipeline.
///
/// Pipeline:
/// 1. Discover all sessions via providers
/// 2. For each session: check hash against state DB
/// 3. NEW/UPDATED: render → SiYuan Sink
/// 4. Track conflicts and missing sources
///
/// AI Extraction:
/// 5. After successful Raw Sync, return session IDs for extraction queue
use std::sync::Arc;

use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::model::hash::compute_session_hash;
use crate::providers::{ProviderRegistry, SessionSummary};
use crate::renderer::MarkdownRenderer;
use crate::sink::SiYuanSink;
use crate::storage::{StateDb, SourceSessionRepo, SyncRunRepo, SyncStatus, SyncTargetRepo};

/// Result of syncing a single session.
#[derive(Debug, Clone)]
pub enum SyncOutcome {
    Created { doc_id: String },
    Updated { doc_id: String },
    Unchanged,
    Skipped { reason: String },
    Conflict { doc_id: String },
    Failed { error: String },
}

/// Options for a sync run.
#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    pub source_filter: Option<String>,
    pub dry_run: bool,
    pub overwrite: bool,
}

/// Statistics for a completed sync run.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SyncStats {
    pub discovered: usize,
    pub new_count: usize,
    pub updated_count: usize,
    pub unchanged_count: usize,
    pub skipped_count: usize,
    pub conflict_count: usize,
    pub failed_count: usize,
    /// Session IDs newly created/updated — eligible for extraction
    pub extraction_candidates: Vec<String>,
}

pub struct SyncEngine {
    config: Arc<Config>,
}

impl SyncEngine {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    /// Run a full sync cycle.
    ///
    /// Returns SyncStats including sessions eligible for AI extraction.
    pub async fn run_sync(
        &self,
        db: &StateDb,
        registry: &ProviderRegistry,
        sink: &SiYuanSink,
        opts: &SyncOptions,
    ) -> anyhow::Result<SyncStats> {
        let run_repo = SyncRunRepo::new(db);
        let trigger = if opts.dry_run { "dry_run" } else { "manual" };
        let run_id = run_repo.start(trigger)?;

        let renderer = MarkdownRenderer::new(
            self.config.content.clone(),
            self.config.security.redact_secrets,
        );

        // Discover all sessions
        let all_summaries = registry.discover_all().await;
        let total_discovered = all_summaries.len();

        // Filter by source if requested
        let summaries: Vec<SessionSummary> = if let Some(src) = &opts.source_filter {
            all_summaries
                .into_iter()
                .filter(|s| s.source.as_str() == src.as_str())
                .collect()
        } else {
            all_summaries
        };

        info!(
            total_discovered,
            filtered = summaries.len(),
            source_filter = ?opts.source_filter,
            dry_run = opts.dry_run,
            "[SYNC] Starting sync"
        );

        let mut stats = SyncStats {
            discovered: summaries.len(),
            ..Default::default()
        };

        for summary in &summaries {
            // Filter: only skip if we KNOW message count is low and it's > 0
            // (Codex from state_5.sqlite has count=0 meaning "unknown" — don't skip those)
            let min_msgs = self.config.content.minimum_messages;
            if min_msgs > 0 && summary.message_count > 0
                && summary.message_count < min_msgs
            {
                debug!(
                    session_id = %summary.external_session_id,
                    message_count = summary.message_count,
                    "Skipping session (below minimum_messages threshold)"
                );
                stats.skipped_count += 1;
                continue;
            }

            let outcome = self
                .sync_session(db, registry, sink, &renderer, summary, opts)
                .await;

            match &outcome {
                SyncOutcome::Created { doc_id } => {
                    info!(
                        session_id = %summary.external_session_id,
                        doc_id = %doc_id,
                        "[SYNC] Created"
                    );
                    stats.new_count += 1;
                    stats.extraction_candidates.push(summary.external_session_id.clone());
                }
                SyncOutcome::Updated { doc_id } => {
                    info!(
                        session_id = %summary.external_session_id,
                        doc_id = %doc_id,
                        "[SYNC] Updated"
                    );
                    stats.updated_count += 1;
                    stats.extraction_candidates.push(summary.external_session_id.clone());
                }
                SyncOutcome::Unchanged => {
                    debug!(session_id = %summary.external_session_id, "Unchanged");
                    stats.unchanged_count += 1;
                }
                SyncOutcome::Skipped { reason } => {
                    debug!(session_id = %summary.external_session_id, reason = %reason, "Skipped");
                    stats.skipped_count += 1;
                }
                SyncOutcome::Conflict { doc_id } => {
                    warn!(
                        session_id = %summary.external_session_id,
                        doc_id = %doc_id,
                        "[SYNC] Conflict"
                    );
                    stats.conflict_count += 1;
                }
                SyncOutcome::Failed { error } => {
                    error!(
                        session_id = %summary.external_session_id,
                        error = %error,
                        "[SYNC] Failed"
                    );
                    stats.failed_count += 1;
                }
            }
        }

        run_repo.finish(
            run_id,
            stats.discovered as i64,
            (stats.new_count + stats.updated_count) as i64,
            (stats.new_count + stats.updated_count) as i64,
            stats.failed_count as i64,
        )?;

        info!(
            discovered = stats.discovered,
            new = stats.new_count,
            updated = stats.updated_count,
            unchanged = stats.unchanged_count,
            skipped = stats.skipped_count,
            failed = stats.failed_count,
            candidates = stats.extraction_candidates.len(),
            "[SYNC] Completed"
        );

        Ok(stats)
    }

    /// Sync a single session.
    async fn sync_session(
        &self,
        db: &StateDb,
        registry: &ProviderRegistry,
        sink: &SiYuanSink,
        renderer: &MarkdownRenderer,
        summary: &SessionSummary,
        opts: &SyncOptions,
    ) -> SyncOutcome {
        let source = summary.source.as_str();
        let session_id = &summary.external_session_id;
        let source_session_repo = SourceSessionRepo::new(db);
        let sync_target_repo = SyncTargetRepo::new(db);

        // Find existing state
        let existing = match source_session_repo.find_by_source_and_id(source, session_id) {
            Ok(e) => e,
            Err(e) => {
                return SyncOutcome::Failed { error: format!("DB lookup: {}", e) };
            }
        };

        // Load the full session
        let provider = match registry.get(summary.source) {
            Some(p) => p,
            None => {
                return SyncOutcome::Skipped {
                    reason: format!("No provider for {:?}", summary.source),
                };
            }
        };

        let session = match provider.load_session(summary).await {
            Ok(s) => s,
            Err(e) => {
                return SyncOutcome::Failed {
                    error: format!("load_session: {}", e),
                };
            }
        };

        // Check actual message count after loading (handles Codex count=0 from DB)
        let min_msgs = self.config.content.minimum_messages;
        if min_msgs > 0 && session.messages.len() < min_msgs {
            return SyncOutcome::Skipped {
                reason: format!("Only {} messages (min: {})", session.messages.len(), min_msgs),
            };
        }

        let content_hash = compute_session_hash(&session);
        let parser_version = provider.parser_version();

        // Check if content changed
        let is_new = existing.is_none();
        let hash_changed = existing
            .as_ref()
            .map(|e| e.content_hash.as_deref() != Some(&content_hash))
            .unwrap_or(true);
        let parser_changed = existing
            .as_ref()
            .map(|e| e.parser_version.as_deref() != Some(parser_version))
            .unwrap_or(false);

        // B03 fix: UNCHANGED only when content hash matches AND we have a confirmed SYNCED target.
        // This prevents "failure then UNCHANGED" syndrome where a failed sync poisons the hash.
        if !is_new && !hash_changed && !parser_changed {
            let db_session_id_check = existing.as_ref().map(|e| e.id).unwrap_or(0);
            let has_synced_target = match SyncTargetRepo::new(db).find(db_session_id_check, "siyuan") {
                Ok(Some(ref t)) => matches!(t.status, SyncStatus::Synced | SyncStatus::Unchanged),
                _ => false,
            };

            if has_synced_target {
                let source_updated_at = session.updated_at.map(|t| t.to_rfc3339());
                let _ = source_session_repo.upsert(
                    source, session_id,
                    session.source_path.as_ref().and_then(|p| p.to_str()),
                    session.project_path.as_deref(),
                    session.project_name.as_deref(),
                    session.title.as_deref(),
                    source_updated_at.as_deref(),
                    Some(&content_hash),
                    Some(parser_version),
                );
                return SyncOutcome::Unchanged;
            }
            // No synced target → fall through and retry
        }

        // Persist session metadata. The content_hash here is the observed hash.
        // SYNCED status is only set in sync_target after confirmed remote write.
        let source_updated_at = session.updated_at.map(|t| t.to_rfc3339());
        let db_session_id = match source_session_repo.upsert(
            source, session_id,
            session.source_path.as_ref().and_then(|p| p.to_str()),
            session.project_path.as_deref(),
            session.project_name.as_deref(),
            session.title.as_deref(),
            source_updated_at.as_deref(),
            Some(&content_hash),
            Some(parser_version),
        ) {
            Ok(id) => id,
            Err(e) => {
                return SyncOutcome::Failed { error: format!("upsert source_session: {}", e) };
            }
        };

        // dry-run: report intent but make NO changes to sync_target or SiYuan
        if opts.dry_run {
            return if is_new {
                SyncOutcome::Created { doc_id: "[dry-run]".to_string() }
            } else {
                SyncOutcome::Updated { doc_id: "[dry-run]".to_string() }
            };
        }

        // Check for existing SiYuan document
        let existing_doc = sink.find_document_by_session(source, session_id).await.ok().flatten();

        // Conflict check: target manually edited
        if let Some(ref doc) = existing_doc {
            if let Ok(Some(target)) = sync_target_repo.find(db_session_id, "siyuan") {
                if let Ok(attrs) = sink.get_block_attrs(&doc.id).await {
                    let current = attrs.get(crate::sink::siyuan::ATTR_CONTENT_HASH).cloned();
                    if current != target.synced_hash && !opts.overwrite {
                        let _ = sync_target_repo.mark_conflict(db_session_id, "siyuan");
                        return SyncOutcome::Conflict { doc_id: doc.id.clone() };
                    }
                }
            }
        }

        // Render to Markdown
        let markdown = renderer.render(&session);

        // Ensure notebook exists
        let notebook_id = match sink.ensure_notebook().await {
            Ok(id) => id,
            Err(e) => {
                let _ = sync_target_repo.mark_failed(db_session_id, "siyuan", &e.to_string(), true);
                return SyncOutcome::Failed { error: format!("ensure_notebook: {}", e) };
            }
        };

        let doc_path = sink.build_document_path(
            source, session_id,
            session.title.as_deref(),
            session.started_at.as_ref(),
        );

        // Create or update document
        let doc_id = if let Some(doc) = &existing_doc {
            match sink.update_document(&doc.id, &markdown).await {
                Ok(()) => doc.id.clone(),
                Err(e) => {
                    let _ = sync_target_repo.mark_failed(db_session_id, "siyuan", &e.to_string(), true);
                    return SyncOutcome::Failed { error: format!("update_document: {}", e) };
                }
            }
        } else {
            let _ = sync_target_repo.upsert_pending(db_session_id, "siyuan");
            match sink.create_document(&notebook_id, &doc_path, &markdown).await {
                Ok(id) => id,
                Err(e) => {
                    let _ = sync_target_repo.mark_failed(db_session_id, "siyuan", &e.to_string(), true);
                    return SyncOutcome::Failed { error: format!("create_document: {}", e) };
                }
            }
        };

        // Set AIKS metadata attributes
        if let Err(e) = sink
            .set_aiks_attrs(&doc_id, source, session_id, &content_hash, parser_version)
            .await
        {
            warn!(error = %e, "Could not set AIKS attrs (non-fatal)");
        }

        // Update sync_target state
        let target_hash = content_hash.clone(); // Use content hash as reference
        let _ = sync_target_repo.mark_synced(
            db_session_id, "siyuan",
            &doc_id, &doc_path,
            &content_hash, &target_hash,
        );

        if is_new {
            SyncOutcome::Created { doc_id }
        } else {
            SyncOutcome::Updated { doc_id }
        }
    }

    /// Mark sessions as missing when source files are deleted.
    pub async fn mark_missing_sessions(
        &self,
        db: &StateDb,
        registry: &ProviderRegistry,
    ) -> anyhow::Result<usize> {
        let source_session_repo = SourceSessionRepo::new(db);
        let all_stored = source_session_repo.list_all()?;
        let all_discovered = registry.discover_all().await;

        let visible: std::collections::HashSet<(String, String)> = all_discovered
            .iter()
            .map(|s| (s.source.as_str().to_string(), s.external_session_id.clone()))
            .collect();

        let mut marked = 0;
        for stored in &all_stored {
            if stored.is_missing { continue; }
            let key = (stored.source.clone(), stored.external_session_id.clone());
            if !visible.contains(&key) {
                source_session_repo.mark_missing(&stored.source, &stored.external_session_id)?;
                marked += 1;
            }
        }
        Ok(marked)
    }

    /// B09: Enqueue all sessions in the DB into the V3 pipeline.
    ///
    /// Used after sync to create pipeline_run records for sessions that
    /// were discovered but don't yet have a pipeline run (or have a failed one).
    pub fn enqueue_all_pending_for_pipeline(&self, db: &StateDb) -> anyhow::Result<usize> {
        use crate::pipeline::repo::PipelineRepo;

        // Step 1: Collect sessions needing pipeline runs (hold lock briefly, then release)
        let rows: Vec<(i64, Option<String>)> = {
            let conn = db.conn();
            let mut stmt = conn.prepare(
                "SELECT ss.id, ss.content_hash
                 FROM source_session ss
                 WHERE NOT EXISTS (
                     SELECT 1 FROM pipeline_run pr
                     WHERE pr.session_id = ss.id
                       AND pr.pipeline_version = 'v3'
                       AND pr.status NOT IN ('FAILED', 'DISCOVERED')
                 )
                 ORDER BY ss.updated_at DESC"
            )?;
            let result: Vec<(i64, Option<String>)> = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .filter_map(|r| r.ok())
                .collect();
            result
            // conn MutexGuard dropped here ─────────────────────────────────────
        };

        let count = rows.len();

        // Step 2: Create pipeline_run records (separate DB lock acquisition)
        let pipeline_repo = PipelineRepo::new(db);
        for (session_id, content_hash) in rows {
            let _ = pipeline_repo.upsert_pipeline_run(session_id, content_hash.as_deref(), "v3");
        }

        if count > 0 {
            info!("[PIPELINE] Enqueued {} sessions into pipeline", count);
        }
        Ok(count)
    }
}
