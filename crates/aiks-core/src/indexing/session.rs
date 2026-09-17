use std::sync::Arc;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::service::EmbeddingProvider;
use crate::storage::StateDb;

const CHARS_PER_TOKEN: f64 = 3.5;

#[derive(Debug, Clone)]
pub struct SessionIndexInput {
    pub session_id: i64,
    pub external_id: String,
    pub source: String,
    pub title: Option<String>,
    pub normalized_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionIndexResult {
    pub content_hash: String,
    pub chunk_count: usize,
    pub embedded_count: usize,
    pub skipped: bool,
}

pub struct SessionIndexService {
    db: Arc<StateDb>,
    embeddings: Arc<dyn EmbeddingProvider>,
}

impl SessionIndexService {
    pub fn new(db: Arc<StateDb>, embeddings: Arc<dyn EmbeddingProvider>) -> Self {
        Self { db, embeddings }
    }

    pub async fn index_session(
        &self,
        input: SessionIndexInput,
    ) -> anyhow::Result<SessionIndexResult> {
        if input.session_id <= 0 {
            anyhow::bail!("session_id must be positive");
        }
        let external_id = required_text("external_id", &input.external_id)?;
        let source = required_text("source", &input.source)?;
        let content_hash = sha256_hex(input.normalized_text.as_bytes());
        let embedding_enabled = self.embeddings.enabled();
        let embedding_model = embedding_enabled.then(|| self.embeddings.model_name().to_string());
        let configured_dimensions = if embedding_enabled {
            self.embeddings.dimensions()
        } else {
            None
        };

        let (db_source, db_external_id, db_title, project_name) = {
            let conn = self.db.conn();
            conn.query_row(
                "SELECT source, external_session_id, title, project_name
                 FROM source_session WHERE id = ?1",
                params![input.session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("Source session not found: {}", input.session_id))?
        };

        if db_source != source || db_external_id != external_id {
            anyhow::bail!(
                "Session identity mismatch for {}: expected {}/{} but got {}/{}",
                input.session_id,
                db_source,
                db_external_id,
                source,
                external_id
            );
        }
        let title = input.title.or(db_title).unwrap_or_default();

        let state = {
            let conn = self.db.conn();
            conn.query_row(
                "SELECT status, indexed_hash, embedding_model, embedding_dimensions, chunk_count
                 FROM session_index_state WHERE session_id = ?1",
                params![input.session_id],
                |row| {
                    Ok(SessionIndexState {
                        status: row.get(0)?,
                        indexed_hash: row.get(1)?,
                        embedding_model: row.get(2)?,
                        embedding_dimensions: row.get(3)?,
                        chunk_count: row.get::<_, i64>(4)?.max(0) as usize,
                    })
                },
            )
            .optional()?
        };

        if state.as_ref().is_some_and(|state| {
            can_skip(
                state,
                &content_hash,
                embedding_enabled,
                embedding_model.as_deref(),
                configured_dimensions,
            )
        }) {
            self.upsert_fts(
                input.session_id,
                external_id,
                source,
                &title,
                project_name.as_deref(),
                &input.normalized_text,
            )?;
            let chunk_count = state.as_ref().map(|value| value.chunk_count).unwrap_or(0);
            return Ok(SessionIndexResult {
                content_hash,
                chunk_count,
                embedded_count: if embedding_enabled { chunk_count } else { 0 },
                skipped: true,
            });
        }

        let chunks = split_into_chunks(
            &input.normalized_text,
            self.embeddings.chunk_target_tokens(),
            self.embeddings.chunk_overlap_tokens(),
        );
        let prepared: Vec<PreparedSessionChunk> = chunks
            .into_iter()
            .enumerate()
            .map(|(index, text)| PreparedSessionChunk {
                id: Uuid::new_v4().to_string(),
                index,
                hash: sha256_hex(text.as_bytes()),
                text,
            })
            .collect();

        self.begin_rebuild(
            input.session_id,
            external_id,
            source,
            &title,
            project_name.as_deref(),
            &input.normalized_text,
            &content_hash,
            &prepared,
        )?;

        let embed_result = if embedding_enabled {
            self.embed_chunks(&prepared, configured_dimensions).await
        } else {
            Ok((Vec::new(), None))
        };

        let (vectors, actual_dimensions) = match embed_result {
            Ok(value) => value,
            Err(error) => {
                self.mark_failed(input.session_id, &content_hash, &error.to_string())?;
                return Err(error);
            }
        };

        self.finish_rebuild(
            input.session_id,
            &content_hash,
            &prepared,
            embedding_model.as_deref(),
            actual_dimensions,
            &vectors,
        )?;

        Ok(SessionIndexResult {
            content_hash,
            chunk_count: prepared.len(),
            embedded_count: vectors.len(),
            skipped: false,
        })
    }

    pub fn remove_session(&self, session_id: i64) -> anyhow::Result<()> {
        let conn = self.db.conn();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result: anyhow::Result<()> = (|| {
            conn.execute(
                "DELETE FROM session_embedding_record
                 WHERE chunk_id IN (SELECT id FROM session_search_chunk WHERE session_id = ?1)",
                params![session_id],
            )?;
            conn.execute(
                "DELETE FROM session_search_chunk WHERE session_id = ?1",
                params![session_id],
            )?;
            conn.execute(
                "DELETE FROM session_search_fts WHERE session_id = ?1",
                params![session_id],
            )?;
            conn.execute(
                "DELETE FROM session_index_state WHERE session_id = ?1",
                params![session_id],
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
        Ok(())
    }

    fn begin_rebuild(
        &self,
        session_id: i64,
        external_id: &str,
        source: &str,
        title: &str,
        project_name: Option<&str>,
        normalized_text: &str,
        content_hash: &str,
        chunks: &[PreparedSessionChunk],
    ) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result: anyhow::Result<()> = (|| {
            conn.execute(
                "DELETE FROM session_embedding_record
                 WHERE chunk_id IN (SELECT id FROM session_search_chunk WHERE session_id = ?1)",
                params![session_id],
            )?;
            conn.execute(
                "DELETE FROM session_search_chunk WHERE session_id = ?1",
                params![session_id],
            )?;
            conn.execute(
                "DELETE FROM session_search_fts WHERE session_id = ?1",
                params![session_id],
            )?;
            conn.execute(
                "INSERT INTO session_search_fts
                 (session_id, external_id, source, title, project_name, content)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    session_id,
                    external_id,
                    source,
                    title,
                    project_name,
                    normalized_text
                ],
            )?;

            for chunk in chunks {
                conn.execute(
                    "INSERT INTO session_search_chunk
                     (id, session_id, chunk_index, text, content_hash, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        chunk.id,
                        session_id,
                        chunk.index as i64,
                        chunk.text,
                        chunk.hash,
                        now
                    ],
                )?;
            }

            conn.execute(
                "INSERT INTO session_index_state
                 (session_id, status, indexed_hash, indexed_at, embedding_model,
                  embedding_dimensions, chunk_count, last_error)
                 VALUES (?1, 'indexing', ?2, NULL, NULL, NULL, ?3, NULL)
                 ON CONFLICT(session_id) DO UPDATE SET
                    status = 'indexing', indexed_hash = excluded.indexed_hash,
                    indexed_at = NULL, embedding_model = NULL,
                    embedding_dimensions = NULL, chunk_count = excluded.chunk_count,
                    last_error = NULL",
                params![session_id, content_hash, chunks.len() as i64],
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
        Ok(())
    }

    async fn embed_chunks(
        &self,
        chunks: &[PreparedSessionChunk],
        configured_dimensions: Option<usize>,
    ) -> anyhow::Result<(Vec<Vec<f32>>, Option<usize>)> {
        if chunks.is_empty() {
            return Ok((Vec::new(), configured_dimensions));
        }
        let batch_size = self.embeddings.batch_size().max(1);
        let mut vectors = Vec::with_capacity(chunks.len());
        let mut dimensions = configured_dimensions;

        for batch in chunks.chunks(batch_size) {
            let texts = batch.iter().map(|chunk| chunk.text.clone()).collect();
            let batch_vectors = self.embeddings.embed(texts).await?;
            if batch_vectors.len() != batch.len() {
                anyhow::bail!(
                    "Embedding response count mismatch: expected {}, got {}",
                    batch.len(),
                    batch_vectors.len()
                );
            }
            for vector in &batch_vectors {
                if vector.is_empty() {
                    anyhow::bail!("Embedding response contained an empty vector");
                }
                match dimensions {
                    Some(expected) if expected != vector.len() => {
                        anyhow::bail!(
                            "Embedding dimension mismatch: expected {expected}, got {}",
                            vector.len()
                        );
                    }
                    None => dimensions = Some(vector.len()),
                    _ => {}
                }
            }
            vectors.extend(batch_vectors);
        }
        Ok((vectors, dimensions))
    }

    fn finish_rebuild(
        &self,
        session_id: i64,
        content_hash: &str,
        chunks: &[PreparedSessionChunk],
        embedding_model: Option<&str>,
        dimensions: Option<usize>,
        vectors: &[Vec<f32>],
    ) -> anyhow::Result<()> {
        if embedding_model.is_some() && chunks.len() != vectors.len() {
            anyhow::bail!(
                "Prepared embedding count mismatch: expected {}, got {}",
                chunks.len(),
                vectors.len()
            );
        }

        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn();
        let current: Option<(String, String)> = conn
            .query_row(
                "SELECT status, COALESCE(indexed_hash, '') FROM session_index_state WHERE session_id = ?1",
                params![session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if current.as_ref().map(|value| value.0.as_str()) != Some("indexing")
            || current.as_ref().map(|value| value.1.as_str()) != Some(content_hash)
        {
            anyhow::bail!("Session index rebuild was superseded: {session_id}");
        }

        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result: anyhow::Result<()> = (|| {
            if let Some(model) = embedding_model {
                for (chunk, vector) in chunks.iter().zip(vectors.iter()) {
                    conn.execute(
                        "INSERT INTO session_embedding_record
                         (id, chunk_id, model, dimensions, vector, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            Uuid::new_v4().to_string(),
                            chunk.id,
                            model,
                            dimensions.unwrap_or(vector.len()) as i64,
                            encode_vector(vector),
                            now
                        ],
                    )?;
                }
            }

            let changed = conn.execute(
                "UPDATE session_index_state
                 SET status = 'ready', indexed_at = ?3, embedding_model = ?4,
                     embedding_dimensions = ?5, chunk_count = ?6, last_error = NULL
                 WHERE session_id = ?1 AND indexed_hash = ?2 AND status = 'indexing'",
                params![
                    session_id,
                    content_hash,
                    now,
                    embedding_model,
                    dimensions.map(|value| value as i64),
                    chunks.len() as i64
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("Session index rebuild was superseded: {session_id}");
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

    fn mark_failed(&self, session_id: i64, content_hash: &str, error: &str) -> anyhow::Result<()> {
        let conn = self.db.conn();
        conn.execute(
            "UPDATE session_index_state
             SET status = 'failed', indexed_at = NULL, embedding_model = NULL,
                 embedding_dimensions = NULL, last_error = ?3
             WHERE session_id = ?1 AND indexed_hash = ?2",
            params![session_id, content_hash, error],
        )?;
        Ok(())
    }

    fn upsert_fts(
        &self,
        session_id: i64,
        external_id: &str,
        source: &str,
        title: &str,
        project_name: Option<&str>,
        normalized_text: &str,
    ) -> anyhow::Result<()> {
        let conn = self.db.conn();
        conn.execute(
            "DELETE FROM session_search_fts WHERE session_id = ?1",
            params![session_id],
        )?;
        conn.execute(
            "INSERT INTO session_search_fts
             (session_id, external_id, source, title, project_name, content)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session_id,
                external_id,
                source,
                title,
                project_name,
                normalized_text
            ],
        )?;
        Ok(())
    }
}

#[derive(Debug)]
struct SessionIndexState {
    status: String,
    indexed_hash: Option<String>,
    embedding_model: Option<String>,
    embedding_dimensions: Option<i64>,
    chunk_count: usize,
}

#[derive(Debug)]
struct PreparedSessionChunk {
    id: String,
    index: usize,
    text: String,
    hash: String,
}

fn can_skip(
    state: &SessionIndexState,
    content_hash: &str,
    embedding_enabled: bool,
    embedding_model: Option<&str>,
    configured_dimensions: Option<usize>,
) -> bool {
    if state.status != "ready" || state.indexed_hash.as_deref() != Some(content_hash) {
        return false;
    }
    if !embedding_enabled {
        return state.embedding_model.is_none();
    }
    if state.embedding_model.as_deref() != embedding_model {
        return false;
    }
    match configured_dimensions {
        Some(expected) => state.embedding_dimensions == Some(expected as i64),
        None => true,
    }
}

fn split_into_chunks(text: &str, target_tokens: usize, overlap_tokens: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = text.chars().collect();
    let target_chars = ((target_tokens.max(1) as f64) * CHARS_PER_TOKEN).round() as usize;
    let target_chars = target_chars.max(1);
    let overlap_chars = (((overlap_tokens as f64) * CHARS_PER_TOKEN).round() as usize)
        .min(target_chars.saturating_sub(1));

    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let end = (start + target_chars).min(chars.len());
        let chunk: String = chars[start..end].iter().collect();
        if !chunk.trim().is_empty() {
            chunks.push(chunk);
        }
        if end >= chars.len() {
            break;
        }
        start = end.saturating_sub(overlap_chars);
    }
    chunks
}

fn encode_vector(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|value| value.to_le_bytes()).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn required_text<'a>(field: &str, value: &'a str) -> anyhow::Result<&'a str> {
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("{field} must not be empty");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use tempfile::tempdir;

    use super::*;
    use crate::storage::SourceSessionRepo;

    struct FakeEmbedding {
        calls: Mutex<usize>,
    }

    #[async_trait]
    impl EmbeddingProvider for FakeEmbedding {
        fn enabled(&self) -> bool {
            true
        }

        fn model_name(&self) -> &str {
            "session-test-embed"
        }

        fn dimensions(&self) -> Option<usize> {
            Some(2)
        }

        async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
            *self.calls.lock().unwrap() += 1;
            Ok(texts.into_iter().map(|_| vec![1.0, 0.0]).collect())
        }
    }

    fn seed_session(db: &StateDb) -> i64 {
        SourceSessionRepo::new(db)
            .upsert(
                "claude_code",
                "session-v42",
                None,
                None,
                Some("AIKS"),
                Some("Session Search"),
                None,
                Some("hash"),
                Some("test"),
            )
            .unwrap()
    }

    #[tokio::test]
    async fn session_index_is_idempotent_and_searchable() {
        let dir = tempdir().unwrap();
        let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
        let session_id = seed_session(&db);
        let embeddings = Arc::new(FakeEmbedding {
            calls: Mutex::new(0),
        });
        let service = SessionIndexService::new(db.clone(), embeddings.clone());
        let input = SessionIndexInput {
            session_id,
            external_id: "session-v42".into(),
            source: "claude_code".into(),
            title: Some("Session Search".into()),
            normalized_text: "kubernetes 节点磁盘空间不足 /var/lib/kubelet/pods".into(),
        };

        let first = service.index_session(input.clone()).await.unwrap();
        let second = service.index_session(input).await.unwrap();
        assert!(!first.skipped);
        assert!(second.skipped);
        assert_eq!(*embeddings.calls.lock().unwrap(), 1);

        let conn = db.conn();
        let fts_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_search_fts WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap();
        let vector_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_embedding_record er
                 JOIN session_search_chunk sc ON sc.id = er.chunk_id
                 WHERE sc.session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 1);
        assert_eq!(vector_count, first.chunk_count as i64);
    }
}
