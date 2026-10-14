//! S3.2 project report drafts: local-only, explicit, versioned user edits.
//! Preview generation is read-only; saving a revision never sends content to SiYuan.
use crate::knowledge::project_memory::ProjectMemoryService;
use crate::storage::StateDb;
use anyhow::{bail, Result};
use chrono::NaiveDate;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectReportDraft {
    pub id: String,
    pub project_id: String,
    pub from_date: String,
    pub through_date: String,
    pub source_filter: Option<String>,
    pub current_revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectReportVersion {
    pub draft: ProjectReportDraft,
    pub revision: i64,
    pub body: String,
    pub saved_at: String,
}

pub struct ProjectReportDraftInput<'a> {
    pub draft_id: Option<&'a str>,
    pub project_id: &'a str,
    pub from: &'a str,
    pub through: &'a str,
    pub source: Option<&'a str>,
    pub expected_revision: i64,
    pub body: &'a str,
}

pub struct ProjectReportDraftService<'a> {
    db: &'a StateDb,
}

impl<'a> ProjectReportDraftService<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    /// A new save uses (None,0); an edit requires the exact latest revision.
    /// The old revision remains immutable, including when stale clients retry.
    pub fn save(&self, input: ProjectReportDraftInput<'_>) -> Result<ProjectReportVersion> {
        let ProjectReportDraftInput {
            draft_id,
            project_id,
            from,
            through,
            source,
            expected_revision,
            body,
        } = input;
        let from_date = NaiveDate::parse_from_str(from, "%Y-%m-%d")?;
        let through_date = NaiveDate::parse_from_str(through, "%Y-%m-%d")?;
        anyhow::ensure!(
            from_date <= through_date,
            "Invalid project report date range"
        );
        anyhow::ensure!(
            !body.trim().is_empty() && body.len() <= 250_000,
            "Reviewed report must contain 1 to 250000 bytes"
        );
        anyhow::ensure!(
            expected_revision >= 0,
            "Expected revision must not be negative"
        );
        let project = ProjectMemoryService::new(self.db)
            .list()?
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| anyhow::anyhow!("Source project no longer exists"))?;
        if let Some(source) = source {
            anyhow::ensure!(
                project.sources.iter().any(|value| value == source),
                "Report source is outside this project"
            );
        }
        let now = chrono::Utc::now().to_rfc3339();
        let id = match draft_id {
            Some(id) => {
                anyhow::ensure!(
                    !id.trim().is_empty() && id.len() <= 128,
                    "Invalid report draft identity"
                );
                id.to_owned()
            }
            None => uuid::Uuid::new_v4().to_string(),
        };
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let revision = if draft_id.is_some() {
            let previous: Option<(String, String, String, Option<String>, i64)> = tx
                .query_row(
                    "SELECT project_id,from_date,through_date,source_filter,current_revision
                     FROM project_report_draft WHERE id = ?1",
                    params![id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .optional()?;
            let Some((saved_project, saved_from, saved_through, saved_source, current)) = previous
            else {
                bail!("Report draft not found");
            };
            anyhow::ensure!(
                saved_project == project_id
                    && saved_from == from
                    && saved_through == through
                    && saved_source.as_deref() == source,
                "Cannot silently change a saved report scope"
            );
            anyhow::ensure!(
                current == expected_revision,
                "Report draft changed; reload before saving another revision"
            );
            let next = current + 1;
            let changed = tx.execute(
                "UPDATE project_report_draft
                 SET current_revision = ?1,updated_at = ?2
                 WHERE id = ?3 AND current_revision = ?4",
                params![next, now, id, current],
            )?;
            anyhow::ensure!(changed == 1, "Concurrent report edit detected");
            next
        } else {
            anyhow::ensure!(
                expected_revision == 0,
                "New report drafts must begin with revision zero"
            );
            tx.execute(
                "INSERT INTO project_report_draft
                 (id,project_id,from_date,through_date,source_filter,
                  current_revision,created_at,updated_at)
                 VALUES (?1,?2,?3,?4,?5,1,?6,?6)",
                params![id, project_id, from, through, source, now],
            )?;
            1
        };
        tx.execute(
            "INSERT INTO project_report_revision(draft_id,revision,body,saved_at)
             VALUES (?1,?2,?3,?4)",
            params![id, revision, body, now],
        )?;
        tx.commit()?;
        drop(conn);
        self.get(&id, Some(revision))?
            .ok_or_else(|| anyhow::anyhow!("Saved report revision not found"))
    }

    pub fn list(&self, project_id: &str) -> Result<Vec<ProjectReportDraft>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id,project_id,from_date,through_date,source_filter,
                    current_revision,created_at,updated_at
             FROM project_report_draft WHERE project_id = ?1
             ORDER BY updated_at DESC,id DESC LIMIT 100",
        )?;
        let rows = stmt.query_map(params![project_id], read_draft)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get(
        &self,
        draft_id: &str,
        revision: Option<i64>,
    ) -> Result<Option<ProjectReportVersion>> {
        let conn = self.db.conn();
        let draft = conn
            .query_row(
                "SELECT id,project_id,from_date,through_date,source_filter,
                        current_revision,created_at,updated_at
                 FROM project_report_draft WHERE id = ?1",
                params![draft_id],
                read_draft,
            )
            .optional()?;
        let Some(draft) = draft else {
            return Ok(None);
        };
        let revision = revision.unwrap_or(draft.current_revision);
        anyhow::ensure!(
            revision >= 1 && revision <= draft.current_revision,
            "Invalid report revision"
        );
        let saved = conn
            .query_row(
                "SELECT body,saved_at FROM project_report_revision
                 WHERE draft_id = ?1 AND revision = ?2",
                params![draft_id, revision],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("Report revision was not found"))?;
        Ok(Some(ProjectReportVersion {
            draft,
            revision,
            body: saved.0,
            saved_at: saved.1,
        }))
    }
}

fn read_draft(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectReportDraft> {
    Ok(ProjectReportDraft {
        id: row.get(0)?,
        project_id: row.get(1)?,
        from_date: row.get(2)?,
        through_date: row.get(3)?,
        source_filter: row.get(4)?,
        current_revision: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn setup() -> (tempfile::TempDir, StateDb, String) {
        let temp = tempfile::tempdir().unwrap();
        let db = StateDb::open(&temp.path().join("reports.db")).unwrap();
        for (id, source, path) in [
            (1, "codex", "C:/repo/app"),
            (2, "claude_code", "c:/repo/app/"),
            (3, "codex", "D:/repo/other"),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO source_session
                     (id,source,external_session_id,project_path,
                      last_seen_at,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,'now','now','now')",
                    params![id, source, format!("s{id}"), path],
                )
                .unwrap();
        }
        let project = ProjectMemoryService::new(&db)
            .list()
            .unwrap()
            .into_iter()
            .find(|item| item.sources.contains(&"claude_code".to_owned()))
            .unwrap()
            .id;
        (temp, db, project)
    }

    #[test]
    fn saved_edits_survive_restart_without_replacing_old_revision() {
        let (temp, db, project) = setup();
        let repo = ProjectReportDraftService::new(&db);
        let first = repo
            .save(ProjectReportDraftInput {
                draft_id: None,
                project_id: &project,
                from: "2026-10-04",
                through: "2026-10-11",
                source: None,
                expected_revision: 0,
                body: "Reviewed version 1",
            })
            .unwrap();
        let id = first.draft.id.clone();
        assert_eq!(first.revision, 1);
        let next = repo
            .save(ProjectReportDraftInput {
                draft_id: Some(&id),
                project_id: &project,
                from: "2026-10-04",
                through: "2026-10-11",
                source: None,
                expected_revision: 1,
                body: "User-edited version 2",
            })
            .unwrap();
        assert_eq!(next.revision, 2);
        assert_eq!(
            repo.get(&id, Some(1)).unwrap().unwrap().body,
            "Reviewed version 1"
        );
        assert!(repo
            .save(ProjectReportDraftInput {
                draft_id: Some(&id),
                project_id: &project,
                from: "2026-10-04",
                through: "2026-10-11",
                source: None,
                expected_revision: 1,
                body: "Stale edit",
            })
            .is_err());
        assert_eq!(repo.list(&project).unwrap().len(), 1);
        drop(db);

        let db = StateDb::open(&temp.path().join("reports.db")).unwrap();
        let reopened = ProjectReportDraftService::new(&db);
        assert_eq!(
            reopened.get(&id, None).unwrap().unwrap().body,
            "User-edited version 2"
        );
        assert_eq!(
            reopened.get(&id, Some(1)).unwrap().unwrap().body,
            "Reviewed version 1"
        );
        assert_eq!(reopened.list(&project).unwrap()[0].current_revision, 2);
    }

    #[test]
    fn new_reports_cannot_cross_project_boundaries_or_rewrite_scope() {
        let (_temp, db, project) = setup();
        let repo = ProjectReportDraftService::new(&db);
        assert!(repo
            .save(ProjectReportDraftInput {
                draft_id: None,
                project_id: &project,
                from: "2026-10-12",
                through: "2026-10-11",
                source: None,
                expected_revision: 0,
                body: "Text",
            })
            .is_err());
        assert!(repo
            .save(ProjectReportDraftInput {
                draft_id: None,
                project_id: &project,
                from: "2026-10-11",
                through: "2026-10-11",
                source: Some("unknown"),
                expected_revision: 0,
                body: "Text",
            })
            .is_err());
        assert!(repo
            .save(ProjectReportDraftInput {
                draft_id: None,
                project_id: "other-project",
                from: "2026-10-11",
                through: "2026-10-11",
                source: None,
                expected_revision: 0,
                body: "Text",
            })
            .is_err());
        assert!(repo
            .save(ProjectReportDraftInput {
                draft_id: None,
                project_id: &project,
                from: "2026-10-11",
                through: "2026-10-11",
                source: None,
                expected_revision: 0,
                body: "",
            })
            .is_err());
        let first = repo
            .save(ProjectReportDraftInput {
                draft_id: None,
                project_id: &project,
                from: "2026-10-11",
                through: "2026-10-11",
                source: Some("codex"),
                expected_revision: 0,
                body: "Safe",
            })
            .unwrap();
        assert!(repo
            .save(ProjectReportDraftInput {
                draft_id: Some(&first.draft.id),
                project_id: &project,
                from: "2026-10-11",
                through: "2026-10-11",
                source: Some("claude_code"),
                expected_revision: 1,
                body: "Wrong scope",
            })
            .is_err());
        assert_eq!(repo.list(&project).unwrap()[0].current_revision, 1);
    }
}
