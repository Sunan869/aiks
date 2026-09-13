/// Sync Engine: orchestrates provider → hash → state → render → sink pipeline.
///
/// For each discovered session:
/// 1. Check content hash against stored hash
/// 2. If NEW or UPDATED: render to Markdown, send to SiYuan
/// 3. If UNCHANGED: skip
/// 4. Track conflicts (target was manually edited)
/// 5. Handle missing sources (original file deleted)
use std::sync::Arc;

use chrono::Utc;
use tracing::{info, warn, error, instrument};

use crate::config::Config;
use crate::model::hash::compute_session_hash;
use crate::providers::{ProviderRegistry, SessionSummary};
use crate::renderer::MarkdownRenderer;
use crate::sink::{SiYuanSink};
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
    /// Only sync sessions from this source
    pub source_filter: Option<String>,
    /// Print what would be done without actually syncing
    pub dry_run: bool,
    /// Force overwrite conflicted documents
    pub overwrite: bool,
}

/// Statistics for a completed sync run.
#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    pub discovered: usize,
    pub new_count: usize,
    pub updated_count: usize,
    pub unchanged_count: usize,
    pub skipped_count: usize,
    pub conflict_count: usize,
    pub failed_count: usize,
}

pub struct SyncEngine {
    config: Arc<Config>,
}

impl SyncEngine {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    /// Run a full sync cycle.
    #[instrument(skip(self, db, registry, sink))]
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
        let summaries = registry.discover_all().await;
        let mut stats = SyncStats {
            discovered: summaries.len(),
            ..Default::default()
        };

        info!(discovered = summaries.len(), "Discovered sessions");

        // Filter by source if requested
        let summaries: Vec<SessionSummary> = if let Some(source) = &opts.source_filter {
            summaries
                .into_iter()
                .filter(|s| s.source.as_str() == source.as_str())
                .collect()
        } else {
            summaries
        };

        for summary in &summaries {
            // Check minimum message count filter
            if summary.message_count < self.config.content.minimum_messages {
                stats.skipped_count += 1;
                continue;
            }

            let outcome = self
                .sync_session(db, registry, sink, &renderer, summary, opts)
                .await;

            match &outcome {
                SyncOutcome::Created { .. } => stats.new_count += 1,
                SyncOutcome::Updated { .. } => stats.updated_count += 1,
                SyncOutcome::Unchanged => stats.unchanged_count += 1,
                SyncOutcome::Skipped { .. } => stats.skipped_count += 1,
                SyncOutcome::Conflict { .. } => stats.conflict_count += 1,
                SyncOutcome::Failed { error } => {
                    stats.failed_count += 1;
                    warn!(error = %error, session_id = %summary.external_session_id, "Session sync failed");
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

        // Find or create source_session record
        let existing = match source_session_repo.find_by_source_and_id(source, session_id) {
            Ok(e) => e,
            Err(e) => {
                return SyncOutcome::Failed {
                    error: format!("DB error: {}", e),
                };
            }
        };

        // Load the full session to compute hash
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
                    error: format!("load_session failed: {}", e),
                };
            }
        };

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

        if !is_new && !hash_changed && !parser_changed {
            // Update last_seen_at but don't re-sync
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

        // Upsert source_session
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

        if opts.dry_run {
            return if is_new {
                SyncOutcome::Created { doc_id: "[dry-run]".to_string() }
            } else {
                SyncOutcome::Updated { doc_id: "[dry-run]".to_string() }
            };
        }

        // Check for existing SiYuan document
        let existing_doc = match sink.find_document_by_session(source, session_id).await {
            Ok(d) => d,
            Err(e) => {
                warn!(error = %e, "Could not search for existing document");
                None
            }
        };

        // Check for conflicts: target was manually modified
        if let Some(ref doc) = existing_doc {
            if let Ok(target) = sync_target_repo.find(db_session_id, "siyuan") {
                if let Some(target) = target {
                    // Get current target hash by fetching attrs
                    if let Ok(attrs) = sink.get_block_attrs(&doc.id).await {
                        let current_target_hash = attrs.get(crate::sink::siyuan::ATTR_CONTENT_HASH).cloned();
                        let stored_target_hash = target.synced_hash.clone();

                        // If current hash != stored synced hash, user edited the doc
                        if current_target_hash != stored_target_hash && !opts.overwrite {
                            let _ = sync_target_repo.mark_conflict(db_session_id, "siyuan");
                            return SyncOutcome::Conflict { doc_id: doc.id.clone() };
                        }
                    }
                }
            }
        }

        // Render to Markdown
        let markdown = renderer.render(&session);

        // Ensure notebook
        let notebook_id = match sink.ensure_notebook().await {
            Ok(id) => id,
            Err(e) => {
                let _ = sync_target_repo.mark_failed(db_session_id, "siyuan", &e.to_string(), true);
                return SyncOutcome::Failed {
                    error: format!("ensure_notebook: {}", e),
                };
            }
        };

        let doc_path = sink.build_document_path(
            source,
            session_id,
            session.title.as_deref(),
            session.started_at.as_ref(),
        );

        let doc_id = if let Some(doc) = &existing_doc {
            // Update existing document
            if let Err(e) = sink.update_document(&doc.id, &markdown).await {
                let _ = sync_target_repo.mark_failed(db_session_id, "siyuan", &e.to_string(), true);
                return SyncOutcome::Failed {
                    error: format!("update_document: {}", e),
                };
            }
            doc.id.clone()
        } else {
            // Create new document
            match sink.create_document(&notebook_id, &doc_path, &markdown).await {
                Ok(id) => {
                    let _ = sync_target_repo.upsert_pending(db_session_id, "siyuan");
                    id
                }
                Err(e) => {
                    let _ = sync_target_repo.mark_failed(db_session_id, "siyuan", &e.to_string(), true);
                    return SyncOutcome::Failed {
                        error: format!("create_document: {}", e),
                    };
                }
            }
        };

        // Set AIKS metadata attributes
        if let Err(e) = sink
            .set_aiks_attrs(&doc_id, source, session_id, &content_hash, parser_version)
            .await
        {
            warn!(error = %e, "Could not set AIKS attrs");
        }

        // Get target hash (current content hash in SiYuan)
        let target_hash = sink
            .get_block_attrs(&doc_id)
            .await
            .ok()
            .and_then(|attrs| attrs.get(crate::sink::siyuan::ATTR_CONTENT_HASH).cloned())
            .unwrap_or_else(|| content_hash.clone());

        // Update sync_target
        let _ = sync_target_repo.mark_synced(
            db_session_id,
            "siyuan",
            &doc_id,
            &doc_path,
            &content_hash,
            &target_hash,
        );

        if is_new {
            SyncOutcome::Created { doc_id }
        } else {
            SyncOutcome::Updated { doc_id }
        }
    }

    /// Mark sessions that no longer exist in their source as missing.
    pub async fn mark_missing_sessions(
        &self,
        db: &StateDb,
        registry: &ProviderRegistry,
    ) -> anyhow::Result<usize> {
        let source_session_repo = SourceSessionRepo::new(db);
        let all_stored = source_session_repo.list_all()?;
        let all_discovered = registry.discover_all().await;

        // Build a set of currently visible sessions
        let visible: std::collections::HashSet<(String, String)> = all_discovered
            .iter()
            .map(|s| (s.source.as_str().to_string(), s.external_session_id.clone()))
            .collect();

        let mut marked = 0;
        for stored in &all_stored {
            if stored.is_missing {
                continue;
            }
            let key = (stored.source.clone(), stored.external_session_id.clone());
            if !visible.contains(&key) {
                source_session_repo
                    .mark_missing(&stored.source, &stored.external_session_id)?;
                marked += 1;
                info!(
                    source = stored.source,
                    session_id = stored.external_session_id,
                    "Marked session as missing"
                );
            }
        }

        Ok(marked)
    }
}
