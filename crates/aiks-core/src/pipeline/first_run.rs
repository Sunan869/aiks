//! S1.2: read-only eligibility for first AI work on a synced raw Session.
//! Historical failed retries are a different API, with a separate safe gate.
use crate::storage::StateDb;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};

pub struct InitialPipelineSession {
    pub source: String,
    pub external_id: String,
    pub title: Option<String>,
    pub project_name: Option<String>,
}

/// The legacy Tauri command may submit *only* eligible raw content. The
/// durable enqueue remains the authoritative deduplication mechanism.
pub fn load_initial_pipeline_session(
    db: &StateDb,
    session_id: i64,
) -> Result<InitialPipelineSession> {
    anyhow::ensure!(session_id > 0, "Invalid source Session ID");
    let conn = db.conn();
    let record = conn
        .query_row(
            "SELECT ss.source,ss.external_session_id,ss.title,ss.project_name
             FROM source_session ss
             JOIN sync_target st ON st.session_id = ss.id AND st.sink = 'siyuan'
             WHERE ss.id = ?1 AND ss.is_missing = 0
               AND st.status IN ('SYNCED','UNCHANGED')
               AND NOT EXISTS (
                   SELECT 1 FROM pipeline_job pj
                   WHERE pj.session_id = ss.id AND pj.status IN ('PENDING','RUNNING')
               )
               AND NOT EXISTS (
                   SELECT 1 FROM pipeline_run pr
                   WHERE pr.session_id = ss.id AND pr.pipeline_version = 'v3'
                     AND pr.status <> 'RAW_ONLY'
               )",
            params![session_id],
            |row| {
                Ok(InitialPipelineSession {
                    source: row.get(0)?,
                    external_id: row.get(1)?,
                    title: row.get(2)?,
                    project_name: row.get(3)?,
                })
            },
        )
        .optional()?;
    record.ok_or_else(|| anyhow::anyhow!(
        "First AI processing requires synced raw content, no SiYuan conflict, and no active, failed or completed pipeline. Retry failures through the safe retry command."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, StateDb) {
        let temp = tempfile::tempdir().unwrap();
        let db = StateDb::open(&temp.path().join("eligibility.db")).unwrap();
        db.conn()
            .execute(
                "INSERT INTO source_session
                 (id,source,external_session_id,project_name,
                  last_seen_at,created_at,updated_at)
                 VALUES (1,'codex','external-1','Safe','now','now','now')",
                [],
            )
            .unwrap();
        (temp, db)
    }

    #[test]
    fn only_synced_raw_source_without_prior_failed_ready_or_active_work_is_eligible() {
        let (_temp, db) = setup();
        assert!(load_initial_pipeline_session(&db, 1).is_err());
        assert!(load_initial_pipeline_session(&db, 0).is_err());
        db.conn()
            .execute(
                "INSERT INTO sync_target(session_id,sink,status)
                 VALUES (1,'siyuan','SYNCED')",
                [],
            )
            .unwrap();
        let accepted = load_initial_pipeline_session(&db, 1).unwrap();
        assert_eq!(accepted.source, "codex");
        assert_eq!(accepted.external_id, "external-1");
        db.conn()
            .execute(
                "UPDATE sync_target SET status = 'CONFLICT' WHERE session_id = 1",
                [],
            )
            .unwrap();
        assert!(load_initial_pipeline_session(&db, 1).is_err());
        db.conn()
            .execute(
                "UPDATE sync_target SET status = 'UNCHANGED' WHERE session_id = 1",
                [],
            )
            .unwrap();
        assert!(load_initial_pipeline_session(&db, 1).is_ok());
        db.conn()
            .execute(
                "INSERT INTO pipeline_run
                 (id,session_id,status,pipeline_version,created_at,updated_at)
                 VALUES ('run-1',1,'FAILED','v3','now','now')",
                [],
            )
            .unwrap();
        for status in ["FAILED", "PROCESSING", "READY", "DISCOVERED"] {
            db.conn()
                .execute(
                    "UPDATE pipeline_run SET status = ?1 WHERE id = 'run-1'",
                    params![status],
                )
                .unwrap();
            assert!(load_initial_pipeline_session(&db, 1).is_err());
        }
        db.conn()
            .execute(
                "UPDATE pipeline_run SET status = 'RAW_ONLY' WHERE id = 'run-1'",
                [],
            )
            .unwrap();
        assert!(load_initial_pipeline_session(&db, 1).is_ok());

        db.conn()
            .execute(
                "INSERT INTO pipeline_job
                 (id,source,external_session_id,session_id,pipeline_run_id,
                  status,created_at,updated_at)
                 VALUES ('job-1','codex','external-1',1,'run-1','PENDING','now','now')",
                [],
            )
            .unwrap();
        for status in ["PENDING", "RUNNING"] {
            db.conn()
                .execute(
                    "UPDATE pipeline_job SET status = ?1 WHERE id = 'job-1'",
                    params![status],
                )
                .unwrap();
            assert!(load_initial_pipeline_session(&db, 1).is_err());
        }
        db.conn()
            .execute(
                "UPDATE pipeline_job SET status = 'CANCELLED' WHERE id = 'job-1'",
                [],
            )
            .unwrap();
        assert!(load_initial_pipeline_session(&db, 1).is_ok());
        db.conn()
            .execute("UPDATE source_session SET is_missing = 1 WHERE id = 1", [])
            .unwrap();
        assert!(load_initial_pipeline_session(&db, 1).is_err());
    }

    #[test]
    fn session_with_other_sink_only_or_missing_identity_cannot_start() {
        let (_temp, db) = setup();
        db.conn()
            .execute(
                "INSERT INTO sync_target(session_id,sink,status)
                 VALUES (1,'mock','SYNCED')",
                [],
            )
            .unwrap();
        assert!(load_initial_pipeline_session(&db, 1).is_err());
        assert!(load_initial_pipeline_session(&db, 9999).is_err());
    }
}
