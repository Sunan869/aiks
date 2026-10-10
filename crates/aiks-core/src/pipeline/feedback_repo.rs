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

const KINDS: &[&str] = &[
    "useful",
    "incorrect",
    "duplicate",
    "outdated",
    "needs_detail",
];

pub struct FeedbackRepo<'a> {
    db: &'a StateDb,
}

impl<'a> FeedbackRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    /// Record feedback without overwriting the knowledge item or earlier reviews.
    pub fn add(&self, knowledge_id: &str, kind: &str, note: &str) -> Result<KnowledgeFeedback> {
        if !KINDS.contains(&kind) {
            bail!("Unsupported knowledge feedback kind");
        }
        if note.chars().count() > 4000 {
            bail!("Feedback note exceeds 4000 characters");
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        self.db
            .conn()
            .execute(
                "INSERT INTO knowledge_feedback (id, knowledge_id, kind, note, created_at)
             SELECT ?1, id, ?3, ?4, ?5 FROM knowledge_item WHERE id = ?2",
                params![id, knowledge_id, kind, note, created_at],
            )
            .and_then(|count| {
                if count == 1 {
                    Ok(count)
                } else {
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
    fn feedback_history_survives_knowledge_removal_and_database_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("feedback-persist.db");
        {
            let db = crate::storage::StateDb::open(&db_path).unwrap();
            db.conn()
                .execute(
                    "INSERT INTO knowledge_item
                     (id, title, content, source_type, managed_by, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'manual', 'user', ?4, ?4)",
                    rusqlite::params!["review-target", "Target", "Body", "2026-01-01T00:00:00Z"],
                )
                .unwrap();
            let repo = super::FeedbackRepo::new(&db);
            repo.add("review-target", "incorrect", "Root cause was different")
                .unwrap();
            repo.add("review-target", "needs_detail", "Add source evidence")
                .unwrap();
            assert_eq!(repo.list("review-target").unwrap().len(), 2);
            db.conn()
                .execute(
                    "DELETE FROM knowledge_item WHERE id = ?1",
                    ["review-target"],
                )
                .unwrap();
            assert_eq!(repo.list("review-target").unwrap().len(), 2);
        }
        let reopened = crate::storage::StateDb::open(&db_path).unwrap();
        let history = super::FeedbackRepo::new(&reopened)
            .list("review-target")
            .unwrap();
        assert_eq!(history.len(), 2);
        assert!(history.iter().any(|item| item.kind == "incorrect"));
        assert!(history.iter().any(|item| item.kind == "needs_detail"));
    }

    #[test]
    fn feedback_rejects_unknown_item_and_invalid_input_without_changes() {
        let temp = tempfile::tempdir().unwrap();
        let db = crate::storage::StateDb::open(&temp.path().join("feedback.db")).unwrap();
        let repo = super::FeedbackRepo::new(&db);
        assert!(repo
            .add("missing-item", "incorrect", "needs verification")
            .is_err());
        assert!(repo.add("missing-item", "delete", "").is_err());
        assert!(repo
            .add("missing-item", "useful", &"x".repeat(4001))
            .is_err());
        assert!(repo.list("missing-item").unwrap().is_empty());
    }
}
