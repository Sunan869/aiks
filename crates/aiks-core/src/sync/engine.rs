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
use crate::storage::{SourceSessionRepo, StateDb, SyncRunRepo, SyncStatus, SyncTargetRepo};

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

/// Exact canonical identity of a session that should enter the knowledge pipeline.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtractionCandidate {
    pub session_id: i64,
    pub source: String,
    pub external_session_id: String,
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
    /// Canonical sessions newly created/updated — eligible for extraction.
    pub extraction_candidates: Vec<ExtractionCandidate>,
}

/// R05: stable hash over SiYuan's exported markdown — the conflict baseline.
fn hash_markdown(md: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("md:{}", hex::encode(Sha256::digest(md.as_bytes())))
}

/// Persist a trustworthy remote-content baseline without marking the whole sync
/// successful yet. This is intentionally separate from `mark_synced`: attrs can
/// still fail after the document write, and a retry must be able to prove that
/// the mapped document is the same AIKS write instead of recreating it.
fn record_target_hash(db: &StateDb, session_id: i64, target_hash: &str) -> anyhow::Result<()> {
    use rusqlite::params;
    db.conn().execute(
        "UPDATE sync_target SET target_hash = ?3 WHERE session_id = ?1 AND sink = ?2",
        params![session_id, "siyuan", target_hash],
    )?;
    Ok(())
}

struct SyncRunContext<'a> {
    renderer: &'a MarkdownRenderer,
    notebook_id: Option<&'a str>,
}

pub struct SyncEngine {
    config: Arc<Config>,
}

impl SyncEngine {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    fn record_extraction_candidate(
        &self,
        db: &StateDb,
        summary: &SessionSummary,
        opts: &SyncOptions,
        stats: &mut SyncStats,
    ) -> Option<ExtractionCandidate> {
        // A dry-run must never create executable follow-up work. New dry-run
        // sessions do not even have a canonical source_session row yet.
        if opts.dry_run {
            return None;
        }

        match SourceSessionRepo::new(db)
            .find_by_source_and_id(summary.source.as_str(), &summary.external_session_id)
        {
            Ok(Some(session)) => {
                let candidate = ExtractionCandidate {
                    session_id: session.id,
                    source: session.source,
                    external_session_id: session.external_session_id,
                };
                stats.extraction_candidates.push(candidate.clone());
                Some(candidate)
            }
            Ok(None) => {
                warn!(
                    source = %summary.source.as_str(),
                    external_session_id = %summary.external_session_id,
                    "[PIPELINE] Synced session missing canonical row; not enqueueing extraction"
                );
                None
            }
            Err(e) => {
                warn!(
                    source = %summary.source.as_str(),
                    external_session_id = %summary.external_session_id,
                    error = %e,
                    "[PIPELINE] Failed to resolve canonical extraction candidate"
                );
                None
            }
        }
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
        self.run_sync_with_candidate_handler(db, registry, sink, opts, |_| {})
            .await
    }

    /// Run a full sync cycle and synchronously notify the caller as soon as
    /// each Created/Updated session becomes eligible for the knowledge pipeline.
    pub async fn run_sync_with_candidate_handler<F>(
        &self,
        db: &StateDb,
        registry: &ProviderRegistry,
        sink: &SiYuanSink,
        opts: &SyncOptions,
        mut on_candidate: F,
    ) -> anyhow::Result<SyncStats>
    where
        F: FnMut(&ExtractionCandidate),
    {
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

        let session_notebook_id = if opts.dry_run {
            None
        } else {
            match sink.ensure_session_notebook().await {
                Ok(id) => Some(id),
                Err(e) => {
                    error!(error = %e, "[SYNC] Failed to resolve session notebook");
                    None
                }
            }
        };

        let sync_context = SyncRunContext {
            renderer: &renderer,
            notebook_id: session_notebook_id.as_deref(),
        };

        for summary in &summaries {
            // Filter: only skip if we KNOW message count is low and it's > 0
            // (Codex from state_5.sqlite has count=0 meaning "unknown" — don't skip those)
            let min_msgs = self.config.content.minimum_messages;
            if min_msgs > 0 && summary.message_count > 0 && summary.message_count < min_msgs {
                debug!(
                    session_id = %summary.external_session_id,
                    message_count = summary.message_count,
                    "Skipping session (below minimum_messages threshold)"
                );
                stats.skipped_count += 1;
                continue;
            }

            let outcome = self
                .sync_session(db, registry, sink, &sync_context, summary, opts)
                .await;

            match &outcome {
                SyncOutcome::Created { doc_id } => {
                    info!(
                        session_id = %summary.external_session_id,
                        doc_id = %doc_id,
                        "[SYNC] Created"
                    );
                    stats.new_count += 1;
                    if let Some(candidate) =
                        self.record_extraction_candidate(db, summary, opts, &mut stats)
                    {
                        on_candidate(&candidate);
                    }
                }
                SyncOutcome::Updated { doc_id } => {
                    info!(
                        session_id = %summary.external_session_id,
                        doc_id = %doc_id,
                        "[SYNC] Updated"
                    );
                    stats.updated_count += 1;
                    if let Some(candidate) =
                        self.record_extraction_candidate(db, summary, opts, &mut stats)
                    {
                        on_candidate(&candidate);
                    }
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
                    if !opts.dry_run && !sink.health_check().await {
                        error!("[SYNC] SiYuan unavailable; aborting remaining sessions");
                        break;
                    }
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
        context: &SyncRunContext<'_>,
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
                return SyncOutcome::Failed {
                    error: format!("DB lookup: {}", e),
                };
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
                reason: format!(
                    "Only {} messages (min: {})",
                    session.messages.len(),
                    min_msgs
                ),
            };
        }

        let content_hash = compute_session_hash(&session);
        let parser_version = provider.parser_version();
        let is_new = existing.is_none();

        // R03: load the locally persisted sync target. It holds the mapping to
        // the remote document, which is the source of truth for create-vs-update.
        let existing_target = match existing.as_ref() {
            Some(e) => sync_target_repo.find(e.id, "siyuan").ok().flatten(),
            None => None,
        };

        // A permanent safety failure for an unchanged source payload cannot be
        // repaired by retrying the same write. Skip it until the source content
        // or parser version changes, at which point a new attempt is allowed.
        if let (Some(existing), Some(target)) = (existing.as_ref(), existing_target.as_ref()) {
            if target.status == SyncStatus::FailedPermanent
                && existing.content_hash.as_deref() == Some(&content_hash)
                && existing.parser_version.as_deref() == Some(parser_version)
            {
                return SyncOutcome::Skipped {
                    reason: "previous permanent SiYuan write failure for unchanged source"
                        .to_string(),
                };
            }
        }

        // B03 + R04: UNCHANGED only when the *confirmed* target matches the
        // current content hash AND the parser version. The hash stored on the
        // source row alone is not enough (dry-run or a stale target must not
        // swallow a pending update).
        if !is_new {
            if let Some(target) = &existing_target {
                if matches!(target.status, SyncStatus::Synced | SyncStatus::Unchanged)
                    && target.synced_hash.as_deref() == Some(&content_hash)
                    && existing.as_ref().map(|e| e.parser_version.as_deref())
                        == Some(Some(parser_version))
                {
                    // Refresh observed metadata only — hash is unchanged, safe.
                    let source_updated_at = session.updated_at.map(|t| t.to_rfc3339());
                    let _ = source_session_repo.upsert(
                        source,
                        session_id,
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
            }
        }

        // R04: dry-run must not advance any state that a later real sync
        // depends on (no source upsert, no target change, no SiYuan call).
        if opts.dry_run {
            return if is_new {
                SyncOutcome::Created {
                    doc_id: "[dry-run]".to_string(),
                }
            } else {
                SyncOutcome::Updated {
                    doc_id: "[dry-run]".to_string(),
                }
            };
        }

        // Persist session metadata. The content_hash here is the observed hash.
        // SYNCED status is only set in sync_target after confirmed remote write.
        let source_updated_at = session.updated_at.map(|t| t.to_rfc3339());
        let db_session_id = match source_session_repo.upsert(
            source,
            session_id,
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
                return SyncOutcome::Failed {
                    error: format!("upsert source_session: {}", e),
                };
            }
        };

        // R03: resolve the existing remote document from the local mapping.
        let existing_doc: Option<crate::sink::siyuan::DocumentInfo> = existing_target
            .as_ref()
            .filter(|t| {
                t.target_id
                    .as_deref()
                    .map(|id| !id.is_empty())
                    .unwrap_or(false)
            })
            .map(|t| crate::sink::siyuan::DocumentInfo {
                id: t.target_id.clone().unwrap_or_default(),
                path: t.target_path.clone().unwrap_or_default(),
                content_hash: t.synced_hash.clone(),
                parser_version: None,
            });

        // A mapped target is safe to update only after proving its remote
        // content still matches a trustworthy baseline. When a baseline exists,
        // a successful Kramdown read already proves the document exists, so no
        // redundant SQL existence probe is needed. Legacy rows without a
        // baseline are probed only to distinguish protected-existing from
        // definitely-missing.
        let mut remote_missing = false;
        if let (Some(doc), Some(target)) = (&existing_doc, existing_target.as_ref()) {
            if !opts.overwrite {
                if let Some(baseline) = target.target_hash.as_ref() {
                    match sink.get_document_markdown(&doc.id).await {
                        Ok(remote_md) => {
                            let remote_hash = hash_markdown(&remote_md);
                            if baseline != &remote_hash {
                                let _ = sync_target_repo.mark_conflict(db_session_id, "siyuan");
                                return SyncOutcome::Conflict {
                                    doc_id: doc.id.clone(),
                                };
                            }
                        }
                        Err(read_error) => match sink.get_doc_notebook(&doc.id).await {
                            Ok(None) => {
                                remote_missing = true;
                            }
                            Ok(Some(_)) => {
                                let msg = format!(
                                    "verify remote SiYuan document {} before update: {}",
                                    doc.id, read_error
                                );
                                let _ = sync_target_repo.mark_failed(
                                    db_session_id,
                                    "siyuan",
                                    &msg,
                                    true,
                                );
                                return SyncOutcome::Failed { error: msg };
                            }
                            Err(probe_error) => {
                                let msg = format!(
                                    "verify remote SiYuan document {} before update: {}; existence check: {}",
                                    doc.id, read_error, probe_error
                                );
                                let _ = sync_target_repo.mark_failed(
                                    db_session_id,
                                    "siyuan",
                                    &msg,
                                    true,
                                );
                                return SyncOutcome::Failed { error: msg };
                            }
                        },
                    }

                    if !remote_missing {
                        // Secondary check: managed attribute was tampered with.
                        if let Ok(attrs) = sink.get_block_attrs(&doc.id).await {
                            let current =
                                attrs.get(crate::sink::siyuan::ATTR_CONTENT_HASH).cloned();
                            if let Some(cur) = current {
                                if target
                                    .synced_hash
                                    .as_deref()
                                    .map(|h| h != cur)
                                    .unwrap_or(false)
                                {
                                    let _ = sync_target_repo.mark_conflict(db_session_id, "siyuan");
                                    return SyncOutcome::Conflict {
                                        doc_id: doc.id.clone(),
                                    };
                                }
                            }
                        }
                    }
                } else {
                    match sink.get_doc_notebook(&doc.id).await {
                        Ok(Some(_)) => {
                            warn!(doc_id = %doc.id,
                                "Mapped SiYuan document has no remote baseline; protecting it as a conflict");
                            let _ = sync_target_repo.mark_conflict(db_session_id, "siyuan");
                            return SyncOutcome::Conflict {
                                doc_id: doc.id.clone(),
                            };
                        }
                        Ok(None) => {
                            remote_missing = true;
                        }
                        Err(e) => {
                            let msg = format!(
                                "verify mapped SiYuan document {} exists before update: {}",
                                doc.id, e
                            );
                            let _ =
                                sync_target_repo.mark_failed(db_session_id, "siyuan", &msg, true);
                            return SyncOutcome::Failed { error: msg };
                        }
                    }
                }
            }
        }

        // Render to Markdown
        let markdown = context.renderer.render(&session);

        let notebook_id = match context.notebook_id {
            Some(id) => id,
            None => {
                return SyncOutcome::Failed {
                    error: "session notebook was not resolved".to_string(),
                };
            }
        };

        let doc_path = sink.build_document_path(
            source,
            session_id,
            session.title.as_deref(),
            session.started_at.as_ref(),
        );

        // Create or update document. An update error is never enough evidence
        // to recreate: only a successful follow-up existence check returning
        // None proves the old document is gone.
        let doc_id = if let Some(doc) = &existing_doc {
            if remote_missing {
                let _ = sync_target_repo.upsert_pending(db_session_id, "siyuan");
                match sink
                    .create_document(notebook_id, &doc_path, &markdown)
                    .await
                {
                    Ok(id) => id,
                    Err(e) => {
                        let retryable = SiYuanSink::is_retryable_write_error(&e);
                        let _ = sync_target_repo.mark_failed(
                            db_session_id,
                            "siyuan",
                            &e.to_string(),
                            retryable,
                        );
                        return SyncOutcome::Failed {
                            error: format!("create_document (remote confirmed missing): {}", e),
                        };
                    }
                }
            } else {
                match sink.update_document(&doc.id, &markdown).await {
                    Ok(()) => doc.id.clone(),
                    Err(update_error) => {
                        if !SiYuanSink::is_retryable_write_error(&update_error) {
                            let msg = update_error.to_string();
                            let _ =
                                sync_target_repo.mark_failed(db_session_id, "siyuan", &msg, false);
                            return SyncOutcome::Failed {
                                error: format!("update_document: {}", update_error),
                            };
                        }

                        match sink.get_doc_notebook(&doc.id).await {
                            Ok(None) => {
                                warn!(doc_id = %doc.id, error = %update_error,
                                    "update_document failed and remote document is confirmed missing; recreating");
                                let _ = sync_target_repo.upsert_pending(db_session_id, "siyuan");
                                match sink
                                    .create_document(notebook_id, &doc_path, &markdown)
                                    .await
                                {
                                    Ok(id) => id,
                                    Err(create_error) => {
                                        let msg = format!(
                                            "create_document after confirmed remote deletion: {}",
                                            create_error
                                        );
                                        let retryable =
                                            SiYuanSink::is_retryable_write_error(&create_error);
                                        let _ = sync_target_repo.mark_failed(
                                            db_session_id,
                                            "siyuan",
                                            &msg,
                                            retryable,
                                        );
                                        return SyncOutcome::Failed { error: msg };
                                    }
                                }
                            }
                            Ok(Some(_)) => {
                                let msg = format!(
                                    "update_document {} failed while remote document still exists: {}",
                                    doc.id, update_error
                                );
                                let _ = sync_target_repo.mark_failed(
                                    db_session_id,
                                    "siyuan",
                                    &msg,
                                    true,
                                );
                                return SyncOutcome::Failed { error: msg };
                            }
                            Err(probe_error) => {
                                let msg = format!(
                                    "update_document {} failed and remote existence is unknown: {}; existence check: {}",
                                    doc.id, update_error, probe_error
                                );
                                let _ = sync_target_repo.mark_failed(
                                    db_session_id,
                                    "siyuan",
                                    &msg,
                                    true,
                                );
                                return SyncOutcome::Failed { error: msg };
                            }
                        }
                    }
                }
            }
        } else {
            let _ = sync_target_repo.upsert_pending(db_session_id, "siyuan");
            match sink
                .create_document(notebook_id, &doc_path, &markdown)
                .await
            {
                Ok(id) => id,
                Err(e) => {
                    let retryable = SiYuanSink::is_retryable_write_error(&e);
                    let _ = sync_target_repo.mark_failed(
                        db_session_id,
                        "siyuan",
                        &e.to_string(),
                        retryable,
                    );
                    return SyncOutcome::Failed {
                        error: format!("create_document: {}", e),
                    };
                }
            }
        };

        // Persist the doc mapping immediately. A later failure must retry this
        // exact remote document rather than creating an orphan duplicate.
        if let Err(e) =
            sync_target_repo.record_target_doc(db_session_id, "siyuan", &doc_id, &doc_path)
        {
            return SyncOutcome::Failed {
                error: format!("record_target_doc: {}", e),
            };
        }

        // Capture and persist the remote baseline BEFORE attrs. If attrs fail,
        // the retry can still prove this mapped document is the AIKS write and
        // safely update it. Baseline capture itself fails closed.
        let target_hash = match sink.get_document_markdown(&doc_id).await {
            Ok(remote_md) => hash_markdown(&remote_md),
            Err(e) => {
                let msg = format!("capture remote baseline for {}: {}", doc_id, e);
                let _ = sync_target_repo.mark_failed(db_session_id, "siyuan", &msg, true);
                return SyncOutcome::Failed { error: msg };
            }
        };
        if let Err(e) = record_target_hash(db, db_session_id, &target_hash) {
            let msg = format!("record remote baseline for {}: {}", doc_id, e);
            let _ = sync_target_repo.mark_failed(db_session_id, "siyuan", &msg, true);
            return SyncOutcome::Failed { error: msg };
        }

        // R05: Set AIKS metadata attributes — a failure here is FATAL for this
        // session. The mapping and baseline are already durable, so the retry
        // path verifies and updates the same document.
        if let Err(e) = sink
            .set_aiks_attrs(&doc_id, source, session_id, &content_hash, parser_version)
            .await
        {
            let msg = format!("set_aiks_attrs: {}", e);
            let _ = sync_target_repo.mark_failed(db_session_id, "siyuan", &msg, true);
            return SyncOutcome::Failed { error: msg };
        }

        // R05: mark_synced errors must not be silently ignored — the run would
        // report success while the state says otherwise.
        if let Err(e) = sync_target_repo.mark_synced(
            db_session_id,
            "siyuan",
            &doc_id,
            &doc_path,
            &content_hash,
            Some(&target_hash),
        ) {
            return SyncOutcome::Failed {
                error: format!("mark_synced: {}", e),
            };
        }

        if is_new {
            SyncOutcome::Created { doc_id }
        } else {
            SyncOutcome::Updated { doc_id }
        }
    }

    /// Mark sessions as missing when source files are deleted.
    ///
    /// R14: only judge "missing" for sources whose provider scan actually
    /// SUCCEEDED. A provider failure (permissions, path error, temporary
    /// outage) previously produced an empty result which marked every stored
    /// session of that source as missing — a false deletion report.
    pub async fn mark_missing_sessions(
        &self,
        db: &StateDb,
        registry: &ProviderRegistry,
    ) -> anyhow::Result<usize> {
        let source_session_repo = SourceSessionRepo::new(db);
        let all_stored = source_session_repo.list_all()?;
        let per_source = registry.discover_all_detailed().await;

        let mut visible: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut scanned_sources: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for (source, result) in per_source {
            match result {
                Ok(sessions) => {
                    scanned_sources.insert(source.as_str().to_string());
                    for s in sessions {
                        visible.insert((source.as_str().to_string(), s.external_session_id));
                    }
                }
                Err(e) => {
                    warn!(
                        source = %source.as_str(),
                        error = %e,
                        "[SYNC] Provider scan failed — skipping missing-detection for this source"
                    );
                }
            }
        }

        let mut marked = 0;
        for stored in &all_stored {
            if stored.is_missing {
                continue;
            }
            // R14: only judge sources that were successfully scanned this round.
            if !scanned_sources.contains(&stored.source) {
                continue;
            }
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
                 ORDER BY ss.updated_at DESC",
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
