//! Receipt, immutable snapshot and durable job commit in one transaction.

use chrono::Utc;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

use crate::model::SourceKind;
use crate::pipeline::job_repo::PipelineJobRepo;
use crate::pipeline::PipelineJob;

use super::validation::validate_identifier;
use super::{
    validate_submission, LocalContext, ServiceError, ServiceStore, SnapshotReceipt,
    ValidatedSnapshot,
};

impl From<rusqlite::Error> for ServiceError {
    fn from(error: rusqlite::Error) -> Self {
        match error {
            rusqlite::Error::SqliteFailure(code, _)
                if matches!(
                    code.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                ) =>
            {
                Self::Unavailable
            }
            _ => Self::Internal,
        }
    }
}

impl ServiceStore {
    pub fn register_source(
        &self,
        context: &LocalContext,
        source: SourceKind,
        registration_key: &str,
    ) -> Result<String, ServiceError> {
        if *context != self.local_context() {
            return Err(ServiceError::Unauthorized);
        }
        validate_identifier(registration_key)?;
        let mut conn = self.db().conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let candidate = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO service_source_registration
             (id, principal_id, space_id, source, registration_key)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(principal_id, space_id, source, registration_key) DO NOTHING",
            params![
                candidate,
                context.principal_id(),
                context.space_id(),
                source.as_str(),
                registration_key
            ],
        )?;
        let id = tx.query_row(
            "SELECT id FROM service_source_registration
             WHERE principal_id=?1 AND space_id=?2 AND source=?3 AND registration_key=?4",
            params![
                context.principal_id(),
                context.space_id(),
                source.as_str(),
                registration_key
            ],
            |row| row.get(0),
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// The boolean indicates newly queued work, not extraction success.
    /// Retried receipt lookup precedes revision CAS so a lost response remains
    /// recoverable after subsequent revisions have already been accepted.
    pub fn accept(
        &self,
        context: &LocalContext,
        snapshot: &ValidatedSnapshot,
    ) -> Result<(SnapshotReceipt, bool), ServiceError> {
        if *context != self.local_context() {
            return Err(ServiceError::Unauthorized);
        }
        let input = &snapshot.submission;
        if input.service_instance_id != context.instance_id()
            || input.space_id != context.space_id()
        {
            return Err(ServiceError::NotFound);
        }
        // A public Rust caller can mutate ValidatedSnapshot. Do not persist
        // bytes or digests that no longer describe its validated submission.
        let checked = validate_submission(input)?;
        if checked.canonical_json != snapshot.canonical_json
            || checked.request_hash != snapshot.request_hash
            || checked.content_hash != snapshot.content_hash
        {
            return Err(ServiceError::InvalidInput);
        }

        let mut conn = self.db().conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let registered: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_source_registration
             WHERE id=?1 AND principal_id=?2 AND space_id=?3 AND source=?4)",
            params![
                input.source_registration_id,
                context.principal_id(),
                context.space_id(),
                input.session.source.as_str()
            ],
            |row| row.get(0),
        )?;
        if !registered {
            return Err(ServiceError::NotFound);
        }
        if let Some((stored_hash, receipt)) = find_receipt(&tx, context, snapshot)? {
            if stored_hash != snapshot.request_hash {
                return Err(ServiceError::Conflict);
            }
            tx.commit()?;
            return Ok((receipt, false));
        }

        let binding: Option<(i64, u32)> = tx
            .query_row(
                "SELECT session_id, current_revision FROM service_session_binding
                 WHERE principal_id=?1 AND space_id=?2 AND registration_id=?3 AND upstream_id=?4",
                params![
                    context.principal_id(),
                    context.space_id(),
                    input.source_registration_id,
                    input.session.external_session_id
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let current_revision = binding.map(|(_, revision)| revision).unwrap_or(0);
        if input.expected_revision != current_revision {
            return Err(ServiceError::Conflict);
        }
        let now = Utc::now().to_rfc3339();
        if let Some((session_id, revision)) = binding {
            if let Some(receipt) = reuse_current(&tx, session_id, revision, snapshot)? {
                save_receipt(&tx, context, snapshot, &receipt, &now)?;
                tx.commit()?;
                return Ok((receipt, false));
            }
        }
        let revision = current_revision
            .checked_add(1)
            .ok_or(ServiceError::RevisionExhausted)?;
        let session_id = match binding {
            Some((session_id, _)) => session_id,
            None => create_binding(&tx, context, snapshot, &now)?,
        };
        tx.execute(
            "UPDATE source_session SET title=?2, project_name=?3, source_updated_at=?4,
             content_hash=?5, parser_version=?6, last_seen_at=?7, updated_at=?7, is_missing=0
             WHERE id=?1",
            params![
                session_id,
                input.session.title,
                input.session.project_name,
                input.session.updated_at.map(|value| value.to_rfc3339()),
                snapshot.content_hash,
                input.parser_version,
                now
            ],
        )?;
        let advanced = tx.execute(
            "UPDATE service_session_binding SET current_revision=?2
             WHERE session_id=?1 AND current_revision=?3",
            params![session_id, revision, current_revision],
        )?;
        if advanced != 1 {
            return Err(ServiceError::Conflict);
        }
        let snapshot_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO service_session_snapshot
             (id, session_id, revision, parser_version, content_hash, canonical_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                snapshot_id,
                session_id,
                revision,
                input.parser_version,
                snapshot.content_hash,
                snapshot.canonical_json,
                now
            ],
        )?;
        // Legacy upsert reuses one run per pipeline version. Snapshot input
        // requires an immutable run per revision instead; do not rebind it.
        let pipeline_run_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO pipeline_run
             (id, session_id, status, pipeline_version, source_hash, created_at, updated_at)
             VALUES (?1, ?2, 'DISCOVERED', ?3, ?4, ?5, ?5)",
            params![
                pipeline_run_id,
                session_id,
                format!("service-v1/r{revision}"),
                snapshot.content_hash,
                now
            ],
        )?;
        let job = PipelineJob {
            pipeline_run_id: pipeline_run_id.clone(),
            session_id,
            session_external_id: input.session.external_session_id.clone(),
            source: input.session.source.as_str().to_owned(),
            session_title: input.session.title.clone(),
            project_name: input.session.project_name.clone(),
        };
        let queued = PipelineJobRepo::enqueue_snapshot_in_tx(&tx, &job)
            .map_err(|_| ServiceError::Internal)?;
        tx.execute(
            "INSERT INTO service_job_input (pipeline_run_id, snapshot_id, durable_job_id)
             VALUES (?1, ?2, ?3)",
            params![pipeline_run_id, snapshot_id, queued.durable_job_id],
        )?;
        let receipt = SnapshotReceipt {
            receipt_id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            snapshot_id,
            revision,
            job_id: queued.durable_job_id,
            pipeline_run_id,
            state: "accepted".to_owned(),
        };
        save_receipt(&tx, context, snapshot, &receipt, &now)?;
        tx.commit()?;
        Ok((receipt, queued.inserted))
    }
}

fn find_receipt(
    tx: &Transaction<'_>,
    context: &LocalContext,
    snapshot: &ValidatedSnapshot,
) -> Result<Option<(String, SnapshotReceipt)>, ServiceError> {
    let input = &snapshot.submission;
    Ok(tx
        .query_row(
            "SELECT r.request_hash, r.id, s.session_id, s.id, s.revision,
                    r.durable_job_id, r.pipeline_run_id
             FROM service_ingest_receipt r
             JOIN service_session_snapshot s ON s.id=r.snapshot_id
             WHERE r.principal_id=?1 AND r.space_id=?2 AND r.registration_id=?3
               AND r.upstream_id=?4 AND r.submission_id=?5",
            params![
                context.principal_id(),
                context.space_id(),
                input.source_registration_id,
                input.session.external_session_id,
                input.submission_id
            ],
            |row| {
                Ok((
                    row.get(0)?,
                    SnapshotReceipt {
                        receipt_id: row.get(1)?,
                        session_id: row.get::<_, i64>(2)?.to_string(),
                        snapshot_id: row.get(3)?,
                        revision: row.get(4)?,
                        job_id: row.get(5)?,
                        pipeline_run_id: row.get(6)?,
                        state: "accepted".to_owned(),
                    },
                ))
            },
        )
        .optional()?)
}

fn reuse_current(
    tx: &Transaction<'_>,
    session_id: i64,
    revision: u32,
    snapshot: &ValidatedSnapshot,
) -> Result<Option<SnapshotReceipt>, ServiceError> {
    Ok(tx
        .query_row(
            "SELECT s.id, i.durable_job_id, i.pipeline_run_id
             FROM service_session_snapshot s
             JOIN service_job_input i ON i.snapshot_id=s.id
             WHERE s.session_id=?1 AND s.revision=?2 AND s.content_hash=?3",
            params![session_id, revision, snapshot.content_hash],
            |row| {
                Ok(SnapshotReceipt {
                    receipt_id: Uuid::new_v4().to_string(),
                    session_id: session_id.to_string(),
                    snapshot_id: row.get(0)?,
                    revision,
                    job_id: row.get(1)?,
                    pipeline_run_id: row.get(2)?,
                    state: "accepted".to_owned(),
                })
            },
        )
        .optional()?)
}

fn create_binding(
    tx: &Transaction<'_>,
    context: &LocalContext,
    snapshot: &ValidatedSnapshot,
    now: &str,
) -> Result<i64, ServiceError> {
    let input = &snapshot.submission;
    // Preserve the existing source key, but do not collide with another
    // device's upstream ID or silently adopt an unrelated legacy session.
    let internal_external_id = format!("svc:{}", Uuid::new_v4());
    tx.execute(
        "INSERT INTO source_session
         (source, external_session_id, last_seen_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?3, ?3)",
        params![input.session.source.as_str(), internal_external_id, now],
    )?;
    let session_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO service_session_binding
         (session_id, principal_id, space_id, registration_id, upstream_id, current_revision)
         VALUES (?1, ?2, ?3, ?4, ?5, 0)",
        params![
            session_id,
            context.principal_id(),
            context.space_id(),
            input.source_registration_id,
            input.session.external_session_id
        ],
    )?;
    Ok(session_id)
}

fn save_receipt(
    tx: &Transaction<'_>,
    context: &LocalContext,
    snapshot: &ValidatedSnapshot,
    receipt: &SnapshotReceipt,
    now: &str,
) -> Result<(), ServiceError> {
    let input = &snapshot.submission;
    tx.execute(
        "INSERT INTO service_ingest_receipt
         (id, principal_id, space_id, registration_id, upstream_id, submission_id,
          request_hash, snapshot_id, pipeline_run_id, durable_job_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            receipt.receipt_id,
            context.principal_id(),
            context.space_id(),
            input.source_registration_id,
            input.session.external_session_id,
            input.submission_id,
            snapshot.request_hash,
            receipt.snapshot_id,
            receipt.pipeline_run_id,
            receipt.job_id,
            now
        ],
    )?;
    Ok(())
}
