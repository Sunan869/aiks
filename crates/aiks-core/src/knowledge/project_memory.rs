//! Read-only, evidence-linked project memory derived from canonical local data.
//! Sessions with the same explicit directory can be grouped across providers.
//! A missing path is *never* merged merely because project display names agree.
use crate::storage::StateDb;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectOverview {
    pub id: String,
    pub title: String,
    pub verified_path: bool,
    pub session_count: usize,
    pub knowledge_count: usize,
    pub sources: Vec<String>,
    pub last_updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMemoryEntry {
    pub knowledge_id: String,
    pub session_id: i64,
    pub source: String,
    pub session_external_id: String,
    pub title: String,
    pub category: String,
    pub summary: String,
    pub updated_at: String,
    pub feedback_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMemorySnapshot {
    pub project: ProjectOverview,
    pub entries: Vec<ProjectMemoryEntry>,
    pub truncated: bool,
}

/// A stable opaque ID; raw source paths never need to cross the UI boundary.
fn project_identity(
    source: &str,
    external_id: &str,
    path: Option<&str>,
    name: Option<&str>,
) -> (String, String, bool) {
    let path = path.unwrap_or("").trim().replace('\\', "/");
    let path = path.trim_end_matches('/');
    let (key, verified) = if !path.is_empty() {
        let windows = path.as_bytes().get(1) == Some(&b':') || path.starts_with("//");
        let canonical = if windows {
            path.to_lowercase()
        } else {
            path.to_string()
        };
        (format!("path:{canonical}"), true)
    } else {
        // Pathless sessions cannot be safely associated with each other.
        (format!("unresolved:{source}:{external_id}"), false)
    };
    let digest = hex::encode(Sha256::digest(key.as_bytes()));
    let title = name
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            path.rsplit('/')
                .next()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("{source} · {external_id}"));
    (format!("project:{}", &digest[..24]), title, verified)
}

pub struct ProjectMemoryService<'a> {
    db: &'a StateDb,
}

impl<'a> ProjectMemoryService<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    pub fn list(&self) -> Result<Vec<ProjectOverview>> {
        let conn = self.db.conn();
        let mut groups: BTreeMap<String, ProjectOverview> = BTreeMap::new();
        let mut session_to_project: HashMap<i64, String> = HashMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT id,source,external_session_id,project_path,project_name,updated_at
                 FROM source_session WHERE is_missing = 0 ORDER BY updated_at DESC, id DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?;
            for row in rows {
                let (sid, source, external, path, name, updated) = row?;
                let (id, title, verified_path) =
                    project_identity(&source, &external, path.as_deref(), name.as_deref());
                let group = groups.entry(id.clone()).or_insert_with(|| ProjectOverview {
                    id: id.clone(),
                    title,
                    verified_path,
                    session_count: 0,
                    knowledge_count: 0,
                    sources: Vec::new(),
                    last_updated_at: updated.clone(),
                });
                group.session_count += 1;
                if !group.sources.contains(&source) {
                    group.sources.push(source);
                }
                if updated > group.last_updated_at {
                    group.last_updated_at = updated;
                }
                session_to_project.insert(sid, id);
            }
        }
        {
            let mut stmt = conn.prepare(
                "SELECT source_session_id, COUNT(*) FROM knowledge_item
                 WHERE status = 'active' AND source_session_id IS NOT NULL
                 GROUP BY source_session_id",
            )?;
            let rows =
                stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;
            for row in rows {
                let (session_id, count) = row?;
                if let Some(group_id) = session_to_project.get(&session_id) {
                    if let Some(group) = groups.get_mut(group_id) {
                        group.knowledge_count += count as usize;
                    }
                }
            }
        }
        let mut results = groups.into_values().collect::<Vec<_>>();
        for project in &mut results {
            project.sources.sort();
        }
        results.sort_by(|a, b| {
            b.last_updated_at
                .cmp(&a.last_updated_at)
                .then(a.id.cmp(&b.id))
        });
        Ok(results)
    }

    pub fn get(&self, project_id: &str, limit: usize) -> Result<ProjectMemorySnapshot> {
        let project = self
            .list()?
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| anyhow::anyhow!("Project identity not found"))?;
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT ki.id,ki.title,ki.category,ki.summary,ki.updated_at,
                    ss.id,ss.source,ss.external_session_id,ss.project_path,ss.project_name,
                    (SELECT kind FROM knowledge_feedback WHERE knowledge_id = ki.id
                     ORDER BY created_at DESC, id DESC LIMIT 1)
             FROM knowledge_item ki JOIN source_session ss ON ss.id = ki.source_session_id
             WHERE ki.status = 'active' AND ss.is_missing = 0
             ORDER BY ki.updated_at DESC, ki.id DESC",
        )?;
        let mut entries = Vec::new();
        let mut truncated = false;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
            ))
        })?;
        for row in rows {
            let (
                knowledge_id,
                title,
                category,
                summary,
                updated,
                session_id,
                source,
                external,
                path,
                name,
                feedback_status,
            ) = row?;
            let (id, _, _) = project_identity(&source, &external, path.as_deref(), name.as_deref());
            if id != project_id {
                continue;
            }
            if entries.len() >= limit.min(200) {
                truncated = true;
                break;
            }
            entries.push(ProjectMemoryEntry {
                knowledge_id,
                session_id,
                source,
                session_external_id: external,
                title,
                category,
                summary: summary.chars().take(500).collect(),
                updated_at: updated,
                feedback_status,
            });
        }
        Ok(ProjectMemorySnapshot {
            project,
            entries,
            truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    #[test]
    fn project_identity_is_cross_provider_only_when_paths_match() {
        let (a, _, _) = project_identity("codex", "1", Some("C:\\Repo\\App"), Some("App"));
        let (b, _, _) = project_identity("claude", "2", Some("c:/repo/app/"), Some("App"));
        assert_eq!(a, b);
        let (other, _, _) = project_identity("codex", "3", Some("D:\\Repo\\App"), Some("App"));
        assert_ne!(a, other);
        let (unknown1, _, _) = project_identity("codex", "4", None, Some("App"));
        let (unknown2, _, _) = project_identity("claude", "5", None, Some("App"));
        assert_ne!(unknown1, unknown2);
    }

    #[test]
    fn project_memory_aggregates_real_session_identity_not_just_name() {
        let tmp = tempfile::tempdir().unwrap();
        let db = StateDb::open(&tmp.path().join("memory.db")).unwrap();
        for (i, source, path) in [
            (1, "codex", "C:/code/a"),
            (2, "claude", "c:/code/a"),
            (3, "opencode", "D:/code/a"),
        ] {
            db.conn().execute(
                "INSERT INTO source_session
                 (id,source,external_session_id,project_path,project_name,last_seen_at,created_at,updated_at)
                 VALUES (?1,?2,?3,?4,'a','2026-01-01','2026-01-01','2026-01-01')",
                params![i, source, i.to_string(), path],
            ).unwrap();
        }
        let service = ProjectMemoryService::new(&db);
        let projects = service.list().unwrap();
        assert_eq!(projects.len(), 2);
        assert!(projects
            .iter()
            .any(|p| p.session_count == 2 && p.sources.len() == 2));
        assert!(projects.iter().any(|p| p.session_count == 1));
        assert!(service.get(&projects[0].id, 50).unwrap().entries.is_empty());
    }
}
