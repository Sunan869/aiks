/// Embedding Stage — chunks knowledge items and embeds them
///
/// Two sub-stages:
/// 1. EmbedChunk: split knowledge content into 800-token chunks
/// 2. Embed: call embedding API and store vectors

use tracing::{info, warn};
use std::time::Instant;

use crate::pipeline::embedding_client::{EmbeddingClient, EmbeddingConfig};
use crate::pipeline::knowledge_repo::KnowledgeRepo;
use crate::pipeline::repo::PipelineRepo;
use crate::storage::StateDb;

const CHARS_PER_TOKEN: f64 = 3.5;

pub struct EmbeddingStage {
    client: EmbeddingClient,
}

impl EmbeddingStage {
    pub fn new(config: EmbeddingConfig) -> anyhow::Result<Self> {
        Ok(Self { client: EmbeddingClient::new(config)? })
    }

    /// Chunk all knowledge items for a session into embedding-sized pieces
    pub fn chunk_knowledge(
        db: &StateDb,
        pipeline_run_id: &str,
        session_id: i64,
        target_tokens: usize,
        overlap_tokens: usize,
    ) -> anyhow::Result<usize> {
        let knowledge_repo = KnowledgeRepo::new(db);
        let pipeline_repo = PipelineRepo::new(db);

        let items = knowledge_repo.get_by_session(session_id)?;
        let mut total_chunks = 0;

        for item in &items {
            let chunks = split_into_chunks(&item.content, target_tokens, overlap_tokens);
            let chunk_pairs: Vec<(Option<String>, String)> = chunks
                .into_iter()
                .map(|text| (None, text))
                .collect();
            let chunk_count = chunk_pairs.len();
            knowledge_repo.save_embedding_chunks(&item.id, &chunk_pairs)?;
            total_chunks += chunk_count;
        }

        pipeline_repo.record_stage(
            pipeline_run_id, "EMBED_CHUNKED", "SUCCESS",
            Some(items.len() as i32), Some(total_chunks as i32), None,
            Some(&serde_json::json!({"knowledge_items": items.len(), "chunks": total_chunks})),
            None,
        )?;

        info!(session_id, items = items.len(), chunks = total_chunks, "[EMBED_CHUNK] Complete");
        Ok(total_chunks)
    }

    /// Embed all chunks for knowledge items of a session
    pub async fn embed_knowledge(
        &self,
        db: &StateDb,
        pipeline_run_id: &str,
        session_id: i64,
    ) -> anyhow::Result<usize> {
        let knowledge_repo = KnowledgeRepo::new(db);
        let pipeline_repo = PipelineRepo::new(db);
        let t0 = Instant::now();

        let items = knowledge_repo.get_by_session(session_id)?;
        let mut total_embedded = 0;
        let mut total_failed = 0;
        let model = self.client.config.model.clone();
        let dimensions = self.client.config.dimensions.unwrap_or(1024) as i32;
        let batch_size = self.client.config.batch_size;

        for item in &items {
            // Get chunks without embeddings
            let chunks = {
                let conn = db.conn();
                let mut stmt = conn.prepare(
                    "SELECT kc.id, kc.text FROM knowledge_chunk kc
                     LEFT JOIN embedding_record er ON er.chunk_id = kc.id AND er.model = ?1
                     WHERE kc.knowledge_id = ?2 AND er.id IS NULL
                     ORDER BY kc.chunk_index"
                )?;
                let rows: Vec<(String, String)> = stmt
                    .query_map(rusqlite::params![model, item.id], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                rows
            };

            if chunks.is_empty() { continue; }

            // Process in batches
            for batch in chunks.chunks(batch_size) {
                let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
                match self.client.embed_batch(texts).await {
                    Ok(embeddings) => {
                        for ((chunk_id, _), vector) in batch.iter().zip(embeddings.iter()) {
                            if vector.is_empty() { total_failed += 1; continue; }
                            knowledge_repo.save_embedding(chunk_id, &model, dimensions, vector)?;
                            total_embedded += 1;
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "[EMBED] Batch failed");
                        total_failed += batch.len();
                    }
                }
            }
        }

        let latency_ms = t0.elapsed().as_millis() as i64;
        info!(
            session_id, embedded = total_embedded, failed = total_failed,
            latency_ms, "[EMBED] Complete"
        );

        let total_attempted = total_embedded + total_failed;
        if total_failed > 0 {
            let error = format!(
                "{} of {} embedding chunk(s) failed",
                total_failed, total_attempted
            );
            pipeline_repo.record_stage(
                pipeline_run_id, "EMBEDDED", "FAILED",
                Some(total_attempted as i32),
                Some(total_embedded as i32),
                Some(latency_ms),
                Some(&serde_json::json!({"embedded": total_embedded, "failed": total_failed})),
                Some(&error),
            )?;
            anyhow::bail!(error);
        }

        pipeline_repo.record_stage(
            pipeline_run_id, "EMBEDDED", "SUCCESS",
            Some(total_attempted as i32),
            Some(total_embedded as i32),
            Some(latency_ms),
            Some(&serde_json::json!({"embedded": total_embedded, "failed": total_failed})),
            None,
        )?;

        Ok(total_embedded)
    }
}

/// Split text into overlapping chunks (simple sliding window by chars)
fn split_into_chunks(text: &str, target_tokens: usize, overlap_tokens: usize) -> Vec<String> {
    let target_chars = (target_tokens as f64 * CHARS_PER_TOKEN) as usize;
    let overlap_chars = (overlap_tokens as f64 * CHARS_PER_TOKEN) as usize;

    if text.len() <= target_chars {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    let chars: Vec<char> = text.chars().collect();

    while start < chars.len() {
        let end = (start + target_chars).min(chars.len());
        let chunk: String = chars[start..end].iter().collect();
        chunks.push(chunk);
        if end >= chars.len() { break; }
        start = end.saturating_sub(overlap_chars);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_short_text_is_one_chunk() {
        let text = "short text";
        let chunks = split_into_chunks(text, 800, 120);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn split_long_text_creates_chunks() {
        let text = "x".repeat(10000);
        let chunks = split_into_chunks(&text, 100, 10);
        assert!(chunks.len() > 1);
        // Each chunk should have overlap with the next
        for i in 1..chunks.len() {
            let overlap_chars = (10_f64 * CHARS_PER_TOKEN) as usize;
            let prev_end: Vec<char> = chunks[i-1].chars().collect();
            let prev_tail: String = prev_end[prev_end.len().saturating_sub(overlap_chars)..].iter().collect();
            assert!(chunks[i].starts_with(&prev_tail[..prev_tail.len().min(20)]));
        }
    }
}
