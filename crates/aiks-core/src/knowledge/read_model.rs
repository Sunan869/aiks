use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::storage::StateDb;

/// Rebuild AIKS' local searchable projection from canonical SiYuan content.
///
/// This function never writes content back to SiYuan. It deliberately leaves
/// `generated_hash` untouched so a later extraction can still detect that the
/// user changed the canonical document after the last AI-generated version.
pub fn refresh_siyuan_document_read_model(
    db: &StateDb,
    siyuan_doc_id: &str,
    markdown: &str,
) -> anyhow::Result<Option<String>> {
    let siyuan_doc_id = siyuan_doc_id.trim();
    if siyuan_doc_id.is_empty() {
        anyhow::bail!("siyuan_doc_id is required");
    }

    let content = markdown.to_string();
    let remote_hash = hex::encode(Sha256::digest(content.as_bytes()));
    let now = Utc::now().to_rfc3339();
    let conn = db.conn();

    let cached: Option<(String, String, String, String)> = conn
        .query_row(
            "SELECT id, title, summary, tags
             FROM knowledge_item
             WHERE siyuan_doc_id = ?1",
            params![siyuan_doc_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((knowledge_id, title, summary, tags_json)) = cached else {
        return Ok(None);
    };

    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result: anyhow::Result<()> = (|| {
        conn.execute(
            "DELETE FROM embedding_record
             WHERE chunk_id IN (SELECT id FROM knowledge_chunk WHERE knowledge_id = ?1)",
            params![&knowledge_id],
        )?;
        conn.execute(
            "DELETE FROM knowledge_chunk WHERE knowledge_id = ?1",
            params![&knowledge_id],
        )?;
        conn.execute(
            "DELETE FROM knowledge_fts WHERE knowledge_id = ?1",
            params![&knowledge_id],
        )?;
        conn.execute(
            "UPDATE knowledge_item
             SET content = ?2,
                 current_remote_hash = ?3,
                 managed_by = 'user',
                 updated_at = ?4
             WHERE id = ?1",
            params![&knowledge_id, &content, &remote_hash, &now],
        )?;
        conn.execute(
            "INSERT INTO knowledge_fts (knowledge_id, title, summary, content, tags)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&knowledge_id, &title, &summary, &content, &tags_json],
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
