// CI lint baseline: pre-existing Clippy debt; remove allowances incrementally.
#![allow(clippy::chunks_exact_to_as_chunks)]

/// Knowledge Item Repository — CRUD for knowledge_item, knowledge_chunk, embedding_record
use chrono::Utc;
use rusqlite::{params, params_from_iter};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ai::schema_v3::{V3ExtractionResult, V3KnowledgeItem};
use crate::storage::StateDb;

pub struct KnowledgeRepo<'a> {
    db: &'a StateDb,
}

impl<'a> KnowledgeRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    /// Save all extracted knowledge items for a session.
    ///
    /// Stable identity policy:
    /// Reuse an existing ID only for a unique normalized (category, title) match.
    /// A changed title is treated as a new identity unless a future extraction
    /// schema provides an explicit stable key; never infer identity from category alone.
    ///
    /// Reused IDs preserve knowledge_sync_target mappings. Chunks/embeddings/FTS
    /// are rebuilt because the extracted content may have changed. Removed items
    /// retain an explicit REMOVED sink tombstone before the item row is deleted.
    pub fn save_items(
        &self,
        session_id: i64,
        project_name: Option<&str>,
        result: &V3ExtractionResult,
    ) -> anyhow::Result<Vec<String>> {
        #[derive(Debug)]
        struct ExistingIdentity {
            id: String,
            title: String,
            category: String,
        }

        fn normalize(value: &str) -> String {
            value
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        }

        fn key(category: &str, title: &str) -> String {
            format!("{}\u{1f}{}", normalize(category), normalize(title))
        }

        let conn = self.db.conn();
        let now = Utc::now().to_rfc3339();
        conn.execute_batch("BEGIN")?;

        let result_res: anyhow::Result<Vec<String>> = (|| {
            let existing: Vec<ExistingIdentity> = {
                let mut stmt = conn.prepare(
                    "SELECT id, title, category
                     FROM knowledge_item
                     WHERE source_session_id = ?1
                     ORDER BY rowid",
                )?;
                let rows = stmt
                    .query_map(params![session_id], |row| {
                        Ok(ExistingIdentity {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            category: row.get(2)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            };

            let mut old_by_key: std::collections::HashMap<String, Vec<usize>> =
                std::collections::HashMap::new();
            for (idx, old) in existing.iter().enumerate() {
                old_by_key
                    .entry(key(&old.category, &old.title))
                    .or_default()
                    .push(idx);
            }

            let mut new_key_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for item in &result.items {
                *new_key_counts
                    .entry(key(&item.category, &item.title))
                    .or_default() += 1;
            }

            let mut assignments: Vec<Option<usize>> = vec![None; result.items.len()];
            let mut used_old: std::collections::HashSet<usize> = std::collections::HashSet::new();

            // Pass 1: exact semantic identity, but only when unique on both sides.
            for (new_idx, item) in result.items.iter().enumerate() {
                let semantic_key = key(&item.category, &item.title);
                if new_key_counts.get(&semantic_key).copied() != Some(1) {
                    continue;
                }
                let Some(old_indexes) = old_by_key.get(&semantic_key) else {
                    continue;
                };
                if old_indexes.len() == 1 && used_old.insert(old_indexes[0]) {
                    assignments[new_idx] = Some(old_indexes[0]);
                }
            }

            // Existing derived data is always invalidated. For matched items the
            // canonical knowledge ID and sink mapping remain intact.
            for old in &existing {
                conn.execute(
                    "DELETE FROM embedding_record
                     WHERE chunk_id IN (
                         SELECT id FROM knowledge_chunk WHERE knowledge_id = ?1
                     )",
                    params![old.id],
                )?;
                conn.execute(
                    "DELETE FROM knowledge_chunk WHERE knowledge_id = ?1",
                    params![old.id],
                )?;
                conn.execute(
                    "DELETE FROM knowledge_fts WHERE knowledge_id = ?1",
                    params![old.id],
                )?;
            }

            // Explicitly tombstone removed items before deleting the item row.
            for (idx, old) in existing.iter().enumerate() {
                if used_old.contains(&idx) {
                    continue;
                }
                conn.execute(
                    "UPDATE knowledge_sync_target
                     SET status = 'REMOVED', error_message = NULL, updated_at = ?2
                     WHERE knowledge_id = ?1",
                    params![old.id, now],
                )?;
                conn.execute("DELETE FROM knowledge_item WHERE id = ?1", params![old.id])?;
            }

            let mut item_ids = Vec::with_capacity(result.items.len());
            for (new_idx, item) in result.items.iter().enumerate() {
                let tags_json =
                    serde_json::to_string(&item.tags).unwrap_or_else(|_| "[]".to_string());
                let content = build_content(item);

                let id = if let Some(old_idx) = assignments[new_idx] {
                    let id = existing[old_idx].id.clone();
                    conn.execute(
                        "UPDATE knowledge_item
                         SET project_name = ?2, title = ?3, category = ?4,
                             summary = ?5, content = ?6, tags = ?7,
                             confidence = ?8, worth_extracting = 1, updated_at = ?9
                         WHERE id = ?1 AND source_session_id = ?10",
                        params![
                            id,
                            project_name,
                            item.title,
                            item.category,
                            item.summary,
                            content,
                            tags_json,
                            item.confidence,
                            now,
                            session_id,
                        ],
                    )?;
                    id
                } else {
                    let id = Uuid::new_v4().to_string();
                    conn.execute(
                        "INSERT INTO knowledge_item
                         (id, source_session_id, project_name, title, category, summary, content,
                          tags, confidence, worth_extracting, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?10)",
                        params![
                            id,
                            session_id,
                            project_name,
                            item.title,
                            item.category,
                            item.summary,
                            content,
                            tags_json,
                            item.confidence,
                            now,
                        ],
                    )?;
                    id
                };

                conn.execute(
                    "INSERT INTO knowledge_fts (knowledge_id, title, summary, content, tags)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, item.title, item.summary, content, tags_json],
                )?;
                item_ids.push(id);
            }

            Ok(item_ids)
        })();

        match result_res {
            Ok(ids) => {
                conn.execute_batch("COMMIT")?;
                Ok(ids)
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Get all knowledge items for a session
    pub fn get_by_session(&self, session_id: i64) -> anyhow::Result<Vec<KnowledgeItemRow>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, title, category, summary, content, tags, confidence, updated_at
             FROM knowledge_item WHERE source_session_id = ?1 ORDER BY rowid",
        )?;
        let rows: Vec<KnowledgeItemRow> = stmt
            .query_map(params![session_id], |row| {
                Ok(KnowledgeItemRow {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    category: row.get(2)?,
                    summary: row.get(3)?,
                    content: row.get(4)?,
                    tags: row.get(5)?,
                    confidence: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Get a single knowledge item by ID
    pub fn get_by_id(&self, id: &str) -> anyhow::Result<Option<KnowledgeItemDetail>> {
        let conn = self.db.conn();
        let result = conn.query_row(
            "SELECT ki.id, ki.source_session_id, ki.project_name, ki.title, ki.category,
                    ki.summary, ki.content, ki.tags, ki.confidence, ki.created_at, ki.updated_at,
                    ss.source, ss.external_session_id, ss.title as session_title
             FROM knowledge_item ki
             JOIN source_session ss ON ss.id = ki.source_session_id
             WHERE ki.id = ?1",
            params![id],
            |row| {
                Ok(KnowledgeItemDetail {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    project_name: row.get(2)?,
                    title: row.get(3)?,
                    category: row.get(4)?,
                    summary: row.get(5)?,
                    content: row.get(6)?,
                    tags: row.get(7)?,
                    confidence: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                    source: row.get(11)?,
                    session_external_id: row.get(12)?,
                    session_title: row.get(13)?,
                    chunks: vec![],
                })
            },
        );

        match result {
            Ok(mut detail) => {
                // Load chunks
                let mut stmt = conn.prepare(
                    "SELECT id, heading, chunk_index, token_count, text
                     FROM knowledge_chunk WHERE knowledge_id = ?1 ORDER BY chunk_index",
                )?;
                detail.chunks = stmt
                    .query_map(params![detail.id], |row| {
                        Ok(KnowledgeChunkRow {
                            id: row.get(0)?,
                            heading: row.get(1)?,
                            chunk_index: row.get(2)?,
                            token_count: row.get(3)?,
                            text: row.get(4)?,
                            has_embedding: false,
                        })
                    })?
                    .filter_map(|r| r.ok())
                    .collect();

                // Check embeddings
                for chunk in &mut detail.chunks {
                    let has: bool = conn
                        .query_row(
                            "SELECT COUNT(*) FROM embedding_record WHERE chunk_id = ?1",
                            params![chunk.id],
                            |row| row.get::<_, i64>(0),
                        )
                        .unwrap_or(0)
                        > 0;
                    chunk.has_embedding = has;
                }

                Ok(Some(detail))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Save embedding chunks for a knowledge item
    pub fn save_embedding_chunks(
        &self,
        knowledge_id: &str,
        chunks: &[(Option<String>, String)], // (heading, text)
    ) -> anyhow::Result<Vec<String>> {
        let conn = self.db.conn();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "DELETE FROM knowledge_chunk WHERE knowledge_id = ?1",
            params![knowledge_id],
        )?;

        let mut ids = Vec::new();
        for (i, (heading, text)) in chunks.iter().enumerate() {
            let id = Uuid::new_v4().to_string();
            let hash = hex::encode(Sha256::digest(text.as_bytes()));
            let tokens = (text.len() as f64 / 3.5) as i32;

            conn.execute(
                "INSERT INTO knowledge_chunk (id, knowledge_id, heading, chunk_index, token_count, text, content_hash, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![id, knowledge_id, heading, i as i32, tokens, text, hash, now],
            )?;
            ids.push(id);
        }
        Ok(ids)
    }

    /// Save an embedding vector for a chunk
    pub fn save_embedding(
        &self,
        chunk_id: &str,
        model: &str,
        dimensions: i32,
        vector: &[f32],
    ) -> anyhow::Result<()> {
        let conn = self.db.conn();
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        // Serialize f32 vector as bytes (little-endian)
        let bytes: Vec<u8> = vector.iter().flat_map(|f| f.to_le_bytes()).collect();

        conn.execute(
            "INSERT OR REPLACE INTO embedding_record (id, chunk_id, model, dimensions, vector, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, chunk_id, model, dimensions, bytes, now],
        )?;
        Ok(())
    }

    /// Load a bounded embedding candidate set for vector reranking.
    ///
    /// When `knowledge_ids` is non-empty, candidates are restricted to those
    /// text/metadata matches. Otherwise the most recently updated knowledge is
    /// used as a deterministic bounded prefilter. The caller can combine a
    /// preferred pass with a recent fallback without ever loading the full
    /// embedding table.
    pub fn load_embedding_candidates(
        &self,
        model: &str,
        knowledge_ids: &[String],
        limit: usize,
    ) -> anyhow::Result<Vec<EmbeddingRow>> {
        if limit == 0 {
            return Ok(vec![]);
        }

        let conn = self.db.conn();
        let mut sql = String::from(
            "SELECT er.chunk_id, er.vector, kc.knowledge_id, kc.text \
             FROM embedding_record er \
             JOIN knowledge_chunk kc ON kc.id = er.chunk_id \
             JOIN knowledge_item ki ON ki.id = kc.knowledge_id \
             WHERE er.model = ?",
        );
        let mut values: Vec<rusqlite::types::Value> = vec![model.to_string().into()];

        if !knowledge_ids.is_empty() {
            sql.push_str(" AND kc.knowledge_id IN (");
            for (index, knowledge_id) in knowledge_ids.iter().enumerate() {
                if index > 0 {
                    sql.push(',');
                }
                sql.push('?');
                values.push(knowledge_id.clone().into());
            }
            sql.push(')');
        }

        sql.push_str(" ORDER BY ki.updated_at DESC, kc.chunk_index ASC LIMIT ?");
        values.push((limit as i64).into());

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(values.iter()), |row| {
                let bytes: Vec<u8> = row.get(1)?;
                let vector: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                Ok(EmbeddingRow {
                    chunk_id: row.get(0)?,
                    vector,
                    knowledge_id: row.get(2)?,
                    chunk_text: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Load all embeddings for a model (for in-memory search)
    pub fn load_all_embeddings(&self, model: &str) -> anyhow::Result<Vec<EmbeddingRow>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT er.chunk_id, er.vector, kc.knowledge_id, kc.text
             FROM embedding_record er
             JOIN knowledge_chunk kc ON kc.id = er.chunk_id
             WHERE er.model = ?1",
        )?;
        let rows: Vec<EmbeddingRow> = stmt
            .query_map(params![model], |row| {
                let bytes: Vec<u8> = row.get(1)?;
                let vector: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                Ok(EmbeddingRow {
                    chunk_id: row.get(0)?,
                    vector,
                    knowledge_id: row.get(2)?,
                    chunk_text: row.get(3)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }
}

fn build_content(item: &V3KnowledgeItem) -> String {
    let mut parts = vec![item.summary.clone()];

    if let Some(p) = &item.problem {
        parts.push(format!("**问题：** {}", p));
    }
    if let Some(rc) = &item.root_causes {
        if !rc.is_empty() {
            parts.push(format!(
                "**根因：**\n{}",
                rc.iter()
                    .map(|s| format!("- {}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
    }
    if let Some(sol) = &item.solutions {
        if !sol.is_empty() {
            parts.push(format!(
                "**解决方案：**\n{}",
                sol.iter()
                    .map(|s| format!("- {}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
    }
    if let Some(cmds) = &item.key_commands {
        if !cmds.is_empty() {
            parts.push(format!("**关键命令：**\n```\n{}\n```", cmds.join("\n")));
        }
    }
    if let Some(files) = &item.key_files {
        if !files.is_empty() {
            parts.push(format!(
                "**关键文件：**\n{}",
                files
                    .iter()
                    .map(|s| format!("- {}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
    }
    if !item.content.is_empty() {
        parts.push(item.content.clone());
    }

    parts.join("\n\n")
}

#[derive(Debug)]
pub struct KnowledgeItemRow {
    pub id: String,
    pub title: String,
    pub category: String,
    pub summary: String,
    pub content: String,
    pub tags: String,
    pub confidence: f64,
    pub updated_at: String,
}

#[derive(Debug)]
pub struct KnowledgeItemDetail {
    pub id: String,
    pub session_id: i64,
    pub project_name: Option<String>,
    pub title: String,
    pub category: String,
    pub summary: String,
    pub content: String,
    pub tags: String,
    pub confidence: f64,
    pub created_at: String,
    pub updated_at: String,
    pub source: String,
    pub session_external_id: String,
    pub session_title: Option<String>,
    pub chunks: Vec<KnowledgeChunkRow>,
}

#[derive(Debug)]
pub struct KnowledgeChunkRow {
    pub id: String,
    pub heading: Option<String>,
    pub chunk_index: i32,
    pub token_count: i32,
    pub text: String,
    pub has_embedding: bool,
}

#[derive(Debug)]
pub struct EmbeddingRow {
    pub chunk_id: String,
    pub vector: Vec<f32>,
    pub knowledge_id: String,
    pub chunk_text: String,
}
