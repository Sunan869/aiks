use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ai::ModelService;
use crate::storage::StateDb;

const CHARS_PER_TOKEN: f64 = 3.5;
const DEFAULT_CHUNK_TARGET_TOKENS: usize = 800;
const DEFAULT_CHUNK_OVERLAP_TOKENS: usize = 120;
const DEFAULT_BATCH_SIZE: usize = 16;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn enabled(&self) -> bool;
    fn model_name(&self) -> &str;
    fn dimensions(&self) -> Option<usize>;

    fn batch_size(&self) -> usize {
        DEFAULT_BATCH_SIZE
    }

    fn chunk_target_tokens(&self) -> usize {
        DEFAULT_CHUNK_TARGET_TOKENS
    }

    fn chunk_overlap_tokens(&self) -> usize {
        DEFAULT_CHUNK_OVERLAP_TOKENS
    }

    async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>>;
}

#[async_trait]
impl EmbeddingProvider for ModelService {
    fn enabled(&self) -> bool {
        self.embedding_config().enabled
    }

    fn model_name(&self) -> &str {
        &self.embedding_config().model
    }

    fn dimensions(&self) -> Option<usize> {
        self.embedding_config().dimensions
    }

    fn batch_size(&self) -> usize {
        self.embedding_config().batch_size.max(1)
    }

    fn chunk_target_tokens(&self) -> usize {
        self.embedding_config().chunk_target_tokens.max(1)
    }

    fn chunk_overlap_tokens(&self) -> usize {
        self.embedding_config().chunk_overlap_tokens
    }

    async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        ModelService::embed_texts(self, texts).await
    }
}

#[derive(Debug, Clone)]
pub struct KnowledgeIndexInput {
    pub knowledge_id: String,
    pub siyuan_doc_id: String,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeIndexResult {
    pub content_hash: String,
    pub chunk_count: usize,
    pub embedded_count: usize,
    pub skipped: bool,
}

pub struct KnowledgeIndexService {
    db: Arc<StateDb>,
    embeddings: Arc<dyn EmbeddingProvider>,
}

impl KnowledgeIndexService {
    pub fn new(db: Arc<StateDb>, embeddings: Arc<dyn EmbeddingProvider>) -> Self {
        Self { db, embeddings }
    }

    pub async fn index_document(
        &self,
        input: KnowledgeIndexInput,
    ) -> anyhow::Result<KnowledgeIndexResult> {
        let knowledge_id = required_text("knowledge_id", &input.knowledge_id)?;
        let siyuan_doc_id = required_text("siyuan_doc_id", &input.siyuan_doc_id)?;
        let content_hash = sha256_hex(input.markdown.as_bytes());
        let embedding_enabled = self.embeddings.enabled();
        let embedding_model = embedding_enabled.then(|| self.embeddings.model_name().to_string());
        let configured_dimensions = if embedding_enabled {
            self.embeddings.dimensions()
        } else {
            None
        };

        let metadata = {
            let conn = self.db.conn();
            conn.query_row(
                "SELECT title, summary, tags, index_status, indexed_hash,
                        embedding_model, embedding_dimensions, index_chunk_count
                 FROM knowledge_item
                 WHERE id = ?1 AND siyuan_doc_id = ?2",
                params![knowledge_id, siyuan_doc_id],
                |row| {
                    Ok(IndexMetadata {
                        title: row.get(0)?,
                        summary: row.get(1)?,
                        tags_json: row.get(2)?,
                        index_status: row.get(3)?,
                        indexed_hash: row.get(4)?,
                        embedding_model: row.get(5)?,
                        embedding_dimensions: row.get(6)?,
                        index_chunk_count: row.get::<_, i64>(7)?.max(0) as usize,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Knowledge item not found or SiYuan binding mismatch: {knowledge_id}/{siyuan_doc_id}"
                )
            })?
        };

        if can_skip(
            &metadata,
            &content_hash,
            embedding_enabled,
            embedding_model.as_deref(),
            configured_dimensions,
        ) {
            return Ok(KnowledgeIndexResult {
                content_hash,
                chunk_count: metadata.index_chunk_count,
                embedded_count: if embedding_enabled {
                    metadata.index_chunk_count
                } else {
                    0
                },
                skipped: true,
            });
        }

        self.begin_rebuild(
            &knowledge_id,
            &content_hash,
            &input.markdown,
            &metadata.title,
            &metadata.summary,
            &metadata.tags_json,
        )?;

        let chunks = split_into_chunks(
            &input.markdown,
            self.embeddings.chunk_target_tokens(),
            self.embeddings.chunk_overlap_tokens(),
        );

        let embedding_result = if embedding_enabled {
            self.embed_chunks(&chunks, configured_dimensions).await
        } else {
            Ok((Vec::new(), None))
        };

        let (vectors, actual_dimensions) = match embedding_result {
            Ok(value) => value,
            Err(error) => {
                let message = error.to_string();
                self.mark_failed_if_current(&knowledge_id, &content_hash, &message)?;
                return Err(error);
            }
        };

        let embedded_count = vectors.len();
        self.finish_rebuild(
            &knowledge_id,
            &content_hash,
            &chunks,
            if embedding_enabled {
                embedding_model.as_deref()
            } else {
                None
            },
            actual_dimensions,
            &vectors,
        )?;

        Ok(KnowledgeIndexResult {
            content_hash,
            chunk_count: chunks.len(),
            embedded_count,
            skipped: false,
        })
    }

    pub fn mark_deleted(&self, siyuan_doc_id: &str) -> anyhow::Result<Option<String>> {
        let siyuan_doc_id = required_text("siyuan_doc_id", siyuan_doc_id)?;
        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn();
        let knowledge_id: Option<String> = conn
            .query_row(
                "SELECT id FROM knowledge_item WHERE siyuan_doc_id = ?1",
                params![siyuan_doc_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(knowledge_id) = knowledge_id else {
            return Ok(None);
        };

        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result: anyhow::Result<()> = (|| {
            delete_vector_derivatives(&conn, &knowledge_id)?;
            conn.execute(
                "DELETE FROM knowledge_fts WHERE knowledge_id = ?1",
                params![knowledge_id],
            )?;
            conn.execute(
                "UPDATE knowledge_item
                 SET status = 'deleted', index_status = 'stale', indexed_hash = NULL,
                     indexed_at = NULL, embedding_model = NULL,
                     embedding_dimensions = NULL, index_chunk_count = 0,
                     last_index_error = NULL, current_remote_hash = NULL, updated_at = ?2
                 WHERE id = ?1",
                params![knowledge_id, now],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(error);
            }
        }

        Ok(Some(knowledge_id))
    }

    fn begin_rebuild(
        &self,
        knowledge_id: &str,
        content_hash: &str,
        markdown: &str,
        title: &str,
        summary: &str,
        tags_json: &str,
    ) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result: anyhow::Result<()> = (|| {
            delete_vector_derivatives(&conn, knowledge_id)?;
            upsert_fts(&conn, knowledge_id, title, summary, markdown, tags_json)?;
            let changed = conn.execute(
                "UPDATE knowledge_item
                 SET content = ?2, current_remote_hash = ?3, index_status = 'indexing',
                     indexed_hash = NULL, indexed_at = NULL, embedding_model = NULL,
                     embedding_dimensions = NULL, index_chunk_count = 0,
                     last_index_error = NULL, updated_at = ?4
                 WHERE id = ?1",
                params![knowledge_id, markdown, content_hash, now],
            )?;
            if changed == 0 {
                anyhow::bail!("Knowledge item not found: {knowledge_id}");
            }
            Ok(())
        })();
        match result {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(error);
            }
        }
        Ok(())
    }

    async fn embed_chunks(
        &self,
        chunks: &[String],
        configured_dimensions: Option<usize>,
    ) -> anyhow::Result<(Vec<Vec<f32>>, Option<usize>)> {
        if chunks.is_empty() {
            return Ok((Vec::new(), configured_dimensions));
        }

        let batch_size = self.embeddings.batch_size().max(1);
        let mut all_vectors = Vec::with_capacity(chunks.len());
        let mut actual_dimensions = configured_dimensions;

        for batch in chunks.chunks(batch_size) {
            let texts = batch.to_vec();
            let vectors = self.embeddings.embed(texts).await?;
            if vectors.len() != batch.len() {
                anyhow::bail!(
                    "Embedding response count mismatch: expected {}, got {}",
                    batch.len(),
                    vectors.len()
                );
            }

            for vector in &vectors {
                if vector.is_empty() {
                    anyhow::bail!("Embedding response contained an empty vector");
                }
                let dimensions = vector.len();
                match actual_dimensions {
                    Some(expected) if expected != dimensions => {
                        anyhow::bail!(
                            "Embedding dimension mismatch: expected {expected}, got {dimensions}"
                        );
                    }
                    None => actual_dimensions = Some(dimensions),
                    _ => {}
                }
            }
            all_vectors.extend(vectors);
        }

        Ok((all_vectors, actual_dimensions))
    }

    fn finish_rebuild(
        &self,
        knowledge_id: &str,
        content_hash: &str,
        chunks: &[String],
        embedding_model: Option<&str>,
        dimensions: Option<usize>,
        vectors: &[Vec<f32>],
    ) -> anyhow::Result<()> {
        if embedding_model.is_some() && vectors.len() != chunks.len() {
            anyhow::bail!(
                "Prepared embedding count mismatch: expected {}, got {}",
                chunks.len(),
                vectors.len()
            );
        }

        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn();
        let current: Option<(Option<String>, String)> = conn
            .query_row(
                "SELECT current_remote_hash, index_status FROM knowledge_item WHERE id = ?1",
                params![knowledge_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((current_hash, current_status)) = current else {
            anyhow::bail!("Knowledge item disappeared while indexing: {knowledge_id}");
        };
        if current_hash.as_deref() != Some(content_hash) || current_status != "indexing" {
            anyhow::bail!("Knowledge index rebuild was superseded: {knowledge_id}");
        }

        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result: anyhow::Result<()> = (|| {
            for (index, text) in chunks.iter().enumerate() {
                let chunk_id = Uuid::new_v4().to_string();
                let chunk_hash = sha256_hex(text.as_bytes());
                let token_count = estimate_token_count(text) as i64;
                conn.execute(
                    "INSERT INTO knowledge_chunk
                     (id, knowledge_id, heading, chunk_index, token_count, text, content_hash, created_at)
                     VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        chunk_id,
                        knowledge_id,
                        index as i64,
                        token_count,
                        text,
                        chunk_hash,
                        now
                    ],
                )?;

                if let Some(model) = embedding_model {
                    let vector = &vectors[index];
                    let actual_dimensions = dimensions.unwrap_or(vector.len());
                    conn.execute(
                        "INSERT INTO embedding_record
                         (id, chunk_id, model, dimensions, vector, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            Uuid::new_v4().to_string(),
                            chunk_id,
                            model,
                            actual_dimensions as i64,
                            encode_vector(vector),
                            now
                        ],
                    )?;
                }
            }

            let changed = conn.execute(
                "UPDATE knowledge_item
                 SET index_status = 'ready', indexed_hash = ?2, indexed_at = ?3,
                     embedding_model = ?4, embedding_dimensions = ?5,
                     index_chunk_count = ?6, last_index_error = NULL
                 WHERE id = ?1 AND current_remote_hash = ?2 AND index_status = 'indexing'",
                params![
                    knowledge_id,
                    content_hash,
                    now,
                    embedding_model,
                    dimensions.map(|value| value as i64),
                    chunks.len() as i64
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("Knowledge index rebuild was superseded: {knowledge_id}");
            }
            Ok(())
        })();
        match result {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(error);
            }
        }
        Ok(())
    }

    fn mark_failed_if_current(
        &self,
        knowledge_id: &str,
        content_hash: &str,
        error: &str,
    ) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn();
        conn.execute(
            "UPDATE knowledge_item
             SET index_status = 'failed', indexed_hash = NULL, indexed_at = NULL,
                 embedding_model = NULL, embedding_dimensions = NULL,
                 index_chunk_count = 0, last_index_error = ?3, updated_at = ?4
             WHERE id = ?1 AND current_remote_hash = ?2 AND index_status = 'indexing'",
            params![knowledge_id, content_hash, error, now],
        )?;
        Ok(())
    }
}

#[derive(Debug)]
struct IndexMetadata {
    title: String,
    summary: String,
    tags_json: String,
    index_status: String,
    indexed_hash: Option<String>,
    embedding_model: Option<String>,
    embedding_dimensions: Option<i64>,
    index_chunk_count: usize,
}

fn can_skip(
    metadata: &IndexMetadata,
    content_hash: &str,
    embedding_enabled: bool,
    embedding_model: Option<&str>,
    configured_dimensions: Option<usize>,
) -> bool {
    if metadata.index_status != "ready" || metadata.indexed_hash.as_deref() != Some(content_hash) {
        return false;
    }

    if !embedding_enabled {
        return metadata.embedding_model.is_none();
    }

    if metadata.embedding_model.as_deref() != embedding_model {
        return false;
    }

    match configured_dimensions {
        Some(expected) => metadata.embedding_dimensions == Some(expected as i64),
        None => true,
    }
}

fn delete_vector_derivatives(
    conn: &rusqlite::Connection,
    knowledge_id: &str,
) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM embedding_record
         WHERE chunk_id IN (SELECT id FROM knowledge_chunk WHERE knowledge_id = ?1)",
        params![knowledge_id],
    )?;
    conn.execute(
        "DELETE FROM knowledge_chunk WHERE knowledge_id = ?1",
        params![knowledge_id],
    )?;
    Ok(())
}

fn upsert_fts(
    conn: &rusqlite::Connection,
    knowledge_id: &str,
    title: &str,
    summary: &str,
    content: &str,
    tags_json: &str,
) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM knowledge_fts WHERE knowledge_id = ?1",
        params![knowledge_id],
    )?;
    conn.execute(
        "INSERT INTO knowledge_fts (knowledge_id, title, summary, content, tags)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![knowledge_id, title, summary, content, tags_json],
    )?;
    Ok(())
}

fn split_into_chunks(text: &str, target_tokens: usize, overlap_tokens: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    let chars: Vec<char> = text.chars().collect();
    let target_chars = ((target_tokens.max(1) as f64) * CHARS_PER_TOKEN)
        .round()
        .max(1.0) as usize;
    let overlap_chars = ((overlap_tokens as f64) * CHARS_PER_TOKEN).round() as usize;

    if chars.len() <= target_chars {
        return vec![text.to_string()];
    }

    let overlap_chars = overlap_chars.min(target_chars.saturating_sub(1));
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let end = (start + target_chars).min(chars.len());
        chunks.push(chars[start..end].iter().collect());
        if end == chars.len() {
            break;
        }
        let next = end.saturating_sub(overlap_chars);
        start = next.max(start + 1);
    }
    chunks
}

fn estimate_token_count(text: &str) -> usize {
    ((text.chars().count() as f64) / CHARS_PER_TOKEN)
        .ceil()
        .max(1.0) as usize
}

fn encode_vector(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * std::mem::size_of::<f32>());
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn required_text(field: &str, value: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("{field} is required");
    }
    Ok(trimmed.to_string())
}
