//! Authenticated, private-by-default imports from a local personal knowledge item.
use chrono::Utc;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    content::{valid_operation_id, validate_markdown, validate_title},
    TeamContext, TeamError, TeamStore,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ImportReceipt {
    pub operation_id: String,
    pub knowledge_id: String,
    pub content_revision: u64,
}

impl TeamStore {
    pub fn import_knowledge(
        &self,
        ctx: &TeamContext,
        operation_id: &str,
        title: &str,
        markdown: &str,
        source_fingerprint: &str,
        now: u64,
    ) -> Result<(ImportReceipt, bool), TeamError> {
        if !valid_operation_id(operation_id)
            || !valid_source_fingerprint(source_fingerprint)
            || now > i64::MAX as u64
        {
            return Err(TeamError::InvalidInput);
        }
        validate_title(title)?;
        validate_markdown(markdown)?;
        let payload_hash = payload_hash(source_fingerprint, title, markdown);

        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ctx.authorize_in_conn(&tx, now)?;
        if ctx.company_id() != self.company_id() {
            return Err(TeamError::Unauthorized);
        }

        if let Some((stored_source, stored_hash, knowledge_id)) = tx
            .query_row(
                "SELECT source_fingerprint,payload_hash,knowledge_id
                 FROM team_knowledge_import
                 WHERE company_id=?1 AND owner_user_id=?2 AND operation_id=?3",
                params![self.company_id(), ctx.user_id(), operation_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
            )
            .optional()?
        {
            if stored_source != source_fingerprint || stored_hash != payload_hash {
                return Err(TeamError::Conflict);
            }
            return Ok((receipt(operation_id, knowledge_id), false));
        }

        if let Some((original_operation, stored_hash, knowledge_id)) = tx
            .query_row(
                "SELECT operation_id,payload_hash,knowledge_id
                 FROM team_knowledge_import
                 WHERE company_id=?1 AND owner_user_id=?2 AND source_fingerprint=?3",
                params![self.company_id(), ctx.user_id(), source_fingerprint],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
            )
            .optional()?
        {
            if stored_hash != payload_hash {
                return Err(TeamError::Conflict);
            }
            return Ok((receipt(&original_operation, knowledge_id), false));
        }

        let knowledge_id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO knowledge_item
             (id,source_session_id,project_name,title,category,summary,content,tags,
              confidence,worth_extracting,source_type,managed_by,status,is_favorite,
              migration_status,index_status,created_at,updated_at)
             VALUES (?1,NULL,NULL,?2,'general','',?3,'[]',1.0,1,'manual','user',
                     'active',0,'pending','pending',?4,?4)",
            params![knowledge_id, title, markdown, created_at],
        )?;
        tx.execute(
            "INSERT INTO team_knowledge_owner(company_id,knowledge_id,owner_user_id,content_revision,grant_version)
             VALUES (?1,?2,?3,1,0)",
            params![self.company_id(), knowledge_id, ctx.user_id()],
        )?;
        tx.execute(
            "INSERT INTO knowledge_fts(knowledge_id,title,summary,content,tags)
             VALUES (?1,?2,'',?3,'[]')",
            params![knowledge_id, title, markdown],
        )?;
        tx.execute(
            "INSERT INTO team_knowledge_import
             (company_id,owner_user_id,operation_id,source_fingerprint,payload_hash,knowledge_id,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                self.company_id(),
                ctx.user_id(),
                operation_id,
                source_fingerprint,
                payload_hash,
                knowledge_id,
                now
            ],
        )?;
        tx.execute(
            "INSERT INTO team_audit_event(id,company_id,actor_user_id,action,resource_id,occurred_at)
             VALUES (?1,?2,?3,'knowledge_imported',?4,?5)",
            params![
                Uuid::new_v4().to_string(),
                self.company_id(),
                ctx.user_id(),
                knowledge_id,
                now
            ],
        )?;
        tx.commit()?;
        Ok((receipt(operation_id, knowledge_id), true))
    }
}

fn receipt(operation_id: &str, knowledge_id: String) -> ImportReceipt {
    ImportReceipt {
        operation_id: operation_id.to_owned(),
        knowledge_id,
        content_revision: 1,
    }
}

fn valid_source_fingerprint(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn payload_hash(source_fingerprint: &str, title: &str, markdown: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [source_fingerprint, title, markdown] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    hex::encode(hasher.finalize())
}
