//! Explicit, local-only project grouping overrides. A binding is a reviewed
//! display association, not an edit to canonical provider metadata or SiYuan.
use crate::knowledge::project_memory::project_identity;
use crate::storage::StateDb;
use anyhow::{bail, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSessionBinding {
    pub session_id: i64,
    pub project_id: String,
    pub updated_at: String,
}

pub struct ProjectSessionBindingService<'a> {
    db: &'a StateDb,
}

impl<'a> ProjectSessionBindingService<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    pub fn list(&self) -> Result<Vec<ProjectSessionBinding>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT b.session_id, b.project_path, b.updated_at
             FROM project_session_binding b
             JOIN source_session ss ON ss.id = b.session_id
             WHERE ss.is_missing = 0 ORDER BY b.updated_at DESC, b.session_id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let session_id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            let updated_at: String = row.get(2)?;
            Ok((session_id, path, updated_at))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (session_id, path, updated_at) = row?;
            let (project_id, _, verified) = project_identity("bound", "", Some(&path), None);
            if verified {
                result.push(ProjectSessionBinding {
                    session_id,
                    project_id,
                    updated_at,
                });
            }
        }
        Ok(result)
    }

    /// Only a currently existing verified absolute-path project may be used
    /// as a target. Never accept caller-supplied filesystem paths.
    pub fn bind(&self, session_id: i64, target_project_id: &str) -> Result<ProjectSessionBinding> {
        anyhow::ensure!(session_id > 0, "Invalid Session identity");
        anyhow::ensure!(
            target_project_id.starts_with("project:"),
            "Invalid project identity"
        );
        let now = chrono::Utc::now().to_rfc3339();
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let source: Option<i64> = tx
            .query_row(
                "SELECT id FROM source_session WHERE id = ?1 AND is_missing = 0",
                [session_id],
                |row| row.get(0),
            )
            .optional()?;
        anyhow::ensure!(source.is_some(), "Session is missing or unavailable");

        let target_path = {
            let mut stmt = tx.prepare(
                "SELECT source,external_session_id,project_path,project_name
                 FROM source_session
                 WHERE is_missing = 0 AND project_path IS NOT NULL
                 ORDER BY id",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?;
            let mut found = None;
            for row in rows {
                let (provider, external, path, name) = row?;
                let (id, _, verified) =
                    project_identity(&provider, &external, Some(&path), name.as_deref());
                if verified && id == target_project_id {
                    found = Some(path);
                    break;
                }
            }
            found
        };
        let Some(target_path) = target_path else {
            bail!("Target must be a currently discovered absolute-path project");
        };
        tx.execute(
            "INSERT INTO project_session_binding (session_id,project_path,updated_at)
             VALUES (?1,?2,?3)
             ON CONFLICT(session_id) DO UPDATE SET
             project_path = excluded.project_path, updated_at = excluded.updated_at",
            params![session_id, target_path, now],
        )?;
        tx.commit()?;
        Ok(ProjectSessionBinding {
            session_id,
            project_id: target_project_id.to_string(),
            updated_at: now,
        })
    }

    /// Clearing the local override restores the provider-derived project.
    pub fn clear(&self, session_id: i64) -> Result<bool> {
        anyhow::ensure!(session_id > 0, "Invalid Session identity");
        let removed = self.db.conn().execute(
            "DELETE FROM project_session_binding WHERE session_id = ?1",
            [session_id],
        )?;
        Ok(removed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::project_memory::ProjectMemoryService;
    use rusqlite::params;

    #[test]
    fn local_reassignment_persists_without_mutating_canonical_source_and_is_reversible() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        let db = StateDb::open(&path).unwrap();
        for (id, source, project_path) in [
            (1, "codex", Some("C:/work/team")),
            (2, "claude_code", None),
            (3, "opencode", Some("D:/work/unrelated")),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO source_session
                     (id,source,external_session_id,project_path,project_name,
                      last_seen_at,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,'team','now','now','now')",
                    params![id, source, format!("session-{id}"), project_path],
                )
                .unwrap();
        }
        db.conn()
            .execute(
                "UPDATE source_session SET project_name = 'misleading-name' WHERE id = 2",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO knowledge_item
                 (id,source_session_id,project_name,title,category,summary,content,
                  source_type,managed_by,status,created_at,updated_at)
                 VALUES ('k2',2,'team','Unassigned fix','decision','reviewed','details',
                         'conversation','pipeline','active','now','now')",
                [],
            )
            .unwrap();
        let project = ProjectMemoryService::new(&db)
            .list()
            .unwrap()
            .into_iter()
            .find(|p| p.sources == vec!["codex"])
            .unwrap();
        let service = ProjectSessionBindingService::new(&db);
        assert!(service.bind(2, "project:forged").is_err());
        assert!(service.bind(999, &project.id).is_err());
        assert!(service.bind(0, &project.id).is_err());
        let bound = service.bind(2, &project.id).unwrap();
        assert_eq!(bound.project_id, project.id);
        assert_eq!(service.list().unwrap().len(), 1);
        let canonical: Option<String> = db
            .conn()
            .query_row(
                "SELECT project_path FROM source_session WHERE id = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(canonical.is_none());
        let unchanged_name: String = db
            .conn()
            .query_row(
                "SELECT project_name FROM source_session WHERE id = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unchanged_name, "misleading-name");
        let grouped = ProjectMemoryService::new(&db).list().unwrap();
        assert_eq!(grouped.len(), 2);
        let target = grouped.iter().find(|p| p.id == project.id).unwrap();
        assert_eq!(target.session_count, 2);
        assert_eq!(target.sources, vec!["claude_code", "codex"]);
        assert_eq!(
            target.title, "team",
            "a manually reclassified Session must not rename its verified target project"
        );
        let target_memory = ProjectMemoryService::new(&db).get(&project.id, 50).unwrap();
        assert_eq!(target_memory.entries.len(), 1);
        assert_eq!(target_memory.entries[0].knowledge_id, "k2");
        drop(db);

        let reopened = StateDb::open(&path).unwrap();
        assert_eq!(
            ProjectSessionBindingService::new(&reopened)
                .list()
                .unwrap()
                .len(),
            1
        );
        assert!(ProjectSessionBindingService::new(&reopened)
            .clear(2)
            .unwrap());
        assert!(!ProjectSessionBindingService::new(&reopened)
            .clear(2)
            .unwrap());
        assert_eq!(
            ProjectMemoryService::new(&reopened).list().unwrap().len(),
            3
        );
        let restored = ProjectMemoryService::new(&reopened)
            .get(&project.id, 50)
            .unwrap();
        assert!(restored.entries.is_empty());
        drop(reopened);
        StateDb::open(&path).unwrap();
    }
}
