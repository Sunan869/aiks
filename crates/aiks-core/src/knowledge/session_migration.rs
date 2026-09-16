use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::sink::SiYuanSink;
use crate::storage::StateDb;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionContentMigrationStats {
    pub total: usize,
    pub moved: usize,
    pub reused: usize,
    pub missing: usize,
    pub failed: usize,
}

#[derive(Debug, Clone)]
struct SessionTarget {
    session_id: i64,
    doc_id: String,
    target_path: Option<String>,
}

/// Move every known raw Session document into the same canonical notebook used
/// by Knowledge while preserving the existing SiYuan document/block ID.
///
/// The SQLite connection is only held while loading/updating mappings. No DB
/// guard crosses an await boundary.
pub async fn migrate_sessions_to_content_notebook(
    db: &StateDb,
    sink: &SiYuanSink,
) -> anyhow::Result<SessionContentMigrationStats> {
    let target_notebook = sink.ensure_notebook().await?;
    let targets = load_targets(db)?;
    let mut stats = SessionContentMigrationStats {
        total: targets.len(),
        ..Default::default()
    };

    for target in targets {
        match sink.get_doc_notebook(&target.doc_id).await {
            Ok(Some(current_notebook)) => {
                // The mapping is useful even if a later move fails, because the
                // SiYuan document ID remains stable across notebooks.
                bind_session_doc(db, target.session_id, &target.doc_id)?;

                if current_notebook == target_notebook {
                    stats.reused += 1;
                    continue;
                }

                let parent = current_parent_path(sink, &target.doc_id)
                    .await
                    .unwrap_or_else(|| parent_path(target.target_path.as_deref()));
                match sink
                    .move_docs(&[target.doc_id.clone()], &target_notebook, &parent)
                    .await
                {
                    Ok(()) => stats.moved += 1,
                    Err(error) => {
                        stats.failed += 1;
                        tracing::warn!(
                            session_id = target.session_id,
                            doc_id = %target.doc_id,
                            error = %error,
                            "[MIGRATION] failed to move raw Session into canonical notebook"
                        );
                    }
                }
            }
            Ok(None) => {
                stats.missing += 1;
                tracing::warn!(
                    session_id = target.session_id,
                    doc_id = %target.doc_id,
                    "[MIGRATION] mapped raw Session document no longer exists in SiYuan"
                );
            }
            Err(error) => {
                stats.failed += 1;
                tracing::warn!(
                    session_id = target.session_id,
                    doc_id = %target.doc_id,
                    error = %error,
                    "[MIGRATION] could not inspect raw Session notebook"
                );
            }
        }
    }

    Ok(stats)
}

fn load_targets(db: &StateDb) -> anyhow::Result<Vec<SessionTarget>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT ss.id, st.target_id, st.target_path
         FROM source_session ss
         JOIN sync_target st ON st.session_id = ss.id
         WHERE st.sink = 'siyuan'
           AND st.target_id IS NOT NULL
           AND st.target_id <> ''
         ORDER BY ss.id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SessionTarget {
                session_id: row.get(0)?,
                doc_id: row.get(1)?,
                target_path: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn bind_session_doc(db: &StateDb, session_id: i64, doc_id: &str) -> anyhow::Result<()> {
    db.conn().execute(
        "UPDATE source_session SET siyuan_doc_id = ?2 WHERE id = ?1",
        params![session_id, doc_id],
    )?;
    Ok(())
}

async fn current_parent_path(sink: &SiYuanSink, doc_id: &str) -> Option<String> {
    let escaped = doc_id.replace('\'', "''");
    let rows = sink
        .query_sql(&format!(
            "SELECT hpath FROM blocks WHERE id = '{escaped}' AND type = 'd' LIMIT 1"
        ))
        .await
        .ok()?;
    let hpath = rows
        .first()?
        .get("hpath")?
        .as_str()
        .filter(|value| !value.trim().is_empty())?;
    Some(parent_path(Some(hpath)))
}

fn parent_path(path: Option<&str>) -> String {
    let path = path.unwrap_or("/10 AI Sessions").trim();
    if path.is_empty() || path == "/" {
        return "/10 AI Sessions".to_string();
    }
    let normalized = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    match normalized.rfind('/') {
        Some(0) | None => "/10 AI Sessions".to_string(),
        Some(index) => normalized[..index].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::parent_path;

    #[test]
    fn preserves_session_hierarchy_when_deriving_move_parent() {
        assert_eq!(
            parent_path(Some(
                "/10 AI Sessions/OpenCode/2026/09/2026-09-16 Work [session]"
            )),
            "/10 AI Sessions/OpenCode/2026/09"
        );
        assert_eq!(
            parent_path(Some("10 AI Sessions/Codex/session")),
            "/10 AI Sessions/Codex"
        );
    }

    #[test]
    fn falls_back_to_session_root_for_missing_or_root_paths() {
        assert_eq!(parent_path(None), "/10 AI Sessions");
        assert_eq!(parent_path(Some("/")), "/10 AI Sessions");
        assert_eq!(parent_path(Some("/single")), "/10 AI Sessions");
    }
}
