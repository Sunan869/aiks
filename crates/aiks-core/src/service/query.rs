//! Authorized bounded read models. Call these only from blocking DB workers.
use super::{LocalContext, RequestContext, ServiceError, SnapshotReceipt};
use crate::{
    model::NormalizedSession,
    search::{SearchCorpus, UnifiedSearchOutcome},
    storage::StateDb,
    team::{knowledge_access_in_conn, Action},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MAX_CONTENT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Page {
    pub limit: usize,
    pub offset: usize,
}
impl Default for Page {
    fn default() -> Self {
        Self {
            limit: 30,
            offset: 0,
        }
    }
}
impl Page {
    pub fn validate(&self) -> Result<(), ServiceError> {
        if self.limit == 0 || self.limit > 100 || self.offset > 1_000_000 {
            return Err(ServiceError::InvalidInput);
        }
        Ok(())
    }
}

pub fn epoch(conn: &Connection) -> Result<i64, ServiceError> {
    Ok(conn.query_row(
        "SELECT generation FROM service_read_epoch WHERE singleton=1",
        [],
        |row| row.get(0),
    )?)
}

fn personal(ctx: &LocalContext) -> RequestContext {
    RequestContext::Personal(ctx.clone())
}

pub fn sessions(db: &StateDb, ctx: &LocalContext, page: Page) -> Result<Value, ServiceError> {
    sessions_for(db, &personal(ctx), 0, page)
}

pub fn sessions_for(
    db: &StateDb,
    ctx: &RequestContext,
    now: u64,
    page: Page,
) -> Result<Value, ServiceError> {
    page.validate()?;
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    ctx.authorize_in_conn(&tx, now)?;
    let mut stmt = tx.prepare(
        "SELECT ss.id,ss.source,substr(COALESCE(ss.title,''),1,4096),b.current_revision,
                d.indexed_revision,d.completed_revision
         FROM service_session_binding b JOIN source_session ss ON ss.id=b.session_id
         LEFT JOIN service_derived_state d ON d.session_id=b.session_id
         WHERE b.principal_id=?1 AND b.space_id=?2
         ORDER BY ss.id DESC LIMIT ?3 OFFSET ?4",
    )?;
    let items = stmt
        .query_map(
            params![
                ctx.principal_id(),
                ctx.space_id(),
                page.limit as i64,
                page.offset as i64
            ],
            |row| {
                let revision: u32 = row.get(3)?;
                let indexed: Option<u32> = row.get(4)?;
                Ok(json!({"session_id":row.get::<_,i64>(0)?.to_string(),"source":row.get::<_,String>(1)?,
                    "title":row.get::<_,String>(2)?,"revision":revision,"indexed_revision":indexed,
                    "completed_revision":row.get::<_,Option<u32>>(5)?,"index_current":revision>0 && indexed==Some(revision),
                    "snapshot_state":if revision==0 {"source_unavailable"} else {"ready"}}))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"items":items,"limit":page.limit,"offset":page.offset}))
}

pub fn session(db: &StateDb, ctx: &LocalContext, id: &str) -> Result<Value, ServiceError> {
    session_for(db, &personal(ctx), 0, id)
}

pub fn session_for(
    db: &StateDb,
    ctx: &RequestContext,
    now: u64,
    id: &str,
) -> Result<Value, ServiceError> {
    let id: i64 = id.parse().map_err(|_| ServiceError::NotFound)?;
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    ctx.authorize_in_conn(&tx, now)?;
    let (revision, title, source): (u32, String, String) = tx
        .query_row(
            "SELECT b.current_revision, substr(COALESCE(ss.title,''),1,4096),ss.source
             FROM service_session_binding b JOIN source_session ss ON ss.id=b.session_id
             WHERE b.session_id=?1 AND b.principal_id=?2 AND b.space_id=?3",
            params![id, ctx.principal_id(), ctx.space_id()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or(ServiceError::NotFound)?;
    if revision == 0 {
        let projection: Option<Option<String>> = tx
            .query_row(
                "SELECT CASE WHEN length(CAST(content AS BLOB))<=1048576 THEN content END
                 FROM session_search_fts WHERE session_id=?1 LIMIT 1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        return Ok(
            json!({"session_id":id.to_string(),"title":title,"source":source,
            "revision":null,"session":null,"snapshot_state":"source_unavailable",
            "content_state":if projection.is_some(){"legacy_projection"}else{"unavailable"},
            "content":projection.flatten(),"index_current":false}),
        );
    }
    let bytes: Option<Vec<u8>> = tx
        .query_row(
            "SELECT CASE WHEN length(canonical_json)<=16777216 THEN canonical_json END
             FROM service_session_snapshot WHERE session_id=?1 AND revision=?2",
            params![id, revision],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(ServiceError::Internal)?;
    let mut session: NormalizedSession =
        serde_json::from_slice(&bytes.ok_or(ServiceError::TooLarge)?)
            .map_err(|_| ServiceError::Internal)?;
    session.source_path = None;
    session.project_path = None;
    session.metadata.clear();
    for message in &mut session.messages {
        message.metadata.clear();
    }
    Ok(
        json!({"session_id":id.to_string(),"revision":revision,"session":session,"snapshot_state":"ready"}),
    )
}

pub fn receipt(
    db: &StateDb,
    ctx: &LocalContext,
    id: &str,
) -> Result<SnapshotReceipt, ServiceError> {
    receipt_for(db, &personal(ctx), 0, id)
}

pub fn receipt_for(
    db: &StateDb,
    ctx: &RequestContext,
    now: u64,
    id: &str,
) -> Result<SnapshotReceipt, ServiceError> {
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    ctx.authorize_in_conn(&tx, now)?;
    tx.query_row(
        "SELECT r.id,s.session_id,s.id,s.revision,r.durable_job_id,r.pipeline_run_id
         FROM service_ingest_receipt r JOIN service_session_snapshot s ON s.id=r.snapshot_id
         JOIN service_session_binding b ON b.session_id=s.session_id
         WHERE r.id=?1 AND r.principal_id=?2 AND r.space_id=?3
           AND b.principal_id=?2 AND b.space_id=?3",
        params![id, ctx.principal_id(), ctx.space_id()],
        |row| {
            Ok(SnapshotReceipt {
                receipt_id: row.get(0)?,
                session_id: row.get::<_, i64>(1)?.to_string(),
                snapshot_id: row.get(2)?,
                revision: row.get(3)?,
                job_id: row.get(4)?,
                pipeline_run_id: row.get(5)?,
                state: "accepted".into(),
            })
        },
    )
    .optional()?
    .ok_or(ServiceError::NotFound)
}

pub fn job(db: &StateDb, ctx: &LocalContext, id: &str) -> Result<Value, ServiceError> {
    job_for(db, &personal(ctx), 0, id)
}

pub fn job_for(
    db: &StateDb,
    ctx: &RequestContext,
    now: u64,
    id: &str,
) -> Result<Value, ServiceError> {
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    ctx.authorize_in_conn(&tx, now)?;
    tx.query_row(
        "SELECT j.id,j.status,j.attempt,r.status,s.revision,b.current_revision
         FROM pipeline_job j JOIN service_job_input i ON i.durable_job_id=j.id
         JOIN pipeline_run r ON r.id=i.pipeline_run_id AND r.id=j.pipeline_run_id
         JOIN service_session_snapshot s ON s.id=i.snapshot_id AND s.session_id=j.session_id
         JOIN service_session_binding b ON b.session_id=s.session_id
         WHERE j.id=?1 AND b.principal_id=?2 AND b.space_id=?3",
        params![id, ctx.principal_id(), ctx.space_id()],
        |row| {
            let status: String = row.get(1)?;
            Ok(json!({"job_id":row.get::<_,String>(0)?,"status":status,
                "attempt":row.get::<_,i64>(2)?,"pipeline_status":row.get::<_,String>(3)?,
                "revision":row.get::<_,u32>(4)?,"current_revision":row.get::<_,u32>(5)?,
                "error_code":if status=="FAILED" {Some("processing_failed")} else {None}}))
        },
    )
    .optional()?
    .ok_or(ServiceError::NotFound)
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
    pub content_revision: Option<u64>,
    pub can_manage: bool,
    pub share_source: String,
    pub stale: bool,
    pub content_state: String,
    pub content: Option<String>,
    #[serde(skip)]
    pub(crate) doc_id: Option<String>,
    #[serde(skip)]
    pub(crate) generation: i64,
}

pub fn knowledge(
    db: &StateDb,
    ctx: &LocalContext,
    id: &str,
) -> Result<KnowledgeView, ServiceError> {
    knowledge_for(db, &personal(ctx), 0, id)
}

pub fn knowledge_for(
    db: &StateDb,
    ctx: &RequestContext,
    now: u64,
    id: &str,
) -> Result<KnowledgeView, ServiceError> {
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    ctx.authorize_in_conn(&tx, now)?;
    let generation = epoch(&tx)?;
    if let Some(team) = ctx.team() {
        knowledge_access_in_conn(&tx, team, id, Action::Read, now)?;
        let content_pending: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM team_content_operation
             WHERE company_id=?1 AND knowledge_id=?2 AND state IN ('applying','verifying'))",
            params![team.company_id(), id],
            |row| row.get(0),
        )?;
        if content_pending {
            return Err(ServiceError::ContentPending);
        }
    }
    let sql = if ctx.is_team() {
        "SELECT ki.id,substr(ki.title,1,4096),substr(ki.summary,1,16384),substr(ki.category,1,256),
                CASE WHEN length(ki.tags)<=65536 THEN ki.tags ELSE '[]' END,
                d.revision,COALESCE(b.current_revision,0),ki.siyuan_doc_id,
                CASE WHEN ki.siyuan_doc_id IS NULL AND length(CAST(ki.content AS BLOB))<=1048576 THEN ki.content END,
                o.content_revision,o.owner_user_id=?3,
                CASE WHEN o.owner_user_id=?3 THEN 'mine'
                     WHEN EXISTS(SELECT 1 FROM document_share_grant g
                                 WHERE g.company_id=o.company_id AND g.knowledge_id=o.knowledge_id
                                   AND g.target_user_id=?3) THEN 'shared_to_me'
                     ELSE 'department' END
         FROM knowledge_item ki
         JOIN team_knowledge_owner o ON o.knowledge_id=ki.id AND o.company_id=?2
         LEFT JOIN service_session_binding b ON b.session_id=ki.source_session_id
         LEFT JOIN service_knowledge_revision d ON d.session_id=b.session_id AND d.knowledge_id=ki.id
         WHERE ki.id=?1 AND ki.status='active'"
    } else {
        "SELECT ki.id,substr(ki.title,1,4096),substr(ki.summary,1,16384),substr(ki.category,1,256),
                CASE WHEN length(ki.tags)<=65536 THEN ki.tags ELSE '[]' END,
                d.revision,COALESCE(b.current_revision,0),ki.siyuan_doc_id,
                CASE WHEN ki.siyuan_doc_id IS NULL AND length(CAST(ki.content AS BLOB))<=1048576 THEN ki.content END,
                NULL,1,'mine'
         FROM knowledge_item ki LEFT JOIN service_session_binding b ON b.session_id=ki.source_session_id
         LEFT JOIN service_knowledge_binding kb ON kb.knowledge_id=ki.id AND ki.source_session_id IS NULL
         LEFT JOIN service_knowledge_revision d ON d.session_id=b.session_id AND d.knowledge_id=ki.id
         WHERE ki.id=?1 AND ki.status='active' AND ((b.principal_id=?2 AND b.space_id=?3) OR (kb.principal_id=?2 AND kb.space_id=?3))"
    };
    let mut result = if let Some(team) = ctx.team() {
        tx.query_row(
            sql,
            params![id, team.company_id(), team.user_id()],
            knowledge_row,
        )
    } else {
        tx.query_row(
            sql,
            params![id, ctx.principal_id(), ctx.space_id()],
            knowledge_row,
        )
    }
    .optional()?
    .ok_or(ServiceError::NotFound)?;
    result.generation = generation;
    if result.doc_id.is_none() && result.content.is_none() {
        result.content_state = "draft_too_large".into();
    }
    Ok(result)
}

fn knowledge_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeView> {
    let revision: Option<u32> = row.get(5)?;
    let current_revision: u32 = row.get(6)?;
    let doc_id: Option<String> = row.get(7)?;
    let tags: String = row.get(4)?;
    Ok(KnowledgeView {
        id: row.get(0)?,
        title: row.get(1)?,
        summary: row.get(2)?,
        category: row.get(3)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        revision,
        current_revision,
        content_revision: row.get(9)?,
        can_manage: row.get(10)?,
        share_source: row.get(11)?,
        stale: revision != Some(current_revision),
        content_state: if doc_id.is_some() {
            "published"
        } else {
            "draft"
        }
        .into(),
        content: row.get(8)?,
        doc_id,
        generation: 0,
    })
}

pub fn knowledge_list(db: &StateDb, ctx: &LocalContext, page: Page) -> Result<Value, ServiceError> {
    knowledge_list_for(db, &personal(ctx), 0, page)
}

pub fn knowledge_list_for(
    db: &StateDb,
    ctx: &RequestContext,
    now: u64,
    page: Page,
) -> Result<Value, ServiceError> {
    page.validate()?;
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    ctx.authorize_in_conn(&tx, now)?;
    let items = if let Some(team) = ctx.team() {
        let mut stmt = tx.prepare(
            "SELECT ki.id,substr(ki.title,1,4096),d.revision,COALESCE(b.current_revision,0),ki.siyuan_doc_id IS NOT NULL,o.content_revision,
                    o.owner_user_id=?2,
                    CASE WHEN o.owner_user_id=?2 THEN 'mine'
                         WHEN EXISTS(SELECT 1 FROM document_share_grant sg
                                     WHERE sg.company_id=o.company_id AND sg.knowledge_id=o.knowledge_id
                                       AND sg.target_user_id=?2) THEN 'shared_to_me'
                         ELSE 'department' END
             FROM knowledge_item ki
             JOIN team_knowledge_owner o ON o.knowledge_id=ki.id AND o.company_id=?1
             LEFT JOIN service_session_binding b ON b.session_id=ki.source_session_id
             LEFT JOIN service_knowledge_revision d ON d.session_id=b.session_id AND d.knowledge_id=ki.id
             WHERE ki.status='active' AND (
                 o.owner_user_id=?2 OR EXISTS(
                     SELECT 1 FROM document_share_grant g WHERE g.company_id=o.company_id
                       AND g.knowledge_id=o.knowledge_id AND g.target_user_id=?2
                 ) OR EXISTS(
                     SELECT 1 FROM document_share_grant g
                     JOIN team_company c ON c.id=g.company_id
                     JOIN team_org_membership m ON m.company_id=g.company_id
                          AND m.generation=c.directory_generation AND m.user_id=?2
                     WHERE g.company_id=o.company_id AND g.knowledge_id=o.knowledge_id
                       AND g.target_org_id IS NOT NULL
                       AND (m.org_id=g.target_org_id OR (g.include_descendants=1 AND EXISTS(
                           SELECT 1 FROM team_org_closure oc WHERE oc.company_id=m.company_id
                             AND oc.generation=m.generation AND oc.ancestor_id=g.target_org_id
                             AND oc.descendant_id=m.org_id)))
                 )
             ) ORDER BY ki.rowid DESC LIMIT ?3 OFFSET ?4",
        )?;
        let items = stmt
            .query_map(
                params![
                    team.company_id(),
                    team.user_id(),
                    page.limit as i64,
                    page.offset as i64
                ],
                knowledge_list_row,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        items
    } else {
        let mut stmt = tx.prepare(
            "SELECT ki.id,substr(ki.title,1,4096),d.revision,COALESCE(b.current_revision,0),ki.siyuan_doc_id IS NOT NULL,NULL,1,'mine'
             FROM knowledge_item ki LEFT JOIN service_session_binding b ON b.session_id=ki.source_session_id
             LEFT JOIN service_knowledge_binding kb ON kb.knowledge_id=ki.id AND ki.source_session_id IS NULL
             LEFT JOIN service_knowledge_revision d ON d.session_id=b.session_id AND d.knowledge_id=ki.id
             WHERE ki.status='active' AND ((b.principal_id=?1 AND b.space_id=?2) OR (kb.principal_id=?1 AND kb.space_id=?2))
             ORDER BY ki.rowid DESC LIMIT ?3 OFFSET ?4",
        )?;
        let items = stmt
            .query_map(
                params![
                    ctx.principal_id(),
                    ctx.space_id(),
                    page.limit as i64,
                    page.offset as i64
                ],
                knowledge_list_row,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        items
    };
    Ok(json!({"items":items,"limit":page.limit,"offset":page.offset}))
}

fn knowledge_list_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let revision: Option<u32> = row.get(2)?;
    let current: u32 = row.get(3)?;
    Ok(
        json!({"id":row.get::<_,String>(0)?,"title":row.get::<_,String>(1)?,"revision":revision,
        "current_revision":current,"content_revision":row.get::<_,Option<u64>>(5)?,
        "can_manage":row.get::<_,bool>(6)?,"share_source":row.get::<_,String>(7)?,
        "stale":revision!=Some(current),
        "content_state":if row.get::<_,bool>(4)? {"published"} else {"draft"}}),
    )
}

pub fn search_response(
    db: &StateDb,
    ctx: &LocalContext,
    before: i64,
    results: UnifiedSearchOutcome,
) -> Result<Value, ServiceError> {
    search_response_for(db, &personal(ctx), 0, before, results)
}

pub fn search_response_for(
    db: &StateDb,
    ctx: &RequestContext,
    now: u64,
    before: i64,
    results: UnifiedSearchOutcome,
) -> Result<Value, ServiceError> {
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    ctx.authorize_in_conn(&tx, now)?;
    if epoch(&tx)? != before {
        return Err(ServiceError::Conflict);
    }
    let mut hits = Vec::with_capacity(results.hits.len());
    for hit in results.hits {
        let revision: u32 = match hit.corpus {
            SearchCorpus::Session => tx
                .query_row(
                    "SELECT b.current_revision FROM service_session_binding b
                     JOIN service_derived_state d ON d.session_id=b.session_id
                     WHERE CAST(b.session_id AS TEXT)=?1 AND b.principal_id=?2 AND b.space_id=?3
                       AND d.indexed_revision=b.current_revision",
                    params![hit.entity_id, ctx.principal_id(), ctx.space_id()],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or(ServiceError::Conflict)?,
            SearchCorpus::Knowledge => {
                if let Some(team) = ctx.team() {
                    knowledge_access_in_conn(&tx, team, &hit.entity_id, Action::Read, now)
                        .map_err(|_| ServiceError::Conflict)?;
                    tx.query_row(
                        "SELECT b.current_revision FROM service_session_binding b
                         JOIN knowledge_item ki ON ki.source_session_id=b.session_id
                         JOIN service_knowledge_revision d ON d.session_id=b.session_id AND d.knowledge_id=ki.id
                         WHERE ki.id=?1 AND d.revision=b.current_revision AND ki.status='active'",
                        [hit.entity_id.as_str()],
                        |row| row.get(0),
                    )
                    .optional()?
                    .ok_or(ServiceError::Conflict)?
                } else {
                    tx.query_row(
                        "SELECT b.current_revision FROM service_session_binding b
                         JOIN knowledge_item ki ON ki.source_session_id=b.session_id
                         JOIN service_knowledge_revision d ON d.session_id=b.session_id AND d.knowledge_id=ki.id
                         WHERE ki.id=?1 AND b.principal_id=?2 AND b.space_id=?3
                           AND d.revision=b.current_revision AND ki.status='active'",
                        params![hit.entity_id, ctx.principal_id(), ctx.space_id()],
                        |row| row.get(0),
                    )
                    .optional()?
                    .ok_or(ServiceError::Conflict)?
                }
            }
        };
        hits.push(json!({"corpus":hit.corpus,"entity_id":hit.entity_id,"title":hit.title,
            "snippet":hit.snippet,"score":hit.score,"match_types":hit.match_types,"revision":revision,
            "snippet_kind":"indexed_projection"}));
    }
    let warnings: Vec<&str> = if results.degraded {
        vec!["search_partially_available"]
    } else {
        vec![]
    };
    Ok(json!({"hits":hits,"degraded":results.degraded,"warnings":warnings}))
}
