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

/// Immutable human review log entry. Review timestamps are NOT dates when
/// an engineering decision took effect; no validity claims are inferred.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeRelationReview {
    pub id: String,
    pub relation_id: String,
    pub source_id: String,
    pub target_id: String,
    pub previous_status: String,
    pub next_status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRelationReviewHistory {
    pub reviews: Vec<KnowledgeRelationReview>,
    pub truncated: bool,
}

/// Canonical knowledge provenance. An unknown Session path does not become
/// a verified project just because two display names match.
struct KnowledgeScope {
    project_name: Option<String>,
    source_session_id: Option<i64>,
    is_missing: bool,
    verified_project_id: Option<String>,
    status: String,
}

fn load_knowledge_scope(
    conn: &rusqlite::Connection,
    knowledge_id: &str,
) -> Result<Option<KnowledgeScope>> {
    let scope = conn
        .query_row(
            "SELECT ki.project_name,ki.status,ki.source_session_id,
                    ss.is_missing,ss.source,ss.external_session_id,
                    COALESCE(pb.project_path,ss.project_path)
             FROM knowledge_item ki
             LEFT JOIN source_session ss ON ss.id = ki.source_session_id
             LEFT JOIN project_session_binding pb ON pb.session_id = ss.id
             WHERE ki.id = ?1",
            [knowledge_id],
            |row| {
                let project_name: Option<String> = row.get(0)?;
                let status: String = row.get(1)?;
                let source_session_id: Option<i64> = row.get(2)?;
                let missing_flag: Option<i64> = row.get(3)?;
                let source: Option<String> = row.get(4)?;
                let external: Option<String> = row.get(5)?;
                let path: Option<String> = row.get(6)?;
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
                    source_session_id,
                    is_missing: source_session_id.is_some() && missing_flag != Some(0),
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
    anyhow::ensure!(
        !source.is_missing && !target.is_missing,
        "Missing source Session cannot establish a knowledge relation"
    );
    match (
        source.verified_project_id.as_deref(),
        target.verified_project_id.as_deref(),
    ) {
        (Some(left), Some(right)) => {
            anyhow::ensure!(
                left == right,
                "Cross-project relation is not allowed for different project paths"
            );
        }
        (None, None) => {
            let same_session = source.source_session_id.is_some()
                && source.source_session_id == target.source_session_id;
            // Manual-only Knowledge has no Provider Session; explicit manual
            // project labels still permit a user-authored relation. Never
            // treat the same name of TWO distinct provider Sessions as proof.
            let both_manual =
                source.source_session_id.is_none() && target.source_session_id.is_none();
            let same_manual_project = both_manual
                && match (
                    source.project_name.as_deref(),
                    target.project_name.as_deref(),
                ) {
                    (Some(left), Some(right)) => {
                        !left.trim().is_empty() && left.trim() == right.trim()
                    }
                    _ => false,
                };
            anyhow::ensure!(
                same_session || same_manual_project,
                "Unverified source projects require the same Session or two explicit manual project labels"
            );
        }
        _ => bail!("Mixed verified and unverified projects need explicit source attribution"),
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

    /// Historical review decisions for one knowledge endpoint, including
    /// rejected relationships and those whose original Session later moved.
    /// Result is bounded; no deleted source text or Provider content is loaded.
    pub fn review_history(
        &self,
        knowledge_id: &str,
        limit: usize,
    ) -> Result<KnowledgeRelationReviewHistory> {
        anyhow::ensure!(
            !knowledge_id.trim().is_empty(),
            "Review history requires a knowledge identity"
        );
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT rr.id,rr.relation_id,r.source_id,r.target_id,
                    rr.previous_status,rr.next_status,rr.created_at
             FROM knowledge_relation_review rr
             JOIN knowledge_relation r ON r.id = rr.relation_id
             WHERE r.source_id = ?1 OR r.target_id = ?1
             ORDER BY rr.created_at DESC,rr.id DESC
             LIMIT ?2",
        )?;
        let cap = limit.clamp(1, 100);
        let rows = stmt.query_map(params![knowledge_id, (cap + 1) as i64], |row| {
            Ok(KnowledgeRelationReview {
                id: row.get(0)?,
                relation_id: row.get(1)?,
                source_id: row.get(2)?,
                target_id: row.get(3)?,
                previous_status: row.get(4)?,
                next_status: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        let mut reviews = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        let truncated = reviews.len() > cap;
        reviews.truncate(cap);
        Ok(KnowledgeRelationReviewHistory { reviews, truncated })
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
        assert_eq!(
            repo.review(&proposed.id, "rejected").unwrap().status,
            "rejected"
        );
        assert_eq!(
            repo.review(&proposed.id, "rejected").unwrap().status,
            "rejected"
        );
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
        assert_eq!(
            repo.review(&proposed.id, "confirmed").unwrap().status,
            "confirmed"
        );
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
        assert_eq!(
            repo.review(&proposed.id, "rejected").unwrap().status,
            "rejected"
        );
    }

    #[test]
    fn pathless_different_provider_sessions_cannot_be_linked_by_a_shared_name() {
        let temp = tempfile::tempdir().unwrap();
        let db = StateDb::open(&temp.path().join("pathless-relations.db")).unwrap();
        for (sid, source, path) in [
            (1, "codex", None),
            (2, "claude_code", None),
            (3, "opencode", Some("C:/repo/one")),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO source_session
                     (id,source,external_session_id,project_path,project_name,
                      last_seen_at,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,'same-display','now','now','now')",
                    params![sid, source, format!("external-{sid}"), path],
                )
                .unwrap();
            db.conn()
                .execute(
                    "INSERT INTO knowledge_item
                     (id,source_session_id,project_name,title,content,
                      source_type,managed_by,created_at,updated_at)
                     VALUES (?1,?2,'same-display','Evidence','Body',
                             'conversation','pipeline','now','now')",
                    params![format!("k{sid}"), sid],
                )
                .unwrap();
        }
        db.conn()
            .execute(
                "INSERT INTO knowledge_item
                 (id,source_session_id,project_name,title,content,
                  source_type,managed_by,created_at,updated_at)
                 VALUES ('k4',1,'same-display','Other evidence','Body',
                         'conversation','pipeline','now','now')",
                [],
            )
            .unwrap();

        add_knowledge(&db, "manual-only", "same-display");
        let repo = RelationRepo::new(&db);
        assert!(repo
            .suggest("k1", "k2", "corrects", "same display is insufficient", None)
            .is_err());
        assert!(repo
            .suggest("k3", "manual-only", "related", "mixed identity", None)
            .is_err());
        assert!(repo
            .suggest("k1", "k3", "corrects", "mixed identity", None)
            .is_err());
        let within_session = repo
            .suggest("k1", "k4", "related", "one source", None)
            .unwrap();
        assert_eq!(
            repo.review(&within_session.id, "confirmed").unwrap().status,
            "confirmed"
        );

        let manual = repo.suggest("k1", "k4", "related", "duplicate", None);
        assert!(manual.is_err());
        db.conn()
            .execute(
                "INSERT INTO project_session_binding(session_id,project_path,updated_at)
                 VALUES (2,'C:/repo/one','now')",
                [],
            )
            .unwrap();
        assert!(repo
            .suggest("k2", "k3", "corrects", "same reviewed project", None)
            .is_ok());
        assert!(repo
            .suggest("k1", "k2", "corrects", "unverified vs bound", None)
            .is_err());

        db.conn()
            .execute("UPDATE source_session SET is_missing = 1 WHERE id = 2", [])
            .unwrap();
        assert!(repo
            .suggest("k2", "k3", "corrects", "missing source", None)
            .is_err());
    }

    #[test]
    fn missing_source_after_proposal_blocks_confirmation_but_keeps_rejection() {
        let temp = tempfile::tempdir().unwrap();
        let db = StateDb::open(&temp.path().join("missing-relation.db")).unwrap();
        for sid in [1, 2] {
            db.conn()
                .execute(
                    "INSERT INTO source_session
                     (id,source,external_session_id,project_path,project_name,
                      last_seen_at,created_at,updated_at)
                     VALUES (?1,'codex',?2,'C:/repo/shared','shared','now','now','now')",
                    params![sid, format!("ext-{sid}")],
                )
                .unwrap();
            db.conn()
                .execute(
                    "INSERT INTO knowledge_item
                     (id,source_session_id,project_name,title,content,
                      source_type,managed_by,created_at,updated_at)
                     VALUES (?1,?2,'shared','Knowledge','Text',
                             'conversation','pipeline','now','now')",
                    params![format!("k{sid}"), sid],
                )
                .unwrap();
        }
        let repo = RelationRepo::new(&db);
        let relation = repo
            .suggest("k1", "k2", "supersedes", "reviewed evidence", None)
            .unwrap();
        db.conn()
            .execute("UPDATE source_session SET is_missing = 1 WHERE id = 2", [])
            .unwrap();
        assert!(repo.review(&relation.id, "confirmed").is_err());
        assert_eq!(repo.list("k1").unwrap()[0].status, "suggested");
        assert_eq!(
            repo.review(&relation.id, "rejected").unwrap().status,
            "rejected"
        );
    }

    #[test]
    fn review_history_is_durable_ordered_bounded_and_includes_rejected_reviews() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("review-history.db");
        let db = StateDb::open(&path).unwrap();
        add_knowledge(&db, "source", "manual-scope");
        add_knowledge(&db, "target", "manual-scope");
        let repo = RelationRepo::new(&db);
        let proposed = repo
            .suggest("source", "target", "corrects", "review evidence", None)
            .unwrap();
        assert!(repo.review_history("source", 100).unwrap().reviews.is_empty());
        assert!(repo.review_history("", 100).is_err());
        repo.review(&proposed.id, "confirmed").unwrap();
        repo.review(&proposed.id, "rejected").unwrap();
        repo.review(&proposed.id, "rejected").unwrap();
        let all = repo.review_history("source", 100).unwrap();
        assert_eq!(all.reviews.len(), 2);
        assert!(!all.truncated);
        assert!(all.reviews.iter().any(|r| {
            r.source_id == "source"
                && r.target_id == "target"
                && r.previous_status == "suggested"
                && r.next_status == "confirmed"
        }));
        assert!(all.reviews.iter().any(|r| r.next_status == "rejected"));
        let one = repo.review_history("target", 1).unwrap();
        assert_eq!(one.reviews.len(), 1);
        assert!(one.truncated);
        drop(db);

        let db = StateDb::open(&path).unwrap();
        let reopened = RelationRepo::new(&db).review_history("target", 100).unwrap();
        assert_eq!(reopened.reviews.len(), 2);
        assert!(reopened.reviews.iter().any(|r| r.next_status == "rejected"));
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
