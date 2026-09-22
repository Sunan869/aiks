//! Authorized bounded read models. Call these only from blocking DB workers.
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{model::NormalizedSession, search::{SearchCorpus, UnifiedSearchOutcome}, storage::StateDb};
use super::{LocalContext, ServiceError, SnapshotReceipt};

pub const MAX_CONTENT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Page {
    pub limit: usize,
    pub offset: usize,
}
impl Default for Page { fn default() -> Self { Self { limit: 30, offset: 0 } } }
impl Page {
    pub fn validate(&self) -> Result<(), ServiceError> {
        if self.limit == 0 || self.limit > 100 || self.offset > 1_000_000 { return Err(ServiceError::InvalidInput); }
        Ok(())
    }
}

pub fn epoch(conn: &Connection) -> Result<i64, ServiceError> {
    Ok(conn.query_row("SELECT generation FROM service_read_epoch WHERE singleton=1", [], |row| row.get(0))?)
}

pub fn sessions(db: &StateDb, ctx: &LocalContext, page: Page) -> Result<Value, ServiceError> {
    page.validate()?;
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT ss.id,ss.source,substr(COALESCE(ss.title,''),1,4096),b.current_revision,
                d.indexed_revision,d.completed_revision
         FROM service_session_binding b JOIN source_session ss ON ss.id=b.session_id
         LEFT JOIN service_derived_state d ON d.session_id=b.session_id
         WHERE b.principal_id=?1 AND b.space_id=?2 AND ss.is_missing=0
         ORDER BY ss.id DESC LIMIT ?3 OFFSET ?4")?;
    let items = stmt.query_map(params![ctx.principal_id(),ctx.space_id(),page.limit as i64,page.offset as i64], |row| {
        let revision: u32 = row.get(3)?;
        let indexed: Option<u32> = row.get(4)?;
        Ok(json!({"session_id":row.get::<_,i64>(0)?.to_string(),"source":row.get::<_,String>(1)?,
            "title":row.get::<_,String>(2)?,"revision":revision,"indexed_revision":indexed,
            "completed_revision":row.get::<_,Option<u32>>(5)?,"index_current":indexed==Some(revision)}))
    })?.collect::<Result<Vec<_>,_>>()?;
    Ok(json!({"items":items,"limit":page.limit,"offset":page.offset}))
}

pub fn session(db: &StateDb, ctx: &LocalContext, id: &str) -> Result<Value, ServiceError> {
    let id: i64 = id.parse().map_err(|_| ServiceError::NotFound)?;
    let conn = db.conn();
    let (revision, bytes): (u32, Option<Vec<u8>>) = conn.query_row(
        "SELECT s.revision,CASE WHEN length(s.canonical_json)<=16777216 THEN s.canonical_json END
         FROM service_session_binding b JOIN source_session ss ON ss.id=b.session_id
         JOIN service_session_snapshot s ON s.session_id=b.session_id AND s.revision=b.current_revision
         WHERE b.session_id=?1 AND b.principal_id=?2 AND b.space_id=?3 AND ss.is_missing=0",
        params![id,ctx.principal_id(),ctx.space_id()], |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional()?.ok_or(ServiceError::NotFound)?;
    let mut session: NormalizedSession = serde_json::from_slice(&bytes.ok_or(ServiceError::TooLarge)?)
        .map_err(|_| ServiceError::Internal)?;
    session.source_path = None;
    session.project_path = None;
    session.metadata.clear();
    for message in &mut session.messages { message.metadata.clear(); }
    Ok(json!({"session_id":id.to_string(),"revision":revision,"session":session}))
}

pub fn receipt(db: &StateDb, ctx: &LocalContext, id: &str) -> Result<SnapshotReceipt, ServiceError> {
    Ok(db.conn().query_row(
        "SELECT r.id,s.session_id,s.id,s.revision,r.durable_job_id,r.pipeline_run_id
         FROM service_ingest_receipt r JOIN service_session_snapshot s ON s.id=r.snapshot_id
         JOIN service_session_binding b ON b.session_id=s.session_id
         WHERE r.id=?1 AND r.principal_id=?2 AND r.space_id=?3
           AND b.principal_id=?2 AND b.space_id=?3",
        params![id,ctx.principal_id(),ctx.space_id()], |row| Ok(SnapshotReceipt {
            receipt_id:row.get(0)?,session_id:row.get::<_,i64>(1)?.to_string(),snapshot_id:row.get(2)?,
            revision:row.get(3)?,job_id:row.get(4)?,pipeline_run_id:row.get(5)?,state:"accepted".into(),
        }),
    ).optional()?.ok_or(ServiceError::NotFound)?)
}

pub fn job(db: &StateDb, ctx: &LocalContext, id: &str) -> Result<Value, ServiceError> {
    Ok(db.conn().query_row(
        "SELECT j.id,j.status,j.attempt,r.status,s.revision,b.current_revision
         FROM pipeline_job j JOIN service_job_input i ON i.durable_job_id=j.id
         JOIN pipeline_run r ON r.id=i.pipeline_run_id AND r.id=j.pipeline_run_id
         JOIN service_session_snapshot s ON s.id=i.snapshot_id AND s.session_id=j.session_id
         JOIN service_session_binding b ON b.session_id=s.session_id
         WHERE j.id=?1 AND b.principal_id=?2 AND b.space_id=?3",
        params![id,ctx.principal_id(),ctx.space_id()], |row| {
            let status: String = row.get(1)?;
            Ok(json!({"job_id":row.get::<_,String>(0)?,"status":status,
                "attempt":row.get::<_,i64>(2)?,"pipeline_status":row.get::<_,String>(3)?,
                "revision":row.get::<_,u32>(4)?,"current_revision":row.get::<_,u32>(5)?,
                "error_code":if status=="FAILED" {Some("processing_failed")} else {None}}))
        },
    ).optional()?.ok_or(ServiceError::NotFound)?)
}

#[derive(Clone, Serialize)]
pub struct KnowledgeView {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub category: String,
    pub tags: Vec<String>,
    pub revision: Option<u32>,
    pub current_revision: u32,
    pub stale: bool,
    pub content_state: String,
    pub content: Option<String>,
    #[serde(skip)] pub(crate) doc_id: Option<String>,
    #[serde(skip)] pub(crate) generation: i64,
}

pub fn knowledge(db: &StateDb, ctx: &LocalContext, id: &str) -> Result<KnowledgeView, ServiceError> {
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    let generation = epoch(&tx)?;
    let mut result = tx.query_row(
        "SELECT ki.id,substr(ki.title,1,4096),substr(ki.summary,1,16384),substr(ki.category,1,256),
            CASE WHEN length(ki.tags)<=65536 THEN ki.tags ELSE '[]' END,
            d.knowledge_revision,b.current_revision,ki.siyuan_doc_id,
            CASE WHEN ki.siyuan_doc_id IS NULL AND length(ki.content)<=1048576 THEN ki.content END
         FROM knowledge_item ki JOIN service_session_binding b ON b.session_id=ki.source_session_id
         LEFT JOIN service_derived_state d ON d.session_id=b.session_id
         WHERE ki.id=?1 AND ki.status='active' AND b.principal_id=?2 AND b.space_id=?3",
        params![id,ctx.principal_id(),ctx.space_id()], |row| {
            let revision: Option<u32> = row.get(5)?;
            let current_revision: u32 = row.get(6)?;
            let doc_id: Option<String> = row.get(7)?;
            let tags: String = row.get(4)?;
            Ok(KnowledgeView { id:row.get(0)?,title:row.get(1)?,summary:row.get(2)?,category:row.get(3)?,
                tags:serde_json::from_str(&tags).unwrap_or_default(),revision,current_revision,
                stale:revision!=Some(current_revision),content_state:if doc_id.is_some(){"published"}else{"draft"}.into(),
                content:row.get(8)?,doc_id,generation })
        },
    ).optional()?.ok_or(ServiceError::NotFound)?;
    // Draft overflow is an explicit unavailable body, never silently truncated.
    if result.doc_id.is_none() && result.content.is_none() { result.content_state = "draft_too_large".into(); }
    Ok(result)
}

pub fn knowledge_list(db: &StateDb, ctx: &LocalContext, page: Page) -> Result<Value, ServiceError> {
    page.validate()?;
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT ki.id,substr(ki.title,1,4096),d.knowledge_revision,b.current_revision,ki.siyuan_doc_id IS NOT NULL
         FROM knowledge_item ki JOIN service_session_binding b ON b.session_id=ki.source_session_id
         LEFT JOIN service_derived_state d ON d.session_id=b.session_id
         WHERE ki.status='active' AND b.principal_id=?1 AND b.space_id=?2
         ORDER BY ki.rowid DESC LIMIT ?3 OFFSET ?4")?;
    let items = stmt.query_map(params![ctx.principal_id(),ctx.space_id(),page.limit as i64,page.offset as i64], |row| {
        let revision: Option<u32> = row.get(2)?;
        let current: u32 = row.get(3)?;
        Ok(json!({"id":row.get::<_,String>(0)?,"title":row.get::<_,String>(1)?,"revision":revision,
            "current_revision":current,"stale":revision!=Some(current),
            "content_state":if row.get::<_,bool>(4)? {"published"} else {"draft"}}))
    })?.collect::<Result<Vec<_>,_>>()?;
    Ok(json!({"items":items,"limit":page.limit,"offset":page.offset}))
}

pub fn search_response(db: &StateDb, ctx: &LocalContext, before: i64, results: UnifiedSearchOutcome) -> Result<Value, ServiceError> {
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    if epoch(&tx)? != before { return Err(ServiceError::Conflict); }
    let mut hits = Vec::with_capacity(results.hits.len());
    for hit in results.hits {
        let sql = match hit.corpus {
            SearchCorpus::Session => "SELECT b.current_revision FROM service_session_binding b
                JOIN service_derived_state d ON d.session_id=b.session_id WHERE CAST(b.session_id AS TEXT)=?1
                AND b.principal_id=?2 AND b.space_id=?3 AND d.indexed_revision=b.current_revision",
            SearchCorpus::Knowledge => "SELECT b.current_revision FROM service_session_binding b
                JOIN knowledge_item ki ON ki.source_session_id=b.session_id
                JOIN service_derived_state d ON d.session_id=b.session_id WHERE ki.id=?1
                AND b.principal_id=?2 AND b.space_id=?3 AND d.knowledge_revision=b.current_revision AND ki.status='active'",
        };
        // Recall has already applied the scope BEFORE its limits. This is a
        // second consistency fence, not an authorization-by-post-filter design.
        let revision: u32 = tx.query_row(sql, params![hit.entity_id,ctx.principal_id(),ctx.space_id()], |row| row.get(0))
            .optional()?.ok_or(ServiceError::Conflict)?;
        hits.push(json!({"corpus":hit.corpus,"entity_id":hit.entity_id,"title":hit.title,
            "snippet":hit.snippet,"score":hit.score,"match_types":hit.match_types,"revision":revision,
            "snippet_kind":"indexed_projection"}));
    }
    let warnings: Vec<&str> = if results.degraded { vec!["search_partially_available"] } else { vec![] };
    Ok(json!({"hits":hits,"degraded":results.degraded,"warnings":warnings}))
}
