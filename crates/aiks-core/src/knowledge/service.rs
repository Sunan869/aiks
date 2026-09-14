/// Knowledge Extraction Service — manages the extraction queue and state.
///
/// Spec §31: Extraction happens in a background queue, never blocking sync.
/// Spec §30: AI failure never affects Raw Session sync.
use std::sync::Arc;

use chrono::Utc;
use rusqlite::params;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::ai::{config::AiModelConfig, extractor::KnowledgeExtractor};
use crate::knowledge::{
    model::{ExtractionStats, ExtractionStatus},
    renderer::KnowledgeRenderer,
};
use crate::model::NormalizedSession;
use crate::sink::SiYuanSink;
use crate::storage::StateDb;

pub const KNOWLEDGE_NOTEBOOK: &str = "AI Knowledge";

/// Request to extract knowledge from a session
#[derive(Debug, Clone)]
pub struct ExtractionRequest {
    pub source: String,
    pub session_id: String,
    pub content_hash: String,
}

/// Result of a single extraction
#[derive(Debug)]
pub enum ExtractionOutcome {
    Success { doc_id: String, score: f64 },
    Skipped { score: f64 },
    Failed { error: String },
}

/// The extraction service manages the queue and state.
pub struct ExtractionService {
    config: AiModelConfig,
    tx: mpsc::UnboundedSender<ExtractionRequest>,
}

impl ExtractionService {
    /// Create a new service and start the background worker.
    pub fn start(
        config: AiModelConfig,
        db: Arc<StateDb>,
        siyuan_base_url: String,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let svc_config = config.clone();

        tokio::spawn(async move {
            run_extraction_worker(rx, svc_config, db, siyuan_base_url).await;
        });

        Self { config, tx }
    }

    /// Enqueue a session for extraction (non-blocking).
    pub fn enqueue(&self, req: ExtractionRequest) {
        if self.config.enabled {
            let _ = self.tx.send(req);
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn config(&self) -> &AiModelConfig {
        &self.config
    }
}

/// Background extraction worker loop
async fn run_extraction_worker(
    mut rx: mpsc::UnboundedReceiver<ExtractionRequest>,
    config: AiModelConfig,
    db: Arc<StateDb>,
    siyuan_base_url: String,
) {
    info!("Knowledge extraction worker started");

    while let Some(req) = rx.recv().await {
        let _extractor = match KnowledgeExtractor::new(config.clone()) {
            Ok(e) => e,
            Err(e) => {
                error!(error = %e, "Cannot create extractor");
                continue;
            }
        };

        let _sink = match SiYuanSink::embedded(&siyuan_base_url, KNOWLEDGE_NOTEBOOK) {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "Cannot create SiYuan sink for extraction");
                continue;
            }
        };

        // Load the session from providers (skip for now - we work from DB records)
        // In a full implementation, we'd load the NormalizedSession here
        // For now, log and mark as pending for manual extraction
        info!(
            source = %req.source,
            session_id = %req.session_id,
            "Processing extraction request"
        );

        // Mark as running
        set_extraction_status(&db, &req.source, &req.session_id, &req.content_hash, ExtractionStatus::Running, None, None, &config);
    }

    info!("Knowledge extraction worker stopped");
}

/// Perform extraction for a specific session (called explicitly)
pub async fn extract_session(
    session: &NormalizedSession,
    config: &AiModelConfig,
    sink: &SiYuanSink,
    _db: &StateDb,
) -> ExtractionOutcome {
    if !config.enabled {
        return ExtractionOutcome::Skipped { score: 0.0 };
    }

    let extractor = match KnowledgeExtractor::new(config.clone()) {
        Ok(e) => e,
        Err(e) => return ExtractionOutcome::Failed { error: e.to_string() },
    };

    // Run extraction
    let doc = match extractor.extract(session).await {
        Ok(d) => d,
        Err(e) => {
            warn!(error = %e, session_id = %session.external_session_id, "Extraction failed");
            return ExtractionOutcome::Failed { error: e.to_string() };
        }
    };

    let score = doc.knowledge_score;

    // Filter by minimum score
    if !doc.worth_extracting || score < config.min_knowledge_score as f64 {
        info!(score, session_id = %session.external_session_id, "Session skipped (score below threshold)");
        return ExtractionOutcome::Skipped { score };
    }

    // Render to Markdown
    let markdown = KnowledgeRenderer::render(&doc, session.source, &session.external_session_id, None);
    let doc_path = KnowledgeRenderer::build_doc_path(&doc, session.source, &session.external_session_id);

    // Ensure notebook
    let notebook_id = match sink.ensure_notebook().await {
        Ok(id) => id,
        Err(e) => return ExtractionOutcome::Failed { error: format!("notebook: {}", e) },
    };

    // Create/update SiYuan document
    let doc_id = match sink.create_document(&notebook_id, &doc_path, &markdown).await {
        Ok(id) => id,
        Err(e) => return ExtractionOutcome::Failed { error: format!("create doc: {}", e) },
    };

    // Set attributes
    let now = Utc::now().to_rfc3339();
    let mut attrs = std::collections::HashMap::new();
    attrs.insert("custom-aiks-managed".to_string(), "true".to_string());
    attrs.insert("custom-aiks-type".to_string(), "knowledge".to_string());
    attrs.insert("custom-aiks-source-session-id".to_string(), session.external_session_id.clone());
    attrs.insert("custom-aiks-source".to_string(), session.source.as_str().to_string());
    attrs.insert("custom-aiks-category".to_string(), doc.category.clone());
    attrs.insert("custom-aiks-score".to_string(), format!("{:.2}", score));
    attrs.insert("custom-aiks-synced-at".to_string(), now);
    let _ = sink.set_block_attrs(&doc_id, &attrs).await;

    info!(doc_id = %doc_id, score, category = %doc.category, "Knowledge extracted and saved");
    ExtractionOutcome::Success { doc_id, score }
}

/// Get knowledge extraction statistics
pub fn get_extraction_stats(db: &StateDb) -> anyhow::Result<ExtractionStats> {
    let conn = db.conn();
    let mut stats = ExtractionStats::default();

    // Check if the table exists first
    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='knowledge_extraction'",
        [],
        |row| row.get::<_, i64>(0),
    ).unwrap_or(0) > 0;

    if !table_exists {
        return Ok(stats);
    }

    let mut stmt = conn.prepare(
        "SELECT status, COUNT(*) FROM knowledge_extraction GROUP BY status"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
    })?;

    for row in rows.flatten() {
        let (status, count) = row;
        stats.total += count;
        match ExtractionStatus::from_str(&status) {
            ExtractionStatus::Success => stats.success += count,
            ExtractionStatus::Skipped => stats.skipped += count,
            ExtractionStatus::Failed => stats.failed += count,
            ExtractionStatus::Pending | ExtractionStatus::Stale => stats.pending += count,
            ExtractionStatus::Running => stats.running += count,
        }
    }

    Ok(stats)
}

/// Upsert an extraction record
pub fn set_extraction_status(
    db: &StateDb,
    source: &str,
    session_id: &str,
    content_hash: &str,
    status: ExtractionStatus,
    knowledge_doc_id: Option<&str>,
    error: Option<&str>,
    config: &AiModelConfig,
) {
    let now = Utc::now().to_rfc3339();
    let _ = db.conn().execute(
        "INSERT INTO knowledge_extraction
         (source, external_session_id, source_content_hash, extractor_version, prompt_version,
          model, model_endpoint, status, knowledge_document_id, error_message, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
         ON CONFLICT(source, external_session_id) DO UPDATE SET
           source_content_hash = excluded.source_content_hash,
           status = excluded.status,
           knowledge_document_id = COALESCE(excluded.knowledge_document_id, knowledge_document_id),
           error_message = excluded.error_message,
           updated_at = excluded.updated_at",
        params![
            source,
            session_id,
            content_hash,
            crate::ai::extractor::EXTRACTOR_VERSION,
            crate::ai::prompts::PROMPT_VERSION,
            config.model,
            config.base_url,
            status.as_str(),
            knowledge_doc_id,
            error,
            now
        ],
    );
}
