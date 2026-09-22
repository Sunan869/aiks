use super::{transport::validate_receipt, ClientError, ClientResult, PendingSubmission};
use aiks_core::{service::SnapshotReceipt, storage::ownership::BusinessDbLease};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
    time::Duration,
};
use uuid::Uuid;

const APP_ID: i64 = 0x41494b43;
const MAX_PENDING_BYTES: i64 = 256 * 1024 * 1024;
const MAX_PENDING_ROWS: i64 = 1024;
const LEASE_MS: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueOutcome {
    Queued(String),
    Existing(String),
    Unchanged,
}

/// A private lease capability, not an input accepted from a webview.
pub struct ClaimedUpload {
    id: String,
    lease: String,
    pending: PendingSubmission,
    attempt: u32,
}
impl ClaimedUpload {
    pub fn pending(&self) -> &PendingSubmission {
        &self.pending
    }
    pub fn id(&self) -> &str {
        &self.id
    }
}
#[derive(Serialize)]
pub struct UploadStatus {
    pub id: String,
    pub state: String,
    pub source: String,
    pub attempt: u32,
    pub error_code: Option<String>,
    pub receipt: Option<SnapshotReceipt>,
}

pub struct CollectorOutbox {
    conn: Mutex<Connection>,
    // Drop SQLite first, then release the cooperative OS writer lease. This
    // reuses only the file lock primitive, not the business schema/database.
    _lease: BusinessDbLease,
}
impl CollectorOutbox {
    pub fn open(path: &Path) -> ClientResult<Self> {
        let lease = BusinessDbLease::acquire(path).map_err(|_| ClientError::Storage)?;
        // Protect a newly created conversation queue with owner-only Unix mode.
        if !path.exists() {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options.open(path).map_err(|_| ClientError::Storage)?;
        }
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(2))?;
        let app: i64 = conn.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let tables: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        if !(app == APP_ID && version == 1) && !(app == 0 && version == 0 && tables == 0) {
            return Err(ClientError::Storage);
        }
        // Refuse a foreign DB before journal/schema PRAGMAs or any data writes.
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if app == 0 {
            tx.execute_batch("CREATE TABLE collector_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
             CREATE TABLE collector_upload(id TEXT PRIMARY KEY,instance_id TEXT NOT NULL,space_id TEXT NOT NULL,
             registration_id TEXT NOT NULL,upstream_id TEXT NOT NULL,submission_id TEXT NOT NULL,source TEXT NOT NULL,
             expected_revision INTEGER NOT NULL,body BLOB,request_hash TEXT NOT NULL,content_hash TEXT NOT NULL,
             state TEXT NOT NULL CHECK(state IN ('pending','inflight','blocked','acknowledged')),
             attempt INTEGER NOT NULL DEFAULT 0,due_ms INTEGER NOT NULL DEFAULT 0,lease_id TEXT,lease_until INTEGER,
             error_code TEXT,receipt_json TEXT,
             UNIQUE(instance_id,space_id,registration_id,upstream_id,submission_id));
             CREATE INDEX collector_pending ON collector_upload(instance_id,space_id,state,due_ms);
             CREATE UNIQUE INDEX collector_one_generation ON collector_upload(instance_id,space_id,registration_id,upstream_id)
             WHERE state IN ('pending','inflight','blocked');
             CREATE TABLE collector_cursor(instance_id TEXT NOT NULL,space_id TEXT NOT NULL,registration_id TEXT NOT NULL,
             upstream_id TEXT NOT NULL,revision INTEGER NOT NULL,content_hash TEXT NOT NULL,
             PRIMARY KEY(instance_id,space_id,registration_id,upstream_id));")?;
            tx.pragma_update(None, "application_id", APP_ID)?;
            tx.pragma_update(None, "user_version", 1)?;
        }
        // Only the process which acquired the writer lease can recover claims.
        tx.execute("UPDATE collector_upload SET state='pending',lease_id=NULL,lease_until=NULL,due_ms=0 WHERE state='inflight'",[])?;
        tx.execute("INSERT INTO collector_meta(key,value) VALUES ('device_id',?1) ON CONFLICT(key) DO NOTHING",[Uuid::new_v4().to_string()])?;
        tx.commit()?;
        Ok(Self {
            conn: Mutex::new(conn),
            _lease: lease,
        })
    }
    fn conn(&self) -> ClientResult<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| ClientError::Storage)
    }
    pub fn device_id(&self) -> ClientResult<String> {
        Ok(self.conn()?.query_row(
            "SELECT value FROM collector_meta WHERE key='device_id'",
            [],
            |r| r.get(0),
        )?)
    }
    pub(super) fn registration(&self, key: &str) -> ClientResult<Option<String>> {
        let key = format!("registration/{key}");
        let value: Option<String> = self.conn()?.query_row("SELECT value FROM collector_meta WHERE key=?1", [key], |r| r.get(0)).optional()?;
        if value.as_ref().is_some_and(|v| !super::valid_id(v)) {
            return Err(ClientError::Storage);
        }
        Ok(value)
    }
    pub(super) fn remember_registration(&self, key: &str, id: &str) -> ClientResult<()> {
        if key.len() > 2048 || !super::valid_id(id) {
            return Err(ClientError::InvalidInput);
        }
        let key = format!("registration/{key}");
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO collector_meta(key,value) VALUES (?1,?2) ON CONFLICT(key) DO NOTHING", params![key,id])?;
        let value: String = tx.query_row("SELECT value FROM collector_meta WHERE key=?1", [&key], |r| r.get(0))?;
        if value != id {
            return Err(ClientError::Conflict);
        }
        tx.commit()?;
        Ok(())
    }
    pub fn enqueue(&self, pending: &PendingSubmission) -> ClientResult<EnqueueOutcome> {
        let s = pending.submission();
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exact:Option<(String,String)>=tx.query_row(
            "SELECT id,request_hash FROM collector_upload WHERE instance_id=?1 AND space_id=?2 AND registration_id=?3 AND upstream_id=?4 AND submission_id=?5",
            params![s.service_instance_id,s.space_id,s.source_registration_id,s.session.external_session_id,s.submission_id],
            |r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((id, hash)) = exact {
            return if hash == pending.request_hash {
                Ok(EnqueueOutcome::Existing(id))
            } else {
                Err(ClientError::Conflict)
            };
        }
        let active:Option<(String,String,u32)>=tx.query_row(
            "SELECT id,content_hash,expected_revision FROM collector_upload WHERE instance_id=?1 AND space_id=?2 AND registration_id=?3 AND upstream_id=?4 AND state IN ('pending','inflight','blocked')",
            params![s.service_instance_id,s.space_id,s.source_registration_id,s.session.external_session_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((id, hash, revision)) = active {
            return if hash == pending.content_hash && revision == s.expected_revision {
                Ok(EnqueueOutcome::Existing(id))
            } else {
                Err(ClientError::Busy)
            };
        }
        let cursor:Option<(u32,String)>=tx.query_row(
            "SELECT revision,content_hash FROM collector_cursor WHERE instance_id=?1 AND space_id=?2 AND registration_id=?3 AND upstream_id=?4",
            params![s.service_instance_id,s.space_id,s.source_registration_id,s.session.external_session_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let expected = cursor.as_ref().map_or(0, |(r, _)| *r);
        if expected != s.expected_revision {
            return Err(ClientError::Conflict);
        }
        if cursor.is_some_and(|(_, hash)| hash == pending.content_hash) {
            return Ok(EnqueueOutcome::Unchanged);
        }
        let (count,bytes):(i64,i64)=tx.query_row("SELECT COUNT(*),COALESCE(SUM(length(body)),0) FROM collector_upload WHERE state<>'acknowledged'",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        if count >= MAX_PENDING_ROWS
            || bytes.saturating_add(pending.body.len() as i64) > MAX_PENDING_BYTES
        {
            return Err(ClientError::TooLarge);
        }
        let id = Uuid::new_v4().to_string();
        tx.execute("INSERT INTO collector_upload(id,instance_id,space_id,registration_id,upstream_id,submission_id,source,expected_revision,body,request_hash,content_hash,state)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'pending')",
            params![id,s.service_instance_id,s.space_id,s.source_registration_id,s.session.external_session_id,s.submission_id,s.session.source.as_str(),s.expected_revision,pending.body,pending.request_hash,pending.content_hash])?;
        tx.commit()?;
        Ok(EnqueueOutcome::Queued(id))
    }
    pub fn next_for(
        &self,
        instance: &str,
        space: &str,
        now: u64,
    ) -> ClientResult<Option<ClaimedUpload>> {
        let now = bounded_time(now);
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE collector_upload SET state='pending',lease_id=NULL,lease_until=NULL WHERE instance_id=?1 AND space_id=?2 AND state='inflight' AND lease_until<=?3",params![instance,space,now])?;
        type Row = (
            String,
            Vec<u8>,
            String,
            String,
            String,
            String,
            String,
            u32,
            u32,
        );
        let row:Option<Row>=tx.query_row(
            "SELECT id,body,request_hash,content_hash,registration_id,upstream_id,submission_id,expected_revision,attempt
             FROM collector_upload WHERE instance_id=?1 AND space_id=?2 AND state='pending' AND due_ms<=?3 ORDER BY rowid LIMIT 1",
            params![instance,space,now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?))).optional()?;
        let Some((
            id,
            body,
            hash,
            content_hash,
            registration,
            upstream,
            submission,
            expected,
            attempt,
        )) = row
        else {
            tx.commit()?;
            return Ok(None);
        };
        if body.len() > 16 * 1024 * 1024 {
            return Err(ClientError::Storage);
        }
        let input = serde_json::from_slice(&body).map_err(|_| ClientError::Storage)?;
        let mut pending = PendingSubmission::new(input).map_err(|_| ClientError::Storage)?;
        let s = pending.submission();
        if s.service_instance_id != instance
            || s.space_id != space
            || s.source_registration_id != registration
            || s.session.external_session_id != upstream
            || s.submission_id != submission
            || s.expected_revision != expected
            || pending.request_hash != hash
            || pending.content_hash != content_hash
        {
            return Err(ClientError::Storage);
        }
        // Retain exactly the serialized envelope which was queued, including map order.
        pending.body = body;
        let attempt = attempt.checked_add(1).ok_or(ClientError::Storage)?;
        let lease = Uuid::new_v4().to_string();
        tx.execute("UPDATE collector_upload SET state='inflight',attempt=?2,lease_id=?3,lease_until=?4,error_code=NULL WHERE id=?1",
            params![id,attempt,lease,now.saturating_add(LEASE_MS as i64)])?;
        tx.commit()?;
        Ok(Some(ClaimedUpload {
            id,
            lease,
            pending,
            attempt,
        }))
    }
    pub fn record_receipt(
        &self,
        claim: &ClaimedUpload,
        receipt: &SnapshotReceipt,
    ) -> ClientResult<()> {
        validate_receipt(&claim.pending, receipt)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_claim(&tx, claim)?;
        let s = claim.pending.submission();
        let current:Option<u32>=tx.query_row("SELECT revision FROM collector_cursor WHERE instance_id=?1 AND space_id=?2 AND registration_id=?3 AND upstream_id=?4",
            params![s.service_instance_id,s.space_id,s.source_registration_id,s.session.external_session_id],|r|r.get(0)).optional()?;
        if current.unwrap_or(0) != s.expected_revision {
            return Err(ClientError::Conflict);
        }
        tx.execute("INSERT INTO collector_cursor(instance_id,space_id,registration_id,upstream_id,revision,content_hash) VALUES (?1,?2,?3,?4,?5,?6)
            ON CONFLICT(instance_id,space_id,registration_id,upstream_id) DO UPDATE SET revision=excluded.revision,content_hash=excluded.content_hash",
            params![s.service_instance_id,s.space_id,s.source_registration_id,s.session.external_session_id,receipt.revision,claim.pending.content_hash])?;
        let json = serde_json::to_string(receipt).map_err(|_| ClientError::InvalidResponse)?;
        tx.execute("UPDATE collector_upload SET state='acknowledged',receipt_json=?2,body=NULL,lease_id=NULL,lease_until=NULL,error_code=NULL WHERE id=?1",params![claim.id,json])?;
        tx.execute("DELETE FROM collector_upload WHERE state='acknowledged' AND id NOT IN (SELECT id FROM collector_upload WHERE state='acknowledged' ORDER BY rowid DESC LIMIT 1000)",[])?;
        tx.commit()?;
        Ok(())
    }
    pub fn record_failure(
        &self,
        claim: &ClaimedUpload,
        error: ClientError,
        now: u64,
    ) -> ClientResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_claim(&tx, claim)?;
        let retry = error == ClientError::Retryable;
        let delay = (1_000_u64.saturating_mul(1_u64 << claim.attempt.min(8))).min(300_000);
        tx.execute("UPDATE collector_upload SET state=?2,due_ms=?3,lease_id=NULL,lease_until=NULL,error_code=?4 WHERE id=?1",
            params![claim.id,if retry{"pending"}else{"blocked"},bounded_time(now.saturating_add(delay)),error.code()])?;
        tx.commit()?;
        Ok(())
    }
    pub fn revision_for(
        &self,
        instance: &str,
        space: &str,
        registration: &str,
        upstream: &str,
    ) -> ClientResult<u32> {
        Ok(self.conn()?.query_row("SELECT revision FROM collector_cursor WHERE instance_id=?1 AND space_id=?2 AND registration_id=?3 AND upstream_id=?4",
            params![instance,space,registration,upstream],|r|r.get(0)).optional()?.unwrap_or(0))
    }
    pub fn statuses(&self, instance: &str, space: &str) -> ClientResult<Vec<UploadStatus>> {
        let conn = self.conn()?;
        let mut statement=conn.prepare("SELECT id,state,source,attempt,error_code,receipt_json FROM collector_upload WHERE instance_id=?1 AND space_id=?2 ORDER BY rowid DESC LIMIT 100")?;
        let rows = statement.query_map(params![instance, space], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, u32>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, state, source, attempt, error_code, json) = row?;
            let receipt = json
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(|_| ClientError::Storage)?;
            result.push(UploadStatus {
                id,
                state,
                source,
                attempt,
                error_code,
                receipt,
            });
        }
        Ok(result)
    }
}
fn bounded_time(now: u64) -> i64 {
    now.min((i64::MAX as u64) - LEASE_MS - 1) as i64
}
fn require_claim(tx: &rusqlite::Transaction<'_>, claim: &ClaimedUpload) -> ClientResult<()> {
    let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM collector_upload WHERE id=?1 AND state='inflight' AND lease_id=?2 AND request_hash=?3)",
        params![claim.id,claim.lease,claim.pending.request_hash],|r|r.get(0))?;
    if !valid {
        return Err(ClientError::Conflict);
    }
    Ok(())
}
