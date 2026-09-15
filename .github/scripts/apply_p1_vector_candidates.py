from pathlib import Path

p = Path("crates/aiks-core/src/pipeline/knowledge_repo.rs")
s = p.read_text()

old = "use rusqlite::params;"
new = "use rusqlite::{params, params_from_iter};"
if old not in s:
    raise SystemExit("rusqlite import marker not found")
s = s.replace(old, new, 1)

marker = '''    /// Load all embeddings for a model (for in-memory search)\n    pub fn load_all_embeddings(&self, model: &str) -> anyhow::Result<Vec<EmbeddingRow>> {\n'''
method = r'''    /// Load a bounded embedding candidate set for vector reranking.
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

'''
if marker not in s:
    raise SystemExit("load_all_embeddings marker not found")
s = s.replace(marker, method + marker, 1)

p.write_text(s)
