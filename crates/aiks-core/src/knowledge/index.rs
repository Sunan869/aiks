use chrono::Utc;
use rusqlite::params;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::storage::StateDb;

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedChunk {
    pub heading: Option<String>,
    pub text: String,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnowledgeIndexStats {
    pub chunk_count: usize,
    pub embedding_count: usize,
}

/// Persists a fully prepared knowledge vector index.
///
/// Embedding requests must happen before calling this service. The service only
/// performs local SQLite work so the old index remains intact until every new
/// vector is available and the replacement can be committed atomically.
pub struct KnowledgeIndexService<'a> {
    db: &'a StateDb,
}

impl<'a> KnowledgeIndexService<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    pub fn replace_knowledge_index(
        &self,
        knowledge_id: &str,
        model: &str,
        dimensions: usize,
        chunks: &[IndexedChunk],
    ) -> anyhow::Result<KnowledgeIndexStats> {
        let knowledge_id = knowledge_id.trim();
        if knowledge_id.is_empty() {
            anyhow::bail!("knowledge_id is required");
        }

        let model = model.trim();
        if model.is_empty() {
            anyhow::bail!("embedding model is required");
        }
        if dimensions == 0 {
            anyhow::bail!("embedding dimensions must be greater than zero");
        }

        for (index, chunk) in chunks.iter().enumerate() {
            if chunk.text.trim().is_empty() {
                anyhow::bail!("chunk {index} text is required");
            }
            if chunk.embedding.len() != dimensions {
                anyhow::bail!(
                    "chunk {index} embedding dimension mismatch: expected {dimensions}, got {}",
                    chunk.embedding.len()
                );
            }
        }

        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn();
        let exists: i64 = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_item WHERE id = ?1)",
            params![knowledge_id],
            |row| row.get(0),
        )?;
        if exists == 0 {
            anyhow::bail!("Knowledge item not found: {knowledge_id}");
        }

        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result: anyhow::Result<()> = (|| {
            conn.execute(
                "DELETE FROM embedding_record
                 WHERE chunk_id IN (SELECT id FROM knowledge_chunk WHERE knowledge_id = ?1)",
                params![knowledge_id],
            )?;
            conn.execute(
                "DELETE FROM knowledge_chunk WHERE knowledge_id = ?1",
                params![knowledge_id],
            )?;

            for (chunk_index, chunk) in chunks.iter().enumerate() {
                let chunk_id = Uuid::new_v4().to_string();
                let embedding_id = Uuid::new_v4().to_string();
                let text = chunk.text.trim();
                let heading = chunk
                    .heading
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let token_count = text.split_whitespace().count().max(1) as i64;
                let content_hash = format!("{:x}", Sha256::digest(text.as_bytes()));
                let vector = encode_vector(&chunk.embedding);

                conn.execute(
                    "INSERT INTO knowledge_chunk
                     (id, knowledge_id, heading, chunk_index, token_count, text, content_hash, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        chunk_id,
                        knowledge_id,
                        heading,
                        chunk_index as i64,
                        token_count,
                        text,
                        content_hash,
                        now
                    ],
                )?;
                conn.execute(
                    "INSERT INTO embedding_record
                     (id, chunk_id, model, dimensions, vector, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        embedding_id,
                        chunk_id,
                        model,
                        dimensions as i64,
                        vector,
                        now
                    ],
                )?;
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

        Ok(KnowledgeIndexStats {
            chunk_count: chunks.len(),
            embedding_count: chunks.len(),
        })
    }
}

fn encode_vector(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * std::mem::size_of::<f32>());
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{CreateKnowledgeInput, KnowledgeService};
    use crate::storage::StateDb;
    use rusqlite::params;
    use tempfile::tempdir;

    #[test]
    fn replace_knowledge_index_atomically_replaces_old_chunks_and_embeddings() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let knowledge = KnowledgeService::new(&db)
            .create_manual(CreateKnowledgeInput {
                title: "Index lifecycle".into(),
                category: Some("engineering".into()),
                project_name: Some("AIKS".into()),
                summary: Some("old summary".into()),
                content: "canonical content".into(),
                tags: vec!["search".into()],
            })
            .unwrap();

        {
            let conn = db.conn();
            conn.execute(
                "INSERT INTO knowledge_chunk
                 (id, knowledge_id, heading, chunk_index, token_count, text, content_hash, created_at)
                 VALUES ('old-chunk', ?1, NULL, 0, 1, 'old text', 'old-hash', '2026-09-17T00:00:00Z')",
                params![knowledge.id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO embedding_record
                 (id, chunk_id, model, dimensions, vector, created_at)
                 VALUES ('old-embedding', 'old-chunk', 'old-model', 2, ?1, '2026-09-17T00:00:00Z')",
                params![vec![0_u8; 8]],
            )
            .unwrap();
        }

        let service = KnowledgeIndexService::new(&db);
        let stats = service
            .replace_knowledge_index(
                &knowledge.id,
                "embed-v4",
                3,
                &[
                    IndexedChunk {
                        heading: Some("A".into()),
                        text: "first chunk".into(),
                        embedding: vec![0.1, 0.2, 0.3],
                    },
                    IndexedChunk {
                        heading: None,
                        text: "second chunk".into(),
                        embedding: vec![0.4, 0.5, 0.6],
                    },
                ],
            )
            .unwrap();

        assert_eq!(stats.chunk_count, 2);
        assert_eq!(stats.embedding_count, 2);

        let conn = db.conn();
        let old_chunk_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM knowledge_chunk WHERE id = 'old-chunk'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let old_embedding_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM embedding_record WHERE id = 'old-embedding'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_chunk_count, 0);
        assert_eq!(old_embedding_count, 0);

        let rows: Vec<(i64, String, i64, Vec<u8>)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT kc.chunk_index, er.model, er.dimensions, er.vector
                     FROM knowledge_chunk kc
                     JOIN embedding_record er ON er.chunk_id = kc.id
                     WHERE kc.knowledge_id = ?1
                     ORDER BY kc.chunk_index",
                )
                .unwrap();
            stmt.query_map(params![knowledge.id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
        };

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, 0);
        assert_eq!(rows[1].0, 1);
        assert!(rows.iter().all(|row| row.1 == "embed-v4"));
        assert!(rows.iter().all(|row| row.2 == 3));
        assert_eq!(rows[0].3.len(), 12);
        assert_eq!(rows[1].3.len(), 12);

        let orphan_embeddings: i64 = conn
            .query_row(
                "SELECT COUNT(*)
                 FROM embedding_record er
                 LEFT JOIN knowledge_chunk kc ON kc.id = er.chunk_id
                 WHERE kc.id IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphan_embeddings, 0);
    }
}
