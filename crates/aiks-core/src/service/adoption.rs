//! Explicit offline adoption; the caller stops legacy engines and creates a
//! consistent DB + content-workspace backup first. No network or filesystem scan.
use super::{
    ingestion::accept_in_tx, validate_submission, validation::validate_identifier, LocalContext,
    ServiceError, SnapshotReceipt, SnapshotSubmission, ValidatedSnapshot,
};
use crate::{model::SourceKind, storage::StateDb};
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Clone)]
pub struct SessionAdoption {
    pub session_id: i64,
    pub source: SourceKind,
    pub upstream_id: String,
    pub registration_id: String,
    pub snapshot: Option<SnapshotSubmission>,
}
#[derive(Clone)]
pub struct AdoptionManifest {
    pub service_instance_id: String,
    pub sessions: Vec<SessionAdoption>,
    pub knowledge_ids: Vec<String>,
}
#[derive(Debug, Default, Serialize)]
pub struct AdoptionReport {
    pub bound: Vec<i64>,
    pub snapshot_ready: Vec<i64>,
    pub source_unavailable: Vec<i64>,
    pub conflict: Vec<i64>,
    pub receipts: Vec<SnapshotReceipt>,
    pub knowledge_bound: Vec<String>,
    pub retained_legacy_jobs: Vec<String>,
    pub retired_legacy_jobs: Vec<String>,
}

pub fn adopt_local_state(
    db: &StateDb,
    context: &LocalContext,
    manifest: &AdoptionManifest,
) -> Result<AdoptionReport, ServiceError> {
    if !db.has_exclusive_lease() {
        return Err(ServiceError::Unavailable);
    }
    if manifest.service_instance_id != context.instance_id() {
        return Err(ServiceError::Unauthorized);
    }
    let checked = validate_manifest(context, manifest)?;
    let mut conn = db.conn();
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let owned: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM service_instance WHERE singleton=1
         AND instance_id=?1 AND principal_id=?2 AND personal_space_id=?3)",
        params![
            context.instance_id(),
            context.principal_id(),
            context.space_id()
        ],
        |r| r.get(0),
    )?;
    if !owned {
        return Err(ServiceError::Unauthorized);
    }
    let running: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM pipeline_job WHERE status='RUNNING')",
        [],
        |r| r.get(0),
    )?;
    if running {
        return Err(ServiceError::Conflict);
    }
    let mut report = AdoptionReport::default();
    for item in &manifest.sessions {
        if !identity_matches(&tx, context, item)? {
            report.conflict.push(item.session_id);
        }
    }
    // A conflicting manifest is all-or-nothing, even if earlier entries matched.
    if !report.conflict.is_empty() {
        return Ok(report);
    }
    for id in &manifest.knowledge_ids {
        verify_knowledge(&tx, context, id, manifest)?;
    }
    let now = Utc::now().to_rfc3339();
    for (item, snapshot) in manifest.sessions.iter().zip(&checked) {
        tx.execute(
            "INSERT INTO service_session_binding(session_id,principal_id,space_id,registration_id,upstream_id,current_revision)
             VALUES (?1,?2,?3,?4,?5,0) ON CONFLICT(session_id) DO NOTHING",
            params![item.session_id,context.principal_id(),context.space_id(),item.registration_id,item.upstream_id])?;
        report.bound.push(item.session_id);
        if let Some(snapshot) = snapshot {
            // The identical ingestion transaction primitive owns receipt CAS,
            // immutable input and durable jobs in both HTTP and adoption paths.
            let (receipt, _, _) = accept_in_tx(&tx, context, snapshot)?;
            report.receipts.push(receipt);
            report.snapshot_ready.push(item.session_id);
        } else {
            let revision: u32 = tx.query_row(
                "SELECT current_revision FROM service_session_binding WHERE session_id=?1",
                [item.session_id],
                |r| r.get(0),
            )?;
            if revision == 0 {
                report.source_unavailable.push(item.session_id);
            } else {
                report.snapshot_ready.push(item.session_id);
            }
        }
        record_legacy_jobs(&tx, item.session_id, &mut report)?;
    }
    for id in &manifest.knowledge_ids {
        // Standalone documents have no source_session_id. Never make this
        // binding an alternate route around a foreign session's ownership.
        tx.execute(
            "INSERT INTO service_knowledge_binding(knowledge_id,principal_id,space_id,created_at)
             SELECT id,?2,?3,?4 FROM knowledge_item WHERE id=?1 AND source_session_id IS NULL
             ON CONFLICT(knowledge_id) DO NOTHING",
            params![id, context.principal_id(), context.space_id(), now],
        )?;
        report.knowledge_bound.push(id.clone());
    }
    tx.commit()?;
    Ok(report)
}

fn validate_manifest(
    ctx: &LocalContext,
    manifest: &AdoptionManifest,
) -> Result<Vec<Option<ValidatedSnapshot>>, ServiceError> {
    if manifest.sessions.len() > 256 || manifest.knowledge_ids.len() > 1024 {
        return Err(ServiceError::TooLarge);
    }
    let mut ids = HashSet::new();
    let mut total = 0_usize;
    let mut checked = Vec::with_capacity(manifest.sessions.len());
    for item in &manifest.sessions {
        if item.session_id <= 0 || !ids.insert(item.session_id) {
            return Err(ServiceError::InvalidInput);
        }
        validate_identifier(&item.upstream_id)?;
        validate_identifier(&item.registration_id)?;
        let snapshot = match &item.snapshot {
            Some(input) => {
                if input.service_instance_id != ctx.instance_id()
                    || input.space_id != ctx.space_id()
                    || input.source_registration_id != item.registration_id
                    || input.session.source != item.source
                    || input.session.external_session_id != item.upstream_id
                    || input.expected_revision != 0
                {
                    return Err(ServiceError::InvalidInput);
                }
                let checked = validate_submission(input)?;
                total = total
                    .checked_add(checked.canonical_json.len())
                    .ok_or(ServiceError::TooLarge)?;
                if total > 32 * 1024 * 1024 {
                    return Err(ServiceError::TooLarge);
                }
                Some(checked)
            }
            None => None,
        };
        checked.push(snapshot);
    }
    let mut knowledge_ids = HashSet::new();
    for id in &manifest.knowledge_ids {
        validate_identifier(id)?;
        if !knowledge_ids.insert(id) {
            return Err(ServiceError::InvalidInput);
        }
    }
    Ok(checked)
}

fn identity_matches(
    tx: &Transaction<'_>,
    ctx: &LocalContext,
    item: &SessionAdoption,
) -> Result<bool, ServiceError> {
    let registered: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM service_source_registration WHERE id=?1
         AND principal_id=?2 AND space_id=?3 AND source=?4)",
        params![
            item.registration_id,
            ctx.principal_id(),
            ctx.space_id(),
            item.source.as_str()
        ],
        |r| r.get(0),
    )?;
    if !registered {
        return Err(ServiceError::NotFound);
    }
    let row: Option<(String, String)> = tx
        .query_row(
            "SELECT source,external_session_id FROM source_session WHERE id=?1",
            [item.session_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((source, upstream)) = row else {
        return Err(ServiceError::NotFound);
    };
    if source != item.source.as_str() || upstream != item.upstream_id {
        return Ok(false);
    }
    let conflict:bool=tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM service_session_binding
         WHERE (session_id=?1 AND (principal_id<>?2 OR space_id<>?3 OR registration_id<>?4 OR upstream_id<>?5))
            OR (space_id=?3 AND registration_id=?4 AND upstream_id=?5 AND session_id<>?1))",
        params![item.session_id,ctx.principal_id(),ctx.space_id(),item.registration_id,item.upstream_id],|r|r.get(0))?;
    Ok(!conflict)
}

fn verify_knowledge(
    tx: &Transaction<'_>,
    ctx: &LocalContext,
    id: &str,
    manifest: &AdoptionManifest,
) -> Result<(), ServiceError> {
    let row: Option<(Option<i64>, Option<String>, Option<String>)> = tx
        .query_row(
            "SELECT ki.source_session_id,kb.principal_id,kb.space_id FROM knowledge_item ki
         LEFT JOIN service_knowledge_binding kb ON kb.knowledge_id=ki.id WHERE ki.id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let (source, owner, space) = row.ok_or(ServiceError::NotFound)?;
    if owner.as_deref().is_some_and(|v| v != ctx.principal_id())
        || space.as_deref().is_some_and(|v| v != ctx.space_id())
    {
        return Err(ServiceError::NotFound);
    }
    if let Some(source) = source {
        if !manifest.sessions.iter().any(|s| s.session_id == source) {
            let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM service_session_binding WHERE session_id=?1 AND principal_id=?2 AND space_id=?3)",
                params![source,ctx.principal_id(),ctx.space_id()],|r|r.get(0))?;
            if !owned {
                return Err(ServiceError::Conflict);
            }
        }
    }
    Ok(())
}

fn record_legacy_jobs(
    tx: &Transaction<'_>,
    session: i64,
    report: &mut AdoptionReport,
) -> Result<(), ServiceError> {
    let mut stmt = tx.prepare(
        "SELECT j.id,j.status FROM pipeline_job j WHERE j.session_id=?1
         AND NOT EXISTS(SELECT 1 FROM service_job_input i WHERE i.durable_job_id=j.id)
         AND j.status IN ('PENDING','FAILED','SUPERSEDED') ORDER BY j.id LIMIT 1001",
    )?;
    let rows = stmt.query_map([session], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (id, status) = row?;
        if report.retained_legacy_jobs.len() + report.retired_legacy_jobs.len() >= 1000 {
            return Err(ServiceError::TooLarge);
        }
        if status == "SUPERSEDED" {
            report.retired_legacy_jobs.push(id);
        } else {
            report.retained_legacy_jobs.push(id);
        }
    }
    Ok(())
}
