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
