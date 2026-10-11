//! Authoritative cross-knowledge project boundary for AI-assisted organization.
//! A mutable project display name is not evidence that two Sessions belong
//! to the same project. This guard reads only canonical local metadata.
use crate::knowledge::project_memory::project_identity;
use crate::storage::StateDb;
use anyhow::{bail, Result};
use rusqlite::{params, OptionalExtension};
use std::collections::HashSet;

pub struct OrganizationScopeGuard<'a> {
    db: &'a StateDb,
}

impl<'a> OrganizationScopeGuard<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    /// Called before requesting AI output AND again immediately before
    /// creating an independent, user-reviewed knowledge document.
    /// Strict source scope is an extra boundary, not a content-version lock.
    pub fn validate(&self, knowledge_ids: &[String]) -> Result<()> {
        anyhow::ensure!(
            !knowledge_ids.is_empty() && knowledge_ids.len() <= 5,
            "Organization requires 1 to 5 source knowledge items"
        );
        let mut unique = HashSet::new();
        let conn = self.db.conn();
        let mut first_scope: Option<(String, bool, i64)> = None;
        for id in knowledge_ids {
            anyhow::ensure!(
                !id.is_empty() && id.len() <= 256
                    && id.chars().all(|ch| ch.is_ascii_alphanumeric()
                        || matches!(ch, '-' | '_' | '.' | ':')),
                "Invalid organization knowledge identity"
            );
            anyhow::ensure!(unique.insert(id), "Duplicate organization source");
            let row = conn
                .query_row(
                    "SELECT ki.status,ki.source_session_id,
                            ss.source,ss.external_session_id,
                            COALESCE(pb.project_path,ss.project_path),ss.is_missing
                     FROM knowledge_item ki
                     LEFT JOIN source_session ss ON ss.id = ki.source_session_id
                     LEFT JOIN project_session_binding pb ON pb.session_id = ss.id
                     WHERE ki.id = ?1",
                    params![id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<i64>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<i64>>(5)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("Organization source knowledge was not found"))?;
            let (status, session_id, provider, external_id, path, is_missing) = row;
            anyhow::ensure!(status == "active", "Organization source is not active");
            if session_id.is_some() {
                anyhow::ensure!(
                    is_missing == Some(0),
                    "Organization source Session is missing"
                );
            }
            if let Some((prior_id, prior_verified, prior_session_id)) = &first_scope {
                let (Some(source), Some(external), Some(current_sid)) =
                    (provider.as_deref(), external_id.as_deref(), session_id)
                else {
                    bail!("Multi-source organization requires traceable Session evidence");
                };
                let (current_id, _, verified) =
                    project_identity(source, external, path.as_deref(), None);
                anyhow::ensure!(
                    (*prior_verified && verified && *prior_id == current_id)
                        || (*prior_session_id > 0 && *prior_session_id == current_sid),
                    "Different or unverified source projects cannot be merged"
                );
            } else {
                let (scope_id, verified) = match (provider.as_deref(), external_id.as_deref()) {
                    (Some(source), Some(external)) => {
                        let (id, _, verified) =
                            project_identity(source, external, path.as_deref(), None);
                        (id, verified)
                    }
                    _ => (String::new(), false),
                };
                first_scope = Some((scope_id, verified, session_id.unwrap_or(0)));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn seed(db: &StateDb) {
        for (sid, source, path, name) in [
            (1, "codex", "C:/source/project", "Same Display"),
            (2, "claude_code", "D:/other/project", "Same Display"),
            (3, "opencode", "c:/source/project/", "Unrelated Name"),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO source_session
                     (id,source,external_session_id,project_path,project_name,
                      last_seen_at,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,?5,'now','now','now')",
                    params![sid, source, format!("session-{sid}"), path, name],
                )
                .unwrap();
            db.conn()
                .execute(
                    "INSERT INTO knowledge_item
                     (id,source_session_id,project_name,title,category,summary,
                      content,source_type,managed_by,status,created_at,updated_at)
                     VALUES (?1,?2,?3,'Fix','decision','Summary','Body',
                             'conversation','pipeline','active','now','now')",
                    params![format!("k{sid}"), sid, name],
                )
                .unwrap();
        }
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn rejects_same_named_different_paths_and_accepts_current_canonical_identity() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("scope.db")).unwrap();
        seed(&db);
        let guard = OrganizationScopeGuard::new(&db);
        assert!(guard.validate(&ids(&["k1", "k3"])).is_ok());
        assert!(guard.validate(&ids(&["k1", "k2"])).is_err());
        assert!(guard.validate(&ids(&["k1", "k1"])).is_err());
        assert!(guard.validate(&ids(&["k1", "missing"])).is_err());
        assert!(guard.validate(&ids(&[])).is_err());
        db.conn()
            .execute(
                "UPDATE knowledge_item SET status = 'archived' WHERE id = 'k3'",
                [],
            )
            .unwrap();
        assert!(guard.validate(&ids(&["k1", "k3"])).is_err());
    }

    #[test]
    fn local_binding_updates_scope_without_touching_provider_path() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("binding.db")).unwrap();
        seed(&db);
        let guard = OrganizationScopeGuard::new(&db);
        assert!(guard.validate(&ids(&["k1", "k2"])).is_err());
        db.conn()
            .execute(
                "INSERT INTO project_session_binding(session_id,project_path,updated_at)
                 VALUES (2,'C:/source/project','now')",
                [],
            )
            .unwrap();
        assert!(guard.validate(&ids(&["k1", "k2"])).is_ok());
        let original: String = db
            .conn()
            .query_row(
                "SELECT project_path FROM source_session WHERE id = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(original, "D:/other/project");
        db.conn()
            .execute("UPDATE source_session SET is_missing = 1 WHERE id = 2", [])
            .unwrap();
        assert!(guard.validate(&ids(&["k1", "k2"])).is_err());
    }

    #[test]
    fn unresolved_and_manual_knowledge_cannot_be_implicitly_cross_merged() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("unresolved.db")).unwrap();
        seed(&db);
        db.conn()
            .execute(
                "UPDATE source_session SET project_path = NULL WHERE id = 3",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO knowledge_item
                 (id,project_name,title,category,summary,content,
                  source_type,managed_by,status,created_at,updated_at)
                 VALUES ('manual','Same Display','Manual','decision','summary','body',
                         'manual','user','active','now','now')",
                [],
            )
            .unwrap();
        let guard = OrganizationScopeGuard::new(&db);
        assert!(guard.validate(&ids(&["manual"])).is_ok());
        assert!(guard.validate(&ids(&["k1", "manual"])).is_err());
        assert!(guard.validate(&ids(&["manual", "k1"])).is_err());
        assert!(guard.validate(&ids(&["k1", "k3"])).is_err());
        assert!(guard.validate(&ids(&["k3"])).is_ok());
    }
}
