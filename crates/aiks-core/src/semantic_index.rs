use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ai::ModelService;
use crate::engine::AiksEngine;
use crate::indexing::{
    EmbeddingProvider, KnowledgeIndexInput, KnowledgeIndexService, SessionIndexInput,
    SessionIndexService,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticIndexRebuildStats {
    pub total: usize,
    pub completed: usize,
    pub knowledge_total: usize,
    pub session_total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub embedded_chunks: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticIndexRebuildProgress {
    pub total: usize,
    pub completed: usize,
    pub knowledge_total: usize,
    pub session_total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub embedded_chunks: usize,
    pub current_kind: Option<String>,
    pub current_id: Option<String>,
}

impl SemanticIndexRebuildStats {
    fn progress(&self, current_kind: Option<&str>, current_id: Option<&str>) -> SemanticIndexRebuildProgress {
        SemanticIndexRebuildProgress {
            total: self.total,
            completed: self.completed,
            knowledge_total: self.knowledge_total,
            session_total: self.session_total,
            succeeded: self.succeeded,
            failed: self.failed,
            embedded_chunks: self.embedded_chunks,
            current_kind: current_kind.map(str::to_string),
            current_id: current_id.map(str::to_string),
        }
    }
}

/// Rebuild all historical semantic vectors with the engine's current embedding
/// configuration. Existing indexing services own chunking, model/dimension
/// identity, vector replacement, and lexical fallback semantics.
///
/// Database rows are materialized before any embedding HTTP await so SQLite
/// locks are never held across network calls. Work is intentionally sequential:
/// a single private embedding server/GPU is the common desktop deployment and
/// bounded throughput is preferable to duplicate model pressure.
pub async fn rebuild_semantic_index<F>(
    engine: &AiksEngine,
    mut on_progress: F,
) -> anyhow::Result<SemanticIndexRebuildStats>
where
    F: FnMut(SemanticIndexRebuildProgress) + Send,
{
    let embedding = engine.embedding_config();
    if !embedding.enabled {
        anyhow::bail!("Semantic search is disabled. Enable Embedding and restart AIKS first.");
    }
    if embedding.base_url.trim().is_empty() {
        anyhow::bail!("Embedding service URL is empty");
    }
    if embedding.model.trim().is_empty() {
        anyhow::bail!("Embedding model is empty");
    }

    let knowledge_rows: Vec<(String, String, String)> = {
        let db = engine.db();
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, siyuan_doc_id, content
             FROM knowledge_item
             WHERE status = 'active'
               AND siyuan_doc_id IS NOT NULL
               AND TRIM(siyuan_doc_id) <> ''
               AND TRIM(content) <> ''
             ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let session_rows: Vec<(i64, String, String, Option<String>, String)> = {
        let db = engine.db();
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT ss.id, ss.external_session_id, ss.source, ss.title, sf.content
             FROM source_session ss
             JOIN session_search_fts sf ON sf.session_id = ss.id
             WHERE ss.is_missing = 0 AND TRIM(sf.content) <> ''
             ORDER BY ss.id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let provider: Arc<dyn EmbeddingProvider> = Arc::new(ModelService::new(
        engine.ai_config().clone(),
        embedding.clone(),
    )?);
    let knowledge_service = KnowledgeIndexService::new(engine.db(), Arc::clone(&provider));
    let session_service = SessionIndexService::new(engine.db(), provider);

    let mut stats = SemanticIndexRebuildStats {
        total: knowledge_rows.len() + session_rows.len(),
        knowledge_total: knowledge_rows.len(),
        session_total: session_rows.len(),
        ..SemanticIndexRebuildStats::default()
    };
    on_progress(stats.progress(None, None));

    for (knowledge_id, siyuan_doc_id, markdown) in knowledge_rows {
        let result = knowledge_service
            .index_document(KnowledgeIndexInput {
                knowledge_id: knowledge_id.clone(),
                siyuan_doc_id,
                markdown,
            })
            .await;

        stats.completed += 1;
        match result {
            Ok(result) => {
                stats.succeeded += 1;
                stats.embedded_chunks += result.embedded_count;
            }
            Err(error) => {
                stats.failed += 1;
                tracing::warn!(
                    knowledge_id = %knowledge_id,
                    error = %error,
                    "[SEMANTIC-INDEX] knowledge rebuild failed"
                );
            }
        }
        on_progress(stats.progress(Some("knowledge"), Some(&knowledge_id)));
    }

    for (session_id, external_id, source, title, normalized_text) in session_rows {
        let display_id = format!("{source}:{external_id}");
        let result = session_service
            .index_session(SessionIndexInput {
                session_id,
                external_id,
                source,
                title,
                normalized_text,
            })
            .await;

        stats.completed += 1;
        match result {
            Ok(result) if result.chunk_count == 0 || result.embedded_count > 0 => {
                stats.succeeded += 1;
                stats.embedded_chunks += result.embedded_count;
            }
            Ok(result) => {
                stats.failed += 1;
                tracing::warn!(
                    session_id,
                    chunk_count = result.chunk_count,
                    "[SEMANTIC-INDEX] session rebuilt lexically but embedding failed"
                );
            }
            Err(error) => {
                stats.failed += 1;
                tracing::warn!(
                    session_id,
                    error = %error,
                    "[SEMANTIC-INDEX] session rebuild failed"
                );
            }
        }
        on_progress(stats.progress(Some("session"), Some(&display_id)));
    }

    Ok(stats)
}
