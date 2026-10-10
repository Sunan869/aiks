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

/// Current project scope of a knowledge item, resolved from the same
/// canonical Session identity used by Project Memory. We do not rely on
/// mutable display names when both absolute paths are available.
struct KnowledgeScope {
    project_name: Option<String>,
    verified_project_id: Option<String>,
    status: String,
}

fn load_knowledge_scope(
    conn: &rusqlite::Connection,
    knowledge_id: &str,
) -> Result<Option<KnowledgeScope>> {
    let scope = conn
        .query_row(
            "SELECT ki.project_name, ki.status, ss.source,
                    ss.external_session_id, ss.project_path
             FROM knowledge_item ki
             LEFT JOIN source_session ss ON ss.id = ki.source_session_id
             WHERE ki.id = ?1",
            [knowledge_id],
            |row| {
                let project_name: Option<String> = row.get(0)?;
                let status: String = row.get(1)?;
                let source: Option<String> = row.get(2)?;
                let external: Option<String> = row.get(3)?;
                let path: Option<String> = row.get(4)?;
                let verified_project_id =
                    match (source.as_deref(), external.as_deref(), path.as_deref()) {
                        (Some(source), Some(external), Some(path)) => {
                            let (id, _, verified) =
                                project_identity(source, external, Some(path), None);
                            verified.then_some(id)
                        }
                        _ => None,
                    };
                Ok(KnowledgeScope {
                    project_name,
                    verified_project_id,
                    status,
                })
            },
        )
        .optional()?;
    Ok(scope)
}

/// Validation is run both when suggesting a relation and immediately before
/// human confirmation. Project membership may have changed in between.
fn ensure_related_knowledge_is_current(
    conn: &rusqlite::Connection,
    source_id: &str,
    target_id: &str,
) -> Result<()> {
    let (Some(source), Some(target)) = (
        load_knowledge_scope(conn, source_id)?,
        load_knowledge_scope(conn, target_id)?,
    ) else {
        bail!("Related knowledge item was not found");
    };
    if source.status != "active" || target.status != "active" {
        bail!("Archived or deleted knowledge cannot establish a new relation");
    }
    if let (Some(left), Some(right)) = (
        source.verified_project_id.as_deref(),
        target.verified_project_id.as_deref(),
    ) {
        if left != right {
            bail!("Cross-project relation is not allowed for different project paths");
        }
    } else if let (Some(left), Some(right)) = (
        source.project_name.as_deref(),
        target.project_name.as_deref(),
    ) {
        if !left.trim().is_empty()
            && !right.trim().is_empty()
            && left.trim() != right.trim()
        {
            bail!("Cross-project relation needs an explicit project reassignment");
        }
    }
    Ok(())
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
        ensure_related_knowledge_is_current(&conn, source_id, target_id)?;
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
            let relation: Option<(String, String, String)> = tx
                .query_row(
                    "SELECT status, source_id, target_id
                     FROM knowledge_relation WHERE id = ?1",
                    [relation_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((previous, source_id, target_id)) = relation else {
                bail!("Knowledge relation was not found");
            };
            if next_status == "confirmed" && previous != "confirmed" {
                // Recheck inside the transaction: source knowledge may have
                // been archived or moved to a different project since review.
                ensure_related_knowledge_is_current(&tx, &source_id, &target_id)?;
            }
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
    fn human_confirmation_rechecks_project_identity_without_losing_review_history() {
        let temp = tempfile::tempdir().unwrap();
        let db = StateDb::open(&temp.path().join("changing-project.db")).unwrap();
        for (sid, source, path) in [
            (1, "codex", "C:/workspace/app"),
            (2, "claude", "c:/workspace/app/"),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO source_session
                     (id,source,external_session_id,project_path,project_name,
                      last_seen_at,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,'App',
                             '2026-01-01','2026-01-01','2026-01-01')",
                    params![sid, source, format!("source-{sid}"), path],
                )
                .unwrap();
            db.conn()
                .execute(
                    "INSERT INTO knowledge_item
                     (id,source_session_id,project_name,title,content,
                      source_type,managed_by,created_at,updated_at)
                     VALUES (?1,?2,'App','Fix','Evidence',
                             'conversation','pipeline','2026-01-01','2026-01-01')",
                    params![format!("item-{sid}"), sid],
                )
                .unwrap();
        }

        let repo = RelationRepo::new(&db);
        let proposed = repo
            .suggest("item-1", "item-2", "supersedes", "reviewed evidence", None)
            .unwrap();
        db.conn()
            .execute(
                "UPDATE source_session SET project_path = 'D:/workspace/app'
                 WHERE id = 2",
                [],
            )
            .unwrap();
        assert!(repo.review(&proposed.id, "confirmed").is_err());
        assert_eq!(repo.list("item-1").unwrap()[0].status, "suggested");
        let audit_count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM knowledge_relation_review
                 WHERE relation_id = ?1",
                [&proposed.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_count, 0, "failed confirmation must not write audit");
        assert_eq!(repo.review(&proposed.id, "rejected").unwrap().status, "rejected");
        assert_eq!(repo.review(&proposed.id, "rejected").unwrap().status, "rejected");
        let review_count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM knowledge_relation_review
                 WHERE relation_id = ?1",
                [&proposed.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(review_count, 1, "repeat review must be idempotent");

        db.conn()
            .execute(
                "UPDATE source_session SET project_path = 'C:/workspace/app'
                 WHERE id = 2",
                [],
            )
            .unwrap();
        assert_eq!(repo.review(&proposed.id, "confirmed").unwrap().status, "confirmed");
        let review_count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM knowledge_relation_review
                 WHERE relation_id = ?1",
                [&proposed.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(review_count, 2);
    }

    #[test]
    fn archived_source_cannot_be_suggested_or_confirmed_but_can_be_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let db = StateDb::open(&temp.path().join("archived-relations.db")).unwrap();
        add_knowledge(&db, "first", "alpha");
        add_knowledge(&db, "second", "alpha");
        let repo = RelationRepo::new(&db);
        let proposed = repo
            .suggest("first", "second", "related", "source evidence", None)
            .unwrap();
        db.conn()
            .execute(
                "UPDATE knowledge_item SET status = 'archived' WHERE id = 'second'",
                [],
            )
            .unwrap();
        assert!(repo
            .suggest("first", "second", "corrects", "new evidence", None)
            .is_err());
        assert!(repo.review(&proposed.id, "confirmed").is_err());
        assert_eq!(repo.review(&proposed.id, "rejected").unwrap().status, "rejected");
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
