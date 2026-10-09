//! Auditable, conservative cross-session knowledge relations.
//! This store never rewrites KnowledgeItem identity or deletes source evidence.
//! Suggested relations require explicit human confirmation or rejection.
use crate::storage::StateDb;
use anyhow::{bail, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

const RELATION_TYPES: &[&str] = &[
    "related",
    "supplements",
    "corrects",
    "supersedes",
    "resolved_by",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRelation {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    pub status: String,
    pub evidence: String,
    pub confidence: Option<f32>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct RelationRepo<'a> {
    db: &'a StateDb,
}

impl<'a> RelationRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    /// Store only an evidence-bearing suggestion; no automatic assertion.
    pub fn suggest(
        &self,
        source_id: &str,
        target_id: &str,
        relation_type: &str,
        evidence: &str,
        confidence: Option<f32>,
    ) -> Result<KnowledgeRelation> {
        if source_id == target_id || source_id.trim().is_empty() || target_id.trim().is_empty() {
            bail!("A relation requires two distinct knowledge identities");
        }
        if !RELATION_TYPES.contains(&relation_type) {
            bail!("Unsupported relation type");
        }
        if evidence.trim().is_empty() || evidence.chars().count() > 4000 {
            bail!("Evidence must contain 1 to 4000 characters");
        }
        if let Some(score) = confidence {
            if !score.is_finite() || !(0.0..=1.0).contains(&score) {
                bail!("Confidence must be within 0..1");
            }
        }
        let conn = self.db.conn();
        let source_project: Option<Option<String>> = conn
            .query_row(
                "SELECT project_name FROM knowledge_item WHERE id = ?1",
                [source_id],
                |row| row.get(0),
            )
            .optional()?;
        let target_project: Option<Option<String>> = conn
            .query_row(
                "SELECT project_name FROM knowledge_item WHERE id = ?1",
                [target_id],
                |row| row.get(0),
            )
            .optional()?;
        let (Some(source_project), Some(target_project)) = (source_project, target_project) else {
            bail!("Related knowledge item was not found");
        };
        if let (Some(source), Some(target)) = (&source_project, &target_project) {
            if !source.trim().is_empty() && !target.trim().is_empty() && source != target {
                bail!("Cross-project relation needs an explicit project reassignment");
            }
        }
        let now = chrono::Utc::now().to_rfc3339();
        let relation = KnowledgeRelation {
            id: uuid::Uuid::new_v4().to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            relation_type: relation_type.to_string(),
            status: "suggested".to_string(),
            evidence: evidence.trim().to_string(),
            confidence,
            created_at: now.clone(),
            updated_at: now,
        };
        conn.execute(
            "INSERT INTO knowledge_relation
             (id,source_id,target_id,relation_type,status,evidence,confidence,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                relation.id,
                relation.source_id,
                relation.target_id,
                relation.relation_type,
                relation.status,
                relation.evidence,
                relation.confidence,
                relation.created_at,
                relation.updated_at
            ],
        )?;
        Ok(relation)
    }

    pub fn list(&self, knowledge_id: &str) -> Result<Vec<KnowledgeRelation>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id,source_id,target_id,relation_type,status,evidence,confidence,created_at,updated_at
             FROM knowledge_relation WHERE source_id = ?1 OR target_id = ?1
             ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = stmt
            .query_map([knowledge_id], |row| {
                Ok(KnowledgeRelation {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    target_id: row.get(2)?,
                    relation_type: row.get(3)?,
                    status: row.get(4)?,
                    evidence: row.get(5)?,
                    confidence: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Human decision is transactional and append-only in the review log.
    pub fn review(&self, relation_id: &str, next_status: &str) -> Result<KnowledgeRelation> {
        if !matches!(next_status, "confirmed" | "rejected") {
            bail!("Only confirmed or rejected review decisions are accepted");
        }
        {
            let mut conn = self.db.conn();
            let tx = conn.transaction()?;
            let previous: Option<String> = tx
                .query_row(
                    "SELECT status FROM knowledge_relation WHERE id = ?1",
                    [relation_id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(previous) = previous else {
                bail!("Knowledge relation was not found");
            };
            if previous != next_status {
                let now = chrono::Utc::now().to_rfc3339();
                tx.execute(
                    "UPDATE knowledge_relation SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    params![next_status, now, relation_id],
                )?;
                tx.execute(
                    "INSERT INTO knowledge_relation_review
                     (id,relation_id,previous_status,next_status,created_at)
                     VALUES (?1,?2,?3,?4,?5)",
                    params![
                        uuid::Uuid::new_v4().to_string(),
                        relation_id,
                        previous,
                        next_status,
                        now
                    ],
                )?;
            }
            tx.commit()?;
        }
        self.list_by_id(relation_id)?
            .ok_or_else(|| anyhow::anyhow!("Knowledge relation disappeared"))
    }

    fn list_by_id(&self, relation_id: &str) -> Result<Option<KnowledgeRelation>> {
        let conn = self.db.conn();
        Ok(conn
            .query_row(
                "SELECT id,source_id,target_id,relation_type,status,evidence,confidence,created_at,updated_at
                 FROM knowledge_relation WHERE id = ?1",
                [relation_id],
                |row| {
                    Ok(KnowledgeRelation {
                        id: row.get(0)?,
                        source_id: row.get(1)?,
                        target_id: row.get(2)?,
                        relation_type: row.get(3)?,
                        status: row.get(4)?,
                        evidence: row.get(5)?,
                        confidence: row.get(6)?,
                        created_at: row.get(7)?,
                        updated_at: row.get(8)?,
                    })
                },
            )
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add_knowledge(db: &StateDb, id: &str, project: &str) {
        db.conn()
            .execute(
                "INSERT INTO knowledge_item
                 (id,project_name,title,content,source_type,managed_by,created_at,updated_at)
                 VALUES (?1,?2,?3,'Text','manual','user','2026-01-01','2026-01-01')",
                params![id, project, id],
            )
            .unwrap();
    }

    #[test]
    fn relation_requires_evidence_human_review_and_project_isolation() {
        let temp = tempfile::tempdir().unwrap();
        let db = StateDb::open(&temp.path().join("relations.db")).unwrap();
        add_knowledge(&db, "first", "alpha");
        add_knowledge(&db, "second", "alpha");
        add_knowledge(&db, "third", "beta");
        let repo = RelationRepo::new(&db);
        assert!(repo
            .suggest("first", "first", "related", "evidence", None)
            .is_err());
        assert!(repo
            .suggest("first", "second", "corrects", "", None)
            .is_err());
        assert!(repo
            .suggest("first", "third", "corrects", "evidence", None)
            .is_err());
        let relation = repo
            .suggest(
                "first",
                "second",
                "corrects",
                "Source session confirms the fix",
                Some(0.8),
            )
            .unwrap();
        assert_eq!(relation.status, "suggested");
        assert!(repo
            .suggest("first", "second", "corrects", "duplicate", None)
            .is_err());
        let reviewed = repo.review(&relation.id, "confirmed").unwrap();
        assert_eq!(reviewed.status, "confirmed");
        assert_eq!(repo.list("first").unwrap().len(), 1);
        assert_eq!(repo.list("second").unwrap().len(), 1);
        let count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM knowledge_relation_review WHERE relation_id = ?1",
                [&relation.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            repo.review(&relation.id, "rejected").unwrap().status,
            "rejected"
        );
        assert!(repo.review(&relation.id, "suggested").is_err());
    }
}
