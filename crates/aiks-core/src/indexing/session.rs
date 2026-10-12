use std::sync::Arc;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::service::{validate_embedding_vector, EmbeddingProvider};
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

        let skip_candidate = state.as_ref().is_some_and(|state| {
            can_skip(
                state,
                &content_hash,
                embedding_enabled,
                embedding_model.as_deref(),
                configured_dimensions,
            )
        });
        if skip_candidate
            && self.persisted_index_is_complete(
                input.session_id,
                state.as_ref().expect("skip_candidate requires index state"),
                embedding_enabled,
            )?
        {
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

        let rebuild = SessionRebuildContext {
            session_id: input.session_id,
            external_id,
            source,
            title: &title,
            project_name: project_name.as_deref(),
            normalized_text: &input.normalized_text,
            content_hash: &content_hash,
            chunks: &prepared,
        };
        self.begin_rebuild(&rebuild)?;

        let embed_result = if embedding_enabled {
            self.embed_chunks(&prepared, configured_dimensions).await
        } else {
            Ok((Vec::new(), None))
        };

        let (vectors, actual_dimensions, final_embedding_model, embedding_error) =
            match embed_result {
                Ok((vectors, dimensions)) => {
                    (vectors, dimensions, embedding_model.as_deref(), None)
                }
                Err(error) => {
                    let message = error.to_string();
                    (Vec::new(), None, None, Some(message))
                }
            };

        let finish = SessionFinishContext {
            session_id: input.session_id,
            content_hash: &content_hash,
            chunks: &prepared,
            embedding_model: final_embedding_model,
            dimensions: actual_dimensions,
            vectors: &vectors,
            last_error: embedding_error.as_deref(),
        };
        self.finish_rebuild(&finish)?;

        Ok(SessionIndexResult {
            content_hash,
            chunk_count: prepared.len(),
            embedded_count: vectors.len(),
            skipped: false,
        })
    }

    fn persisted_index_is_complete(
        &self,
        session_id: i64,
        state: &SessionIndexState,
        embedding_enabled: bool,
    ) -> anyhow::Result<bool> {
        let conn = self.db.conn();
        let (chunk_count, embedding_count, invalid_dimensions): (i64, i64, i64) = conn.query_row(
            "SELECT COUNT(c.id), COUNT(e.id),
                        COALESCE(SUM(CASE WHEN e.id IS NOT NULL AND
                            (e.dimensions <= 0 OR e.dimensions != ?2 OR
                             LENGTH(e.vector) != e.dimensions * 4)
                            THEN 1 ELSE 0 END), 0)
                 FROM session_search_chunk c
                 LEFT JOIN session_embedding_record e ON e.chunk_id = c.id
                 WHERE c.session_id = ?1",
            params![session_id, state.embedding_dimensions],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if chunk_count != state.chunk_count as i64 {
            return Ok(false);
        }
        if embedding_enabled {
            return Ok(state.embedding_dimensions.is_some_and(|dim| dim > 0)
                && embedding_count == chunk_count
                && invalid_dimensions == 0);
        }
        Ok(embedding_count == 0)
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

    fn begin_rebuild(&self, rebuild: &SessionRebuildContext<'_>) -> anyhow::Result<()> {
        let session_id = rebuild.session_id;
        let external_id = rebuild.external_id;
        let source = rebuild.source;
        let title = rebuild.title;
        let project_name = rebuild.project_name;
        let normalized_text = rebuild.normalized_text;
        let content_hash = rebuild.content_hash;
        let chunks = rebuild.chunks;
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
                validate_embedding_vector(vector, &mut dimensions)?;
            }
            vectors.extend(batch_vectors);
        }
        Ok((vectors, dimensions))
    }

    fn finish_rebuild(&self, finish: &SessionFinishContext<'_>) -> anyhow::Result<()> {
        let session_id = finish.session_id;
        let content_hash = finish.content_hash;
        let chunks = finish.chunks;
        let embedding_model = finish.embedding_model;
        let dimensions = finish.dimensions;
        let vectors = finish.vectors;
        let last_error = finish.last_error;

        if embedding_model.is_some() && chunks.len() != vectors.len() {
            anyhow::bail!(
                "Prepared embedding count mismatch: expected {}, got {}",
                chunks.len(),
                vectors.len()
            );
        }

        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result: anyhow::Result<()> = (|| {
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
            let stored_chunks: Vec<String> = {
                let mut stmt = conn.prepare(
                    "SELECT id FROM session_search_chunk WHERE session_id = ?1 ORDER BY chunk_index",
                )?;
                let rows = stmt.query_map(params![session_id], |row| row.get(0))?;
                rows.collect::<rusqlite::Result<Vec<String>>>()?
            };
            if stored_chunks.len() != chunks.len()
                || stored_chunks
                    .iter()
                    .zip(chunks.iter())
                    .any(|(stored, prepared)| stored != &prepared.id)
            {
                anyhow::bail!("Session index rebuild generation was superseded: {session_id}");
            }
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
                     embedding_dimensions = ?5, chunk_count = ?6, last_error = ?7
                 WHERE session_id = ?1 AND indexed_hash = ?2 AND status = 'indexing'",
                params![
                    session_id,
                    content_hash,
                    now,
                    embedding_model,
                    dimensions.map(|value| value as i64),
                    chunks.len() as i64,
                    last_error
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

#[derive(Debug)]
struct SessionRebuildContext<'a> {
    session_id: i64,
    external_id: &'a str,
    source: &'a str,
    title: &'a str,
    project_name: Option<&'a str>,
    normalized_text: &'a str,
    content_hash: &'a str,
    chunks: &'a [PreparedSessionChunk],
}

#[derive(Debug)]
struct SessionFinishContext<'a> {
    session_id: i64,
    content_hash: &'a str,
    chunks: &'a [PreparedSessionChunk],
    embedding_model: Option<&'a str>,
    dimensions: Option<usize>,
    vectors: &'a [Vec<f32>],
    last_error: Option<&'a str>,
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
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
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

    struct MalformedEmbedding(Vec<f32>);

    #[async_trait]
    impl EmbeddingProvider for MalformedEmbedding {
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
            Ok(vec![self.0.clone(); texts.len()])
        }
    }

    #[tokio::test]
    async fn invalid_embedding_vectors_never_enter_session_index() {
        for invalid in [
            vec![f32::INFINITY, 1.0],
            vec![0.0, 0.0],
            vec![1.0],
            vec![1.0; 16_385],
        ] {
            let dir = tempdir().unwrap();
            let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
            let session_id = seed_session(&db);
            let indexer =
                SessionIndexService::new(db.clone(), Arc::new(MalformedEmbedding(invalid)));
            let result = indexer
                .index_session(SessionIndexInput {
                    session_id,
                    external_id: "session-v42".into(),
                    source: "claude_code".into(),
                    title: None,
                    normalized_text: "important searchable content".into(),
                })
                .await
                .unwrap();
            assert_eq!(result.embedded_count, 0);
            let conn = db.conn();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM session_embedding_record", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0);
            let lexical_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM session_search_fts WHERE session_id = ?1",
                    params![session_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(lexical_count, 1);
        }
    }

    struct LateBadEmbedding {
        calls: Mutex<usize>,
    }

    #[async_trait]
    impl EmbeddingProvider for LateBadEmbedding {
        fn enabled(&self) -> bool {
            true
        }

        fn model_name(&self) -> &str {
            "session-test-embed"
        }

        fn dimensions(&self) -> Option<usize> {
            Some(2)
        }

        fn batch_size(&self) -> usize {
            1
        }

        fn chunk_target_tokens(&self) -> usize {
            2
        }

        fn chunk_overlap_tokens(&self) -> usize {
            0
        }

        async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
            let mut count = self.calls.lock().unwrap();
            *count += 1;
            let vector = if *count == 1 {
                vec![1.0, 0.0]
            } else {
                vec![f32::NAN, 0.0]
            };
            Ok(vec![vector; texts.len()])
        }
    }

    #[tokio::test]
    async fn later_invalid_embedding_batch_is_atomic_and_recoverable() {
        let dir = tempdir().unwrap();
        let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
        let session_id = seed_session(&db);
        let input = SessionIndexInput {
            session_id,
            external_id: "session-v42".into(),
            source: "claude_code".into(),
            title: None,
            normalized_text: "first chunk and then a later chunk".into(),
        };
        let bad = SessionIndexService::new(
            db.clone(),
            Arc::new(LateBadEmbedding {
                calls: Mutex::new(0),
            }),
        );
        let result = bad.index_session(input.clone()).await.unwrap();
        assert!(result.chunk_count > 1);
        assert_eq!(result.embedded_count, 0);
        {
            let conn = db.conn();
            let vectors: i64 = conn
                .query_row("SELECT COUNT(*) FROM session_embedding_record", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(vectors, 0);
            let (status, last_error): (String, Option<String>) = conn
                .query_row(
                    "SELECT status, last_error FROM session_index_state WHERE session_id = ?1",
                    params![session_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(status, "ready");
            assert!(last_error.unwrap().contains("non-finite"));
            let lexical_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM session_search_fts WHERE session_id = ?1",
                    params![session_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(lexical_count, 1);
        }

        let repaired = SessionIndexService::new(
            db.clone(),
            Arc::new(FakeEmbedding {
                calls: Mutex::new(0),
            }),
        );
        let retry = repaired.index_session(input).await.unwrap();
        assert!(!retry.skipped);
        assert_eq!(retry.embedded_count, retry.chunk_count);
        assert_eq!(retry.chunk_count, 1);
        let (recovered_status, recovered_error): (String, Option<String>) = db
            .conn()
            .query_row(
                "SELECT status, last_error FROM session_index_state WHERE session_id = ?1",
                params![session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(recovered_status, "ready");
        assert!(recovered_error.is_none());
        let recovered_vectors: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM session_embedding_record", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(recovered_vectors, retry.chunk_count as i64);
    }

    #[test]
    fn concurrent_same_content_rebuild_rejects_stale_embedding_generation() {
        let dir = tempdir().unwrap();
        let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
        let session_id = seed_session(&db);
        let service = SessionIndexService::new(
            db.clone(),
            Arc::new(FakeEmbedding {
                calls: Mutex::new(0),
            }),
        );
        let old_chunks = vec![PreparedSessionChunk {
            id: "old-generation".into(),
            index: 0,
            hash: "same".into(),
            text: "searchable".into(),
        }];
        let new_chunks = vec![PreparedSessionChunk {
            id: "new-generation".into(),
            index: 0,
            hash: "same".into(),
            text: "searchable".into(),
        }];
        for chunks in [&old_chunks, &new_chunks] {
            service
                .begin_rebuild(&SessionRebuildContext {
                    session_id,
                    external_id: "session-v42",
                    source: "claude_code",
                    title: "Session Search",
                    project_name: Some("AIKS"),
                    normalized_text: "searchable",
                    content_hash: "same",
                    chunks,
                })
                .unwrap();
        }
        let stale_vectors = vec![vec![1.0, 0.0]];
        let stale = service.finish_rebuild(&SessionFinishContext {
            session_id,
            content_hash: "same",
            chunks: &old_chunks,
            embedding_model: Some("session-test-embed"),
            dimensions: Some(2),
            vectors: &stale_vectors,
            last_error: None,
        });
        assert!(stale.unwrap_err().to_string().contains("superseded"));
        let pending: String = db
            .conn()
            .query_row(
                "SELECT status FROM session_index_state WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending, "indexing");
        let persisted_generation: String = db
            .conn()
            .query_row(
                "SELECT id FROM session_search_chunk WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted_generation, "new-generation");
        let premature_vectors: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM session_embedding_record", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(premature_vectors, 0);
        service
            .finish_rebuild(&SessionFinishContext {
                session_id,
                content_hash: "same",
                chunks: &new_chunks,
                embedding_model: Some("session-test-embed"),
                dimensions: Some(2),
                vectors: &stale_vectors,
                last_error: None,
            })
            .unwrap();
        let conn = db.conn();
        let stored: String = conn
            .query_row("SELECT chunk_id FROM session_embedding_record", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(stored, "new-generation");
    }

    #[tokio::test]
    async fn missing_persisted_embedding_rebuilds_instead_of_falsely_skipping() {
        let dir = tempdir().unwrap();
        let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
        let session_id = seed_session(&db);
        let provider = Arc::new(FakeEmbedding {
            calls: Mutex::new(0),
        });
        let service = SessionIndexService::new(db.clone(), provider.clone());
        let input = SessionIndexInput {
            session_id,
            external_id: "session-v42".into(),
            source: "claude_code".into(),
            title: None,
            normalized_text: "searchable current content".into(),
        };
        let initial = service.index_session(input.clone()).await.unwrap();
        assert_eq!(initial.embedded_count, 1);
        assert!(service.index_session(input.clone()).await.unwrap().skipped);
        {
            let conn = db.conn();
            conn.execute(
                "DELETE FROM session_embedding_record WHERE chunk_id IN
                 (SELECT id FROM session_search_chunk WHERE session_id = ?1)",
                params![session_id],
            )
            .unwrap();
        }
        let recovered = service.index_session(input.clone()).await.unwrap();
        assert!(!recovered.skipped);
        assert_eq!(recovered.embedded_count, 1);
        assert_eq!(*provider.calls.lock().unwrap(), 2);
        assert!(service.index_session(input).await.unwrap().skipped);
    }

    #[tokio::test]
    async fn malformed_persisted_vector_rebuilds_before_idempotent_skip() {
        for corrupt_vector in [vec![0_u8; 3], vec![0_u8; 4]] {
            let dir = tempdir().unwrap();
            let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
            let session_id = seed_session(&db);
            let provider = Arc::new(FakeEmbedding {
                calls: Mutex::new(0),
            });
            let service = SessionIndexService::new(db.clone(), provider.clone());
            let input = SessionIndexInput {
                session_id,
                external_id: "session-v42".into(),
                source: "claude_code".into(),
                title: None,
                normalized_text: "searchable current content".into(),
            };
            assert_eq!(
                service
                    .index_session(input.clone())
                    .await
                    .unwrap()
                    .embedded_count,
                1
            );
            {
                let conn = db.conn();
                conn.execute(
                    "UPDATE session_embedding_record SET vector = ?1 WHERE chunk_id IN
                     (SELECT id FROM session_search_chunk WHERE session_id = ?2)",
                    params![corrupt_vector, session_id],
                )
                .unwrap();
            }
            let repaired = service.index_session(input.clone()).await.unwrap();
            assert!(!repaired.skipped);
            assert_eq!(repaired.embedded_count, 1);
            assert_eq!(*provider.calls.lock().unwrap(), 2);
            let (stored_dimensions, stored_bytes): (i64, i64) = db
                .conn()
                .query_row(
                    "SELECT dimensions, LENGTH(vector) FROM session_embedding_record",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(stored_dimensions, 2);
            assert_eq!(stored_bytes, 8);
            assert!(service.index_session(input).await.unwrap().skipped);
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
    async fn malformed_embedding_preserves_lexical_session_and_allows_retry() {
        let dir = tempdir().unwrap();
        let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
        let session_id = seed_session(&db);
        let input = SessionIndexInput {
            session_id,
            external_id: "session-v42".into(),
            source: "claude_code".into(),
            title: Some("Session Search".into()),
            normalized_text: "still searchable".into(),
        };
        let bad = SessionIndexService::new(
            db.clone(),
            Arc::new(MalformedEmbedding(vec![f32::NAN, 0.0])),
        );
        let result = bad.index_session(input.clone()).await.unwrap();
        assert_eq!(result.embedded_count, 0);
        let (status, error): (String, Option<String>) = db
            .conn()
            .query_row(
                "SELECT status, last_error FROM session_index_state WHERE session_id = ?1",
                params![session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "ready");
        assert!(error.unwrap().contains("Embedding response"));
        let fts_count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM session_search_fts WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 1);
        let good = Arc::new(FakeEmbedding {
            calls: Mutex::new(0),
        });
        let repaired = SessionIndexService::new(db.clone(), good);
        let retry = repaired.index_session(input).await.unwrap();
        assert!(!retry.skipped);
        assert_eq!(retry.embedded_count, 1);
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
