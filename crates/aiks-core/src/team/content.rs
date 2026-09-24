//! Durable owner-only content operations. The SQLite intent is committed before
//! any SiYuan request; network calls never hold the database mutex/transaction.
use std::{sync::Arc, time::Duration};

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, watch, Mutex};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::{
    ai::ModelService,
    indexing::{KnowledgeIndexInput, KnowledgeIndexService},
    sink::SiYuanSink,
};

use super::{knowledge_access_in_conn, Action, TeamContext, TeamError, TeamStore};

pub const MAX_TEAM_CONTENT_BYTES: usize = 1024 * 1024;
pub const MAX_MANAGED_ASSET_BYTES: usize = 8 * 1024 * 1024;
const LEASE_SECONDS: u64 = 60;
const MAX_ATTEMPTS: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContentOperationKind {
    Content,
    Publish,
}
impl ContentOperationKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Publish => "publish",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ContentOperation {
    pub operation_id: String,
    pub knowledge_id: String,
    pub base_revision: u64,
    pub result_revision: Option<u64>,
    pub state: String,
}

#[derive(Clone, Debug)]
pub struct ManagedAsset {
    pub id: String,
    pub knowledge_id: String,
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct ClaimedOperation {
    id: String,
    knowledge_id: String,
    owner_user_id: String,
    origin_session_id: String,
    kind: ContentOperationKind,
    base_revision: u64,
    directory_max_age: u64,
    target_title: String,
    target_markdown: String,
    target_hash: String,
    expected_remote_hash: Option<String>,
    target_doc_id: Option<String>,
    target_category: Option<String>,
    lease_token: String,
    attempt_count: u32,
}

impl TeamStore {
    #[allow(clippy::too_many_arguments)]
    pub fn enqueue_content_update(
        &self,
        ctx: &TeamContext,
        knowledge_id: &str,
        operation_id: &str,
        base_revision: u64,
        title: &str,
        markdown: &str,
        now: u64,
    ) -> Result<ContentOperation, TeamError> {
        validate_title(title)?;
        validate_markdown(markdown)?;
        self.enqueue_operation(
            ctx,
            knowledge_id,
            operation_id,
            base_revision,
            ContentOperationKind::Content,
            title,
            markdown,
            now,
        )
    }

    pub fn enqueue_publish(
        &self,
        ctx: &TeamContext,
        knowledge_id: &str,
        operation_id: &str,
        base_revision: u64,
        now: u64,
    ) -> Result<ContentOperation, TeamError> {
        if !valid_operation_id(operation_id) || base_revision == 0 || now > i64::MAX as u64 {
            return Err(TeamError::InvalidInput);
        }
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ctx.authorize_in_conn(&tx, now)?;
        knowledge_access_in_conn(&tx, ctx, knowledge_id, Action::Edit, now)?;
        let row: (u64, String, String, String, Option<String>) = tx
            .query_row(
                "SELECT o.content_revision,ki.title,ki.category,ki.content,ki.siyuan_doc_id
                 FROM team_knowledge_owner o JOIN knowledge_item ki ON ki.id=o.knowledge_id
                 WHERE o.company_id=?1 AND o.knowledge_id=?2 AND o.owner_user_id=?3",
                params![self.company_id(), knowledge_id, ctx.user_id()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?
            .ok_or(TeamError::NotFound)?;
        if let Some((_, existing, owner)) =
            read_operation_with_hash(&tx, self.company_id(), operation_id)?
        {
            if existing.knowledge_id == knowledge_id
                && existing.base_revision == base_revision
                && owner == ctx.user_id()
            {
                return Ok(existing);
            }
            return Err(TeamError::Conflict);
        }
        if row.4.is_some() {
            return Err(TeamError::Conflict);
        }
        validate_title(&row.1)?;
        validate_markdown(&row.3)?;
        let operation = enqueue_operation_in_tx(
            &tx,
            self.company_id(),
            ctx,
            knowledge_id,
            operation_id,
            base_revision,
            ContentOperationKind::Publish,
            &row.1,
            &row.3,
            row.0,
            None,
            None,
            Some(&row.2),
            now,
        )?;
        tx.commit()?;
        Ok(operation)
    }

    #[allow(clippy::too_many_arguments)]
    fn enqueue_operation(
        &self,
        ctx: &TeamContext,
        knowledge_id: &str,
        operation_id: &str,
        base_revision: u64,
        kind: ContentOperationKind,
        title: &str,
        markdown: &str,
        now: u64,
    ) -> Result<ContentOperation, TeamError> {
        if !valid_operation_id(operation_id) || base_revision == 0 || now > i64::MAX as u64 {
            return Err(TeamError::InvalidInput);
        }
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ctx.authorize_in_conn(&tx, now)?;
        knowledge_access_in_conn(&tx, ctx, knowledge_id, Action::Edit, now)?;
        let row: (u64, Option<String>, Option<String>) = tx
            .query_row(
                "SELECT o.content_revision,ki.siyuan_doc_id,ki.current_remote_hash
                 FROM team_knowledge_owner o JOIN knowledge_item ki ON ki.id=o.knowledge_id
                 WHERE o.company_id=?1 AND o.knowledge_id=?2 AND o.owner_user_id=?3",
                params![self.company_id(), knowledge_id, ctx.user_id()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or(TeamError::NotFound)?;
        if row.1.is_some() && row.2.is_none() {
            // Never overwrite a canonical remote document without a proven baseline.
            return Err(TeamError::Conflict);
        }
        let operation = enqueue_operation_in_tx(
            &tx,
            self.company_id(),
            ctx,
            knowledge_id,
            operation_id,
            base_revision,
            kind,
            title,
            markdown,
            row.0,
            row.1.as_deref(),
            row.2.as_deref(),
            None,
            now,
        )?;
        tx.commit()?;
        Ok(operation)
    }

    pub fn content_operation(
        &self,
        ctx: &TeamContext,
        operation_id: &str,
        now: u64,
    ) -> Result<ContentOperation, TeamError> {
        if !valid_operation_id(operation_id) || now > i64::MAX as u64 {
            return Err(TeamError::InvalidInput);
        }
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        ctx.authorize_in_conn(&tx, now)?;
        let operation =
            read_operation(&tx, self.company_id(), operation_id)?.ok_or(TeamError::NotFound)?;
        knowledge_access_in_conn(&tx, ctx, &operation.knowledge_id, Action::Edit, now)?;
        Ok(operation)
    }

    pub fn create_managed_asset(
        &self,
        ctx: &TeamContext,
        knowledge_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
        now: u64,
    ) -> Result<String, TeamError> {
        if now > i64::MAX as u64
            || bytes.is_empty()
            || bytes.len() > MAX_MANAGED_ASSET_BYTES
            || !valid_filename(filename)
            || !valid_content_type(content_type)
        {
            return Err(TeamError::InvalidInput);
        }
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ctx.authorize_in_conn(&tx, now)?;
        knowledge_access_in_conn(&tx, ctx, knowledge_id, Action::Edit, now)?;
        let id = Uuid::new_v4().to_string();
        let hash = sha256(bytes);
        tx.execute(
            "INSERT INTO team_managed_asset(company_id,id,knowledge_id,owner_user_id,filename,content_type,content_hash,byte_len,bytes,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                self.company_id(),
                id,
                knowledge_id,
                ctx.user_id(),
                filename,
                content_type,
                hash,
                bytes.len() as i64,
                bytes,
                now
            ],
        )?;
        tx.execute(
            "INSERT INTO team_audit_event(id,company_id,actor_user_id,action,resource_id,occurred_at)
             VALUES (?1,?2,?3,'managed_asset_created',?4,?5)",
            params![Uuid::new_v4().to_string(), self.company_id(), ctx.user_id(), id, now],
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn managed_asset(
        &self,
        ctx: &TeamContext,
        knowledge_id: &str,
        asset_id: &str,
        now: u64,
    ) -> Result<ManagedAsset, TeamError> {
        if !valid_operation_id(asset_id) || now > i64::MAX as u64 {
            return Err(TeamError::InvalidInput);
        }
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        ctx.authorize_in_conn(&tx, now)?;
        knowledge_access_in_conn(&tx, ctx, knowledge_id, Action::Read, now)?;
        tx.query_row(
            "SELECT id,knowledge_id,filename,content_type,bytes FROM team_managed_asset
             WHERE company_id=?1 AND knowledge_id=?2 AND id=?3",
            params![self.company_id(), knowledge_id, asset_id],
            |r| {
                Ok(ManagedAsset {
                    id: r.get(0)?,
                    knowledge_id: r.get(1)?,
                    filename: r.get(2)?,
                    content_type: r.get(3)?,
                    bytes: r.get(4)?,
                })
            },
        )
        .optional()?
        .ok_or(TeamError::NotFound)
    }

    fn claim_content_operation(&self, now: u64) -> Result<Option<ClaimedOperation>, TeamError> {
        if now > i64::MAX as u64 {
            return Err(TeamError::InvalidInput);
        }
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let candidate: Option<String> = tx
            .query_row(
                "SELECT id FROM team_content_operation
                 WHERE company_id=?1 AND (
                    state='pending' OR (state IN ('applying','verifying') AND lease_expires_at<=?2)
                 ) ORDER BY created_at,id LIMIT 1",
                params![self.company_id(), now],
                |r| r.get(0),
            )
            .optional()?;
        let Some(id) = candidate else {
            tx.commit()?;
            return Ok(None);
        };
        let lease = Uuid::new_v4().to_string();
        let expires = now.checked_add(LEASE_SECONDS).ok_or(TeamError::Storage)?;
        let changed = tx.execute(
            "UPDATE team_content_operation SET state='applying',lease_token=?3,lease_expires_at=?4,
                 attempt_count=attempt_count+1,updated_at=?2
             WHERE company_id=?1 AND id=?5 AND (
                 state='pending' OR (state IN ('applying','verifying') AND lease_expires_at<=?2)
             )",
            params![self.company_id(), now, lease, expires, id],
        )?;
        if changed != 1 {
            tx.commit()?;
            return Ok(None);
        }
        let claimed = tx.query_row(
            "SELECT o.id,o.knowledge_id,o.owner_user_id,o.origin_session_id,o.kind,o.base_revision,o.directory_max_age,o.target_title,
                    o.target_markdown,o.target_hash,o.expected_remote_hash,o.target_doc_id,o.lease_token,o.attempt_count,p.category
             FROM team_content_operation o
             LEFT JOIN team_content_publish_target p ON p.company_id=o.company_id AND p.operation_id=o.id
             WHERE o.company_id=?1 AND o.id=?2",
            params![self.company_id(), id],
            |r| {
                let kind: String = r.get(4)?;
                Ok(ClaimedOperation {
                    id: r.get(0)?,
                    knowledge_id: r.get(1)?,
                    owner_user_id: r.get(2)?,
                    origin_session_id: r.get(3)?,
                    kind: if kind == "publish" {
                        ContentOperationKind::Publish
                    } else {
                        ContentOperationKind::Content
                    },
                    base_revision: r.get(5)?,
                    directory_max_age: r.get(6)?,
                    target_title: r.get(7)?,
                    target_markdown: r.get(8)?,
                    target_hash: r.get(9)?,
                    expected_remote_hash: r.get(10)?,
                    target_doc_id: r.get(11)?,
                    lease_token: r.get(12)?,
                    attempt_count: r.get(13)?,
                    target_category: r.get(14)?,
                })
            },
        )?;
        tx.commit()?;
        Ok(Some(claimed))
    }

    fn operation_owner_valid(&self, op: &ClaimedOperation, now: u64) -> Result<bool, TeamError> {
        if now > i64::MAX as u64 || !(1..=3600).contains(&op.directory_max_age) {
            return Ok(false);
        }
        let conn = self.db.conn();
        let row: Option<(u64, u64)> = conn
            .query_row(
                "SELECT o.content_revision,s.observed_at
                 FROM team_knowledge_owner o
                 JOIN team_user u ON u.company_id=o.company_id AND u.id=o.owner_user_id AND u.active=1
                 JOIN team_user_state us ON us.company_id=u.company_id AND us.user_id=u.id
                 JOIN team_company c ON c.id=o.company_id AND c.directory_generation=us.last_generation
                 JOIN team_org_snapshot s ON s.company_id=c.id AND s.generation=c.directory_generation
                 JOIN team_auth_session a ON a.id=?4 AND a.company_id=o.company_id
                      AND a.user_id=o.owner_user_id AND a.space_id=u.private_space_id
                      AND a.auth_version=us.auth_version AND a.revoked_at IS NULL
                      AND a.created_at<=?5 AND a.refresh_expires_at>?5
                 JOIN knowledge_item ki ON ki.id=o.knowledge_id AND ki.status='active'
                 WHERE o.company_id=?1 AND o.knowledge_id=?2 AND o.owner_user_id=?3",
                params![
                    self.company_id(),
                    op.knowledge_id,
                    op.owner_user_id,
                    op.origin_session_id,
                    now
                ],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(row.is_some_and(|(revision, observed)| {
            revision == op.base_revision
                && now
                    .checked_sub(observed)
                    .is_some_and(|age| age <= op.directory_max_age)
        }))
    }

    fn mark_operation_state(
        &self,
        op: &ClaimedOperation,
        state: &str,
        failure_code: Option<&str>,
        now: u64,
    ) -> Result<(), TeamError> {
        let conn = self.db.conn();
        conn.execute(
            "UPDATE team_content_operation SET state=?4,failure_code=?5,lease_token=NULL,
                 lease_expires_at=NULL,updated_at=?3
             WHERE company_id=?1 AND id=?2 AND lease_token=?6",
            params![
                self.company_id(),
                op.id,
                now,
                state,
                failure_code,
                op.lease_token
            ],
        )?;
        Ok(())
    }

    fn retry_or_fail_operation(&self, op: &ClaimedOperation, now: u64) -> Result<(), TeamError> {
        if op.attempt_count < MAX_ATTEMPTS {
            self.mark_operation_state(op, "pending", Some("dependency_unavailable"), now)
        } else {
            self.mark_operation_state(op, "failed", Some("dependency_unavailable"), now)
        }
    }

    fn set_operation_verifying(&self, op: &ClaimedOperation, now: u64) -> Result<(), TeamError> {
        let expires = now.checked_add(LEASE_SECONDS).ok_or(TeamError::Storage)?;
        let conn = self.db.conn();
        conn.execute(
            "UPDATE team_content_operation SET state='verifying',lease_expires_at=?4,updated_at=?3
             WHERE company_id=?1 AND id=?2 AND lease_token=?5",
            params![self.company_id(), op.id, now, expires, op.lease_token],
        )?;
        Ok(())
    }

    fn finalize_operation(
        &self,
        op: &ClaimedOperation,
        doc_id: Option<&str>,
        remote_hash: Option<&str>,
        now: u64,
    ) -> Result<u64, TeamError> {
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !worker_owner_valid_in_tx(&tx, self.company_id(), op, now)? {
            tx.execute(
                "UPDATE team_content_operation SET state='conflict',failure_code='authorization_changed',
                 lease_token=NULL,lease_expires_at=NULL,updated_at=?3
                 WHERE company_id=?1 AND id=?2 AND lease_token=?4",
                params![self.company_id(), op.id, now, op.lease_token],
            )?;
            tx.commit()?;
            return Err(TeamError::Conflict);
        }
        let next = op.base_revision.checked_add(1).ok_or(TeamError::Storage)?;
        let changed = tx.execute(
            "UPDATE team_knowledge_owner SET content_revision=?4
             WHERE company_id=?1 AND knowledge_id=?2 AND owner_user_id=?3 AND content_revision=?5",
            params![
                self.company_id(),
                op.knowledge_id,
                op.owner_user_id,
                next,
                op.base_revision
            ],
        )?;
        if changed != 1 {
            return Err(TeamError::Conflict);
        }
        let updated_at = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE knowledge_item SET title=?2,content=?3,managed_by='user',
                 siyuan_doc_id=COALESCE(?4,siyuan_doc_id),generated_hash=?5,
                 current_remote_hash=COALESCE(?6,current_remote_hash),migration_status=CASE WHEN COALESCE(?4,siyuan_doc_id) IS NULL THEN migration_status ELSE 'migrated' END,
                 index_status=CASE WHEN COALESCE(?4,siyuan_doc_id) IS NULL THEN 'pending' ELSE 'stale' END,
                 indexed_hash=NULL,indexed_at=NULL,embedding_model=NULL,embedding_dimensions=NULL,
                 index_chunk_count=0,last_index_error=NULL,updated_at=?7
             WHERE id=?1",
            params![
                op.knowledge_id,
                op.target_title,
                op.target_markdown,
                doc_id,
                op.target_hash,
                remote_hash,
                updated_at
            ],
        )?;
        tx.execute(
            "DELETE FROM embedding_record WHERE chunk_id IN (SELECT id FROM knowledge_chunk WHERE knowledge_id=?1)",
            [op.knowledge_id.as_str()],
        )?;
        tx.execute(
            "DELETE FROM knowledge_chunk WHERE knowledge_id=?1",
            [op.knowledge_id.as_str()],
        )?;
        tx.execute(
            "DELETE FROM knowledge_fts WHERE knowledge_id=?1",
            [op.knowledge_id.as_str()],
        )?;
        tx.execute(
            "INSERT INTO knowledge_fts(knowledge_id,title,summary,content,tags)
             SELECT id,title,summary,content,tags FROM knowledge_item WHERE id=?1",
            [op.knowledge_id.as_str()],
        )?;
        let done = tx.execute(
            "UPDATE team_content_operation SET state='done',result_revision=?4,target_doc_id=COALESCE(?5,target_doc_id),
                 failure_code=NULL,lease_token=NULL,lease_expires_at=NULL,updated_at=?3
             WHERE company_id=?1 AND id=?2 AND lease_token=?6",
            params![self.company_id(), op.id, now, next, doc_id, op.lease_token],
        )?;
        if done != 1 {
            return Err(TeamError::Conflict);
        }
        tx.execute(
            "INSERT INTO team_audit_event(id,company_id,actor_user_id,action,resource_id,occurred_at)
             VALUES (?1,?2,?3,'knowledge_content_committed',?4,?5)",
            params![Uuid::new_v4().to_string(), self.company_id(), op.owner_user_id, op.knowledge_id, now],
        )?;
        tx.commit()?;
        Ok(next)
    }
}

#[allow(clippy::too_many_arguments)]
fn enqueue_operation_in_tx(
    tx: &Transaction<'_>,
    company_id: &str,
    ctx: &TeamContext,
    knowledge_id: &str,
    operation_id: &str,
    base_revision: u64,
    kind: ContentOperationKind,
    title: &str,
    markdown: &str,
    current_revision: u64,
    target_doc_id: Option<&str>,
    expected_remote_hash: Option<&str>,
    target_category: Option<&str>,
    now: u64,
) -> Result<ContentOperation, TeamError> {
    let payload_hash = operation_payload_hash(kind, knowledge_id, base_revision, title, markdown);
    if let Some(existing) = read_operation_with_hash(tx, company_id, operation_id)? {
        if existing.0 != payload_hash
            || existing.1.knowledge_id != knowledge_id
            || existing.2 != ctx.user_id()
        {
            return Err(TeamError::Conflict);
        }
        return Ok(existing.1);
    }
    if current_revision != base_revision {
        return Err(TeamError::Conflict);
    }
    let open: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM team_content_operation WHERE company_id=?1 AND knowledge_id=?2
         AND state IN ('pending','applying','verifying'))",
        params![company_id, knowledge_id],
        |r| r.get(0),
    )?;
    if open {
        return Err(TeamError::Conflict);
    }
    let target_hash = sha256(markdown.as_bytes());
    tx.execute(
        "INSERT INTO team_content_operation(company_id,id,knowledge_id,owner_user_id,origin_session_id,kind,base_revision,
             directory_max_age,payload_hash,target_title,target_markdown,target_hash,expected_remote_hash,
             target_doc_id,state,created_at,updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,'pending',?15,?15)",
        params![
            company_id,
            operation_id,
            knowledge_id,
            ctx.user_id(),
            ctx.session_id(),
            kind.as_str(),
            base_revision,
            ctx.directory_max_age(),
            payload_hash,
            title,
            markdown,
            target_hash,
            expected_remote_hash,
            target_doc_id,
            now
        ],
    )?;
    if let Some(category) = target_category {
        tx.execute(
            "INSERT INTO team_content_publish_target(company_id,operation_id,category) VALUES (?1,?2,?3)",
            params![company_id, operation_id, category],
        )?;
    }
    tx.execute(
        "INSERT INTO team_audit_event(id,company_id,actor_user_id,action,resource_id,occurred_at)
         VALUES (?1,?2,?3,'knowledge_content_queued',?4,?5)",
        params![
            Uuid::new_v4().to_string(),
            company_id,
            ctx.user_id(),
            knowledge_id,
            now
        ],
    )?;
    Ok(ContentOperation {
        operation_id: operation_id.to_owned(),
        knowledge_id: knowledge_id.to_owned(),
        base_revision,
        result_revision: None,
        state: "pending".into(),
    })
}

fn read_operation_with_hash(
    tx: &Transaction<'_>,
    company: &str,
    id: &str,
) -> Result<Option<(String, ContentOperation, String)>, TeamError> {
    Ok(tx
        .query_row(
            "SELECT payload_hash,id,knowledge_id,base_revision,result_revision,state,owner_user_id
             FROM team_content_operation WHERE company_id=?1 AND id=?2",
            params![company, id],
            |r| {
                Ok((
                    r.get(0)?,
                    ContentOperation {
                        operation_id: r.get(1)?,
                        knowledge_id: r.get(2)?,
                        base_revision: r.get(3)?,
                        result_revision: r.get(4)?,
                        state: r.get(5)?,
                    },
                    r.get(6)?,
                ))
            },
        )
        .optional()?)
}

fn read_operation(
    tx: &Transaction<'_>,
    company: &str,
    id: &str,
) -> Result<Option<ContentOperation>, TeamError> {
    Ok(tx
        .query_row(
            "SELECT id,knowledge_id,base_revision,result_revision,state FROM team_content_operation
             WHERE company_id=?1 AND id=?2",
            params![company, id],
            |r| {
                Ok(ContentOperation {
                    operation_id: r.get(0)?,
                    knowledge_id: r.get(1)?,
                    base_revision: r.get(2)?,
                    result_revision: r.get(3)?,
                    state: r.get(4)?,
                })
            },
        )
        .optional()?)
}

fn worker_owner_valid_in_tx(
    tx: &Transaction<'_>,
    company: &str,
    op: &ClaimedOperation,
    now: u64,
) -> Result<bool, TeamError> {
    let row: Option<(u64, u64)> = tx
        .query_row(
            "SELECT o.content_revision,s.observed_at
             FROM team_knowledge_owner o
             JOIN team_user u ON u.company_id=o.company_id AND u.id=o.owner_user_id AND u.active=1
             JOIN team_user_state us ON us.company_id=u.company_id AND us.user_id=u.id
             JOIN team_company c ON c.id=o.company_id AND c.directory_generation=us.last_generation
             JOIN team_org_snapshot s ON s.company_id=c.id AND s.generation=c.directory_generation
             JOIN team_auth_session a ON a.id=?4 AND a.company_id=o.company_id
                  AND a.user_id=o.owner_user_id AND a.space_id=u.private_space_id
                  AND a.auth_version=us.auth_version AND a.revoked_at IS NULL
                  AND a.created_at<=?5 AND a.refresh_expires_at>?5
             JOIN knowledge_item ki ON ki.id=o.knowledge_id AND ki.status='active'
             WHERE o.company_id=?1 AND o.knowledge_id=?2 AND o.owner_user_id=?3",
            params![
                company,
                op.knowledge_id,
                op.owner_user_id,
                op.origin_session_id,
                now
            ],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.is_some_and(|(revision, observed)| {
        revision == op.base_revision
            && now
                .checked_sub(observed)
                .is_some_and(|age| age <= op.directory_max_age)
    }))
}

pub(super) fn validate_title(value: &str) -> Result<(), TeamError> {
    if value.trim().is_empty()
        || value.len() > 4096
        || value.contains('\0')
        || value
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    {
        return Err(TeamError::InvalidInput);
    }
    Ok(())
}

pub(super) fn validate_markdown(value: &str) -> Result<(), TeamError> {
    if value.len() > MAX_TEAM_CONTENT_BYTES || value.contains('\0') {
        return Err(TeamError::InvalidInput);
    }
    Ok(())
}

pub(super) fn valid_operation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.trim() == value
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn valid_filename(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value.trim() == value
        && !value.chars().any(|ch| matches!(ch, '/' | '\\' | '\0'))
        && !value.chars().any(char::is_control)
}

fn valid_content_type(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    !value.is_empty()
        && value.len() <= 128
        && !value.chars().any(|ch| matches!(ch, '\r' | '\n' | '\0'))
        && !matches!(
            value.as_str(),
            "text/html" | "image/svg+xml" | "application/xhtml+xml"
        )
        && (value.starts_with("image/")
            || value.starts_with("audio/")
            || value.starts_with("video/")
            || matches!(
                value.as_str(),
                "application/pdf"
                    | "application/octet-stream"
                    | "text/plain"
                    | "text/markdown"
                    | "application/json"
            ))
}

fn operation_payload_hash(
    kind: ContentOperationKind,
    knowledge_id: &str,
    base_revision: u64,
    title: &str,
    markdown: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_str().as_bytes());
    hasher.update([0]);
    hasher.update(knowledge_id.as_bytes());
    hasher.update([0]);
    hasher.update(base_revision.to_be_bytes());
    hasher.update([0]);
    hasher.update(title.as_bytes());
    hasher.update([0]);
    hasher.update(markdown.as_bytes());
    hex::encode(hasher.finalize())
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn markdown_matches(left: &str, right: &str) -> bool {
    left.trim_end_matches(['\r', '\n']) == right.trim_end_matches(['\r', '\n'])
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Serial durable worker. The wake channel is capacity one and a periodic poll
/// recovers committed intents after restart even if the original process died.
pub struct ContentWorker {
    wake_tx: mpsc::Sender<()>,
    stop: watch::Sender<bool>,
    supervisor: Mutex<Option<JoinHandle<()>>>,
}

impl ContentWorker {
    pub fn start(store: Arc<TeamStore>, sink: Arc<SiYuanSink>, models: Arc<ModelService>) -> Self {
        let (wake_tx, wake_rx) = mpsc::channel(1);
        let (stop, stop_rx) = watch::channel(false);
        let task = tokio::spawn(run_content_worker(store, sink, models, wake_rx, stop_rx));
        let worker = Self {
            wake_tx,
            stop,
            supervisor: Mutex::new(Some(task)),
        };
        worker.wake();
        worker
    }

    pub fn wake(&self) {
        let _ = self.wake_tx.try_send(());
    }

    pub async fn shutdown(&self, grace: Duration) {
        let _ = self.stop.send(true);
        let mut guard = self.supervisor.lock().await;
        if let Some(mut handle) = guard.take() {
            if tokio::time::timeout(grace, &mut handle).await.is_err() {
                handle.abort();
                let _ = handle.await;
            }
        }
    }
}

impl Drop for ContentWorker {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

async fn run_content_worker(
    store: Arc<TeamStore>,
    sink: Arc<SiYuanSink>,
    models: Arc<ModelService>,
    mut wake_rx: mpsc::Receiver<()>,
    mut stop_rx: watch::Receiver<bool>,
) {
    let mut poll = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            _ = poll.tick() => {},
            signal = wake_rx.recv() => { if signal.is_none() { return; } },
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow() { return; }
            }
        }
        loop {
            if *stop_rx.borrow() {
                return;
            }
            let now = unix_now();
            let claimed = match store.claim_content_operation(now) {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(error=%error, "team content claim failed");
                    break;
                }
            };
            let Some(op) = claimed else {
                break;
            };
            if let Err(error) = process_operation(&store, &sink, models.clone(), &op).await {
                tracing::warn!(operation_id=%op.id, error=%error, "team content operation stopped");
            }
        }
    }
}

async fn process_operation(
    store: &Arc<TeamStore>,
    sink: &Arc<SiYuanSink>,
    models: Arc<ModelService>,
    op: &ClaimedOperation,
) -> Result<(), TeamError> {
    let now = unix_now();
    if !store.operation_owner_valid(op, now)? {
        store.mark_operation_state(op, "conflict", Some("authorization_changed"), now)?;
        return Ok(());
    }

    let result = match op.kind {
        ContentOperationKind::Publish => publish_operation(sink, store, op).await,
        ContentOperationKind::Content => update_operation(sink, store, op).await,
    };
    let (doc_id, remote_markdown) = match result {
        Ok(value) => value,
        Err(ProcessError::Conflict) => {
            store.mark_operation_state(op, "conflict", Some("remote_changed"), unix_now())?;
            return Ok(());
        }
        Err(ProcessError::Unavailable) => {
            store.retry_or_fail_operation(op, unix_now())?;
            return Ok(());
        }
    };
    let remote_hash = remote_markdown
        .as_ref()
        .map(|value| sha256(value.as_bytes()));
    let revision =
        store.finalize_operation(op, doc_id.as_deref(), remote_hash.as_deref(), unix_now())?;
    let _ = revision;
    if let (Some(doc_id), Some(markdown)) = (doc_id, remote_markdown) {
        let index = KnowledgeIndexService::new(store.db().clone(), models);
        if let Err(error) = index
            .index_document(KnowledgeIndexInput {
                knowledge_id: op.knowledge_id.clone(),
                siyuan_doc_id: doc_id,
                markdown,
            })
            .await
        {
            tracing::warn!(knowledge_id=%op.knowledge_id, error=%error, "team content reindex failed");
        }
    }
    Ok(())
}

#[derive(Debug)]
enum ProcessError {
    Conflict,
    Unavailable,
}

async fn update_operation(
    sink: &Arc<SiYuanSink>,
    store: &Arc<TeamStore>,
    op: &ClaimedOperation,
) -> Result<(Option<String>, Option<String>), ProcessError> {
    let Some(doc_id) = op.target_doc_id.as_deref() else {
        // Draft edits are committed from the durable intent without external I/O.
        return Ok((None, None));
    };
    let expected = op
        .expected_remote_hash
        .as_deref()
        .ok_or(ProcessError::Conflict)?;
    let current = sink
        .get_document_markdown_bounded(doc_id, MAX_TEAM_CONTENT_BYTES)
        .await
        .map_err(|_| ProcessError::Unavailable)?;
    if markdown_matches(&current, &op.target_markdown) {
        return Ok((Some(doc_id.to_owned()), Some(current)));
    }
    if sha256(current.as_bytes()) != expected {
        return Err(ProcessError::Conflict);
    }
    if !store
        .operation_owner_valid(op, unix_now())
        .map_err(|_| ProcessError::Unavailable)?
    {
        return Err(ProcessError::Conflict);
    }
    store
        .set_operation_verifying(op, unix_now())
        .map_err(|_| ProcessError::Unavailable)?;
    let write = sink.update_document(doc_id, &op.target_markdown).await;
    let after = sink
        .get_document_markdown_bounded(doc_id, MAX_TEAM_CONTENT_BYTES)
        .await;
    match after {
        Ok(markdown) if markdown_matches(&markdown, &op.target_markdown) => {
            Ok((Some(doc_id.to_owned()), Some(markdown)))
        }
        Ok(markdown) if sha256(markdown.as_bytes()) != expected => Err(ProcessError::Conflict),
        Ok(_) if write.is_err() => Err(ProcessError::Unavailable),
        Ok(_) => Err(ProcessError::Conflict),
        Err(_) => Err(ProcessError::Unavailable),
    }
}

async fn publish_operation(
    sink: &Arc<SiYuanSink>,
    store: &Arc<TeamStore>,
    op: &ClaimedOperation,
) -> Result<(Option<String>, Option<String>), ProcessError> {
    if !store
        .operation_owner_valid(op, unix_now())
        .map_err(|_| ProcessError::Unavailable)?
    {
        return Err(ProcessError::Conflict);
    }
    let category = op
        .target_category
        .as_deref()
        .ok_or(ProcessError::Conflict)?;
    let notebook = sink
        .ensure_notebook()
        .await
        .map_err(|_| ProcessError::Unavailable)?;
    let path = sink.build_knowledge_path(category, &op.knowledge_id, &op.target_title);
    store
        .set_operation_verifying(op, unix_now())
        .map_err(|_| ProcessError::Unavailable)?;
    let doc_id = sink
        .create_document_reconciled(&notebook, &path, &op.target_markdown)
        .await
        .map_err(|_| ProcessError::Unavailable)?;
    let markdown = sink
        .get_document_markdown_bounded(&doc_id, MAX_TEAM_CONTENT_BYTES)
        .await
        .map_err(|_| ProcessError::Unavailable)?;
    if !markdown_matches(&markdown, &op.target_markdown) {
        return Err(ProcessError::Conflict);
    }
    // Attributes are advisory metadata, never authorization. Failure is retried
    // because a later pass safely reconciles the same deterministic document.
    sink.set_knowledge_attrs(&doc_id, &op.knowledge_id, "team", &op.target_hash, category)
        .await
        .map_err(|_| ProcessError::Unavailable)?;
    Ok((Some(doc_id), Some(markdown)))
}
