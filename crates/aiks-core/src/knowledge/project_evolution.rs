//! Read-only, reviewable project evolution. Never infer contradictions from
//! similar titles or rewrite earlier knowledge/Provider/SiYuan content.
use crate::knowledge::project_memory::{project_identity, ProjectMemoryService};
use crate::storage::StateDb;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEvolutionLink {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub source_title: String,
    pub target_title: String,
    pub relation_type: String,
    pub status: String,
    pub evidence: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEvolutionSnapshot {
    pub links: Vec<ProjectEvolutionLink>,
    pub truncated: bool,
}

pub struct ProjectEvolutionService<'a> {
    db: &'a StateDb,
}

impl<'a> ProjectEvolutionService<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    /// Resolve BOTH ends against their CURRENT canonical Session project IDs.
    /// Human-confirmed corrections are facts about the review, not proof the
    /// attached proposition is true. Suggested links remain visibly tentative.
    pub fn list(&self, project_id: &str, limit: usize) -> Result<ProjectEvolutionSnapshot> {
        let project_exists = ProjectMemoryService::new(self.db)
            .list()?
            .iter()
            .any(|project| project.id == project_id);
        anyhow::ensure!(project_exists, "Project identity not found");

        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT r.id,r.source_id,r.target_id,a.title,b.title,
                    r.relation_type,r.status,r.evidence,r.updated_at,
                    sa.source,sa.external_session_id,
                    COALESCE(pa.project_path,sa.project_path),sa.project_name,
                    sb.source,sb.external_session_id,
                    COALESCE(pb.project_path,sb.project_path),sb.project_name
             FROM knowledge_relation r
             JOIN knowledge_item a ON a.id = r.source_id
             JOIN knowledge_item b ON b.id = r.target_id
             JOIN source_session sa ON sa.id = a.source_session_id
             JOIN source_session sb ON sb.id = b.source_session_id
             LEFT JOIN project_session_binding pa ON pa.session_id = sa.id
             LEFT JOIN project_session_binding pb ON pb.session_id = sb.id
             WHERE r.relation_type IN ('supersedes','corrects','resolved_by')
               AND r.status IN ('confirmed','suggested')
               AND a.status = 'active' AND b.status = 'active'
               AND sa.is_missing = 0 AND sb.is_missing = 0
             ORDER BY r.updated_at DESC,r.id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                ProjectEvolutionLink {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    target_id: row.get(2)?,
                    source_title: row.get(3)?,
                    target_title: row.get(4)?,
                    relation_type: row.get(5)?,
                    status: row.get(6)?,
                    evidence: row.get(7)?,
                    updated_at: row.get(8)?,
                },
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, String>(14)?,
                row.get::<_, Option<String>>(15)?,
                row.get::<_, Option<String>>(16)?,
            ))
        })?;
        let mut links = Vec::new();
        let mut truncated = false;
        for row in rows {
            let (
                link,
                source,
                external,
                path,
                name,
                other_source,
                other_external,
                other_path,
                other_name,
            ) = row?;
            let (left, _, _) =
                project_identity(&source, &external, path.as_deref(), name.as_deref());
            let (right, _, _) = project_identity(
                &other_source,
                &other_external,
                other_path.as_deref(),
                other_name.as_deref(),
            );
            if left != project_id || right != project_id {
                continue;
            }
            if links.len() >= limit.clamp(1, 100) {
                truncated = true;
                break;
            }
            links.push(link);
        }
        Ok(ProjectEvolutionSnapshot { links, truncated })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn only_current_same_project_evidence_and_human_review_status_are_returned() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("evolution.db")).unwrap();
        for (sid, source, path) in [
            (1, "codex", "C:/repo/alpha"),
            (2, "claude_code", "c:/repo/alpha/"),
            (3, "opencode", "D:/repo/alpha"),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO source_session
                     (id,source,external_session_id,project_path,project_name,
                      last_seen_at,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,'alpha','now','now','now')",
                    params![sid, source, format!("session-{sid}"), path],
                )
                .unwrap();
            db.conn()
                .execute(
                    "INSERT INTO knowledge_item
                     (id,source_session_id,title,category,summary,content,
                      source_type,managed_by,status,created_at,updated_at)
                     VALUES (?1,?2,?1,'decision','summary','content',
                             'conversation','pipeline','active','now','now')",
                    params![format!("knowledge-{sid}"), sid],
                )
                .unwrap();
        }
        for (id, source, target, status, relation_type) in [
            (
                "reviewed",
                "knowledge-1",
                "knowledge-2",
                "confirmed",
                "corrects",
            ),
            (
                "pending",
                "knowledge-2",
                "knowledge-1",
                "suggested",
                "supersedes",
            ),
            (
                "excluded-cross",
                "knowledge-1",
                "knowledge-3",
                "confirmed",
                "corrects",
            ),
            (
                "excluded-rejected",
                "knowledge-1",
                "knowledge-2",
                "rejected",
                "supersedes",
            ),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO knowledge_relation
                     (id,source_id,target_id,relation_type,status,evidence,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,?5,'user-provided evidence','now','now')",
                    params![id, source, target, relation_type, status],
                )
                .unwrap();
        }
        let service = ProjectEvolutionService::new(&db);
        let project = ProjectMemoryService::new(&db)
            .list()
            .unwrap()
            .into_iter()
            .find(|item| item.sources.len() == 2)
            .unwrap();
        let links = service.list(&project.id, 100).unwrap();
        assert_eq!(links.links.len(), 2);
        assert!(links
            .links
            .iter()
            .any(|item| item.id == "reviewed" && item.status == "confirmed"));
        assert!(links
            .links
            .iter()
            .any(|item| item.id == "pending" && item.status == "suggested"));
        assert!(!links.truncated);
        let limited = service.list(&project.id, 1).unwrap();
        assert!(limited.truncated);
        assert_eq!(limited.links.len(), 1);
        assert!(service.list("project:unknown", 100).is_err());

        db.conn()
            .execute(
                "UPDATE source_session SET project_path = 'D:/repo/alpha'
                 WHERE id = 2",
                [],
            )
            .unwrap();
        assert!(service.list(&project.id, 100).unwrap().links.is_empty());

        db.conn()
            .execute(
                "UPDATE source_session SET project_path = 'C:/repo/alpha',
                 is_missing = 1 WHERE id = 2",
                [],
            )
            .unwrap();
        assert!(service.list(&project.id, 100).unwrap().links.is_empty());

        db.conn()
            .execute("UPDATE source_session SET is_missing = 0 WHERE id = 2", [])
            .unwrap();
        db.conn()
            .execute(
                "UPDATE knowledge_item SET status = 'archived' WHERE id = 'knowledge-2'",
                [],
            )
            .unwrap();
        assert!(service.list(&project.id, 100).unwrap().links.is_empty());
    }
}
