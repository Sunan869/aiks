//! Durable, local-only feedback on extracted knowledge.
//! Feedback is append-only and deliberately separate from pipeline-managed rows,
//! so re-extraction cannot erase a user's correction or review decision.
//! Knowledge IDs are validated on write; history remains if a pipeline-owned
//! knowledge row is later removed during re-extraction.
use crate::storage::StateDb;
use anyhow::{bail, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeFeedback {
    pub id: String,
    pub knowledge_id: String,
    pub kind: String,
    pub note: String,
    pub created_at: String,
}

const KINDS: &[&str] = &["useful", "incorrect", "duplicate", "outdated", "needs_detail"];

pub struct FeedbackRepo<'a> {
    db: &'a StateDb,
}

impl<'a> FeedbackRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    fn ensure_schema(&self) -> Result<()> {
        self.db.conn().execute_batch(
            "CREATE TABLE IF NOT EXISTS knowledge_feedback (
                id TEXT PRIMARY KEY,
                knowledge_id TEXT NOT NULL,
                kind TEXT NOT NULL CHECK (kind IN ('useful','incorrect','duplicate','outdated','needs_detail')),
                note TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_knowledge_feedback_item_time
                ON knowledge_feedback(knowledge_id, created_at DESC);",
        )?;
        Ok(())
    }

    /// Record feedback without overwriting the knowledge item or earlier reviews.
    pub fn add(&self, knowledge_id: &str, kind: &str, note: &str) -> Result<KnowledgeFeedback> {
        if !KINDS.contains(&kind) {
            bail!("Unsupported knowledge feedback kind");
        }
        if note.chars().count() > 4000 {
            bail!("Feedback note exceeds 4000 characters");
        }
        self.ensure_schema()?;
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        self.db.conn().execute(
            "INSERT INTO knowledge_feedback (id, knowledge_id, kind, note, created_at)
             SELECT ?1, id, ?3, ?4, ?5 FROM knowledge_item WHERE id = ?2",
            params![id, knowledge_id, kind, note, created_at],
        ).and_then(|count| {
            if count == 1 { Ok(count) } else {
                Err(rusqlite::Error::QueryReturnedNoRows)
            }
        })?;
        Ok(KnowledgeFeedback {
            id,
            knowledge_id: knowledge_id.to_string(),
            kind: kind.to_string(),
            note: note.to_string(),
            created_at,
        })
    }

    pub fn list(&self, knowledge_id: &str) -> Result<Vec<KnowledgeFeedback>> {
        self.ensure_schema()?;
        let conn = self.db.conn();
        let mut statement = conn.prepare(
            "SELECT id, knowledge_id, kind, note, created_at FROM knowledge_feedback
             WHERE knowledge_id = ?1 ORDER BY created_at DESC, id DESC",
        )?;
        let feedback = statement
            .query_map([knowledge_id], |row| {
                Ok(KnowledgeFeedback {
                    id: row.get(0)?,
                    knowledge_id: row.get(1)?,
                    kind: row.get(2)?,
                    note: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(feedback)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn allowed_feedback_types_are_explicit() {
        assert!(super::KINDS.contains(&"incorrect"));
        assert!(super::KINDS.contains(&"needs_detail"));
        assert!(!super::KINDS.contains(&"delete"));
    }

    #[test]
    fn feedback_rejects_unknown_item_and_invalid_input_without_changes() {
        let temp = tempfile::tempdir().unwrap();
        let db = crate::storage::StateDb::open(&temp.path().join("feedback.db")).unwrap();
        let repo = super::FeedbackRepo::new(&db);
        assert!(repo.add("missing-item", "incorrect", "needs verification").is_err());
        assert!(repo.add("missing-item", "delete", "").is_err());
        assert!(repo.add("missing-item", "useful", &"x".repeat(4001)).is_err());
        assert!(repo.list("missing-item").unwrap().is_empty());
    }
}
