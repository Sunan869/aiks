//! Auditable, conservative cross-session knowledge relations.
//! This store never rewrites KnowledgeItem identity or deletes source evidence.
//! Suggested relations require explicit human confirmation or rejection.
use crate::knowledge::project_memory::project_identity;
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
        // Display names are not unique project identities. If both knowledge
        // items belong to sessions with verified absolute project paths,
        // compare the same stable identity used by Project Memory.
        let load_project =
            |knowledge_id: &str| -> Result<Option<(Option<String>, Option<String>)>> {
                let endpoint: Option<(
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                )> = conn
                    .query_row(
                        "SELECT ki.project_name, ss.source, ss.external_session_id, ss.project_path
                         FROM knowledge_item ki
                         LEFT JOIN source_session ss ON ss.id = ki.source_session_id
                         WHERE ki.id = ?1",
                        [knowledge_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .optional()?;
                Ok(endpoint.map(|(project, source, external, path)| {
                    let verified = match (source.as_deref(), external.as_deref(), path.as_deref()) {
                        (Some(source), Some(external), Some(path)) => {
                            let (id, _, is_verified) =
                                project_identity(source, external, Some(path), None);
                            is_verified.then_some(id)
                        }
                        _ => None,
                    };
                    (project, verified)
                }))
            };
        let (Some((source_project, source_identity)), Some((target_project, target_identity))) =
            (load_project(source_id)?, load_project(target_id)?)
        else {
            bail!("Related knowledge item was not found");
        };
        if let (Some(source), Some(target)) = (&source_identity, &target_identity) {
            if source != target {
                bail!("Cross-project relation is not allowed for different project paths");
            }
        } else if let (Some(source), Some(target)) = (&source_project, &target_project) {
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
    fn relation_rejects_same_named_projects_in_different_directories() {
        let temp = tempfile::tempdir().unwrap();
        let db = StateDb::open(&temp.path().join("path-relations.db")).unwrap();
        for (sid, source, project_path, name) in [
            (1, "codex", "C:/work/project-a", "Shared"),
            (2, "claude", "D:/work/project-a", "Shared"),
            (3, "opencode", "c:/work/project-a/", "Different display"),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO source_session
                     (id,source,external_session_id,project_path,project_name,
                      last_seen_at,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,?5,
                             '2026-01-01','2026-01-01','2026-01-01')",
                    params![sid, source, format!("session-{sid}"), project_path, name],
                )
                .unwrap();
            db.conn()
                .execute(
                    "INSERT INTO knowledge_item
                     (id,source_session_id,project_name,title,content,
                      source_type,managed_by,created_at,updated_at)
                     VALUES (?1,?2,?3,'Decision','Evidence',
                             'conversation','pipeline','2026-01-01','2026-01-01')",
                    params![format!("knowledge-{sid}"), sid, name],
                )
                .unwrap();
        }
        let relations = RelationRepo::new(&db);
        assert!(relations
            .suggest(
                "knowledge-1",
                "knowledge-2",
                "related",
                "matching topic",
                None
            )
            .is_err());
        // Path identity wins over mutable display-name differences.
        let same_project = relations
            .suggest(
                "knowledge-1",
                "knowledge-3",
                "related",
                "same absolute path",
                None,
            )
            .unwrap();
        assert_eq!(same_project.status, "suggested");
        assert_eq!(relations.list("knowledge-1").unwrap().len(), 1);
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
