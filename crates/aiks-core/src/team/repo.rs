use std::sync::Arc;

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

use crate::storage::StateDb;

use super::TeamError;

/// Bound to one company for its entire lifetime. This is NOT an authenticated
/// user context and cannot be supplied by an HTTP caller to impersonate a user.
pub struct TeamStore {
    pub(super) db: Arc<StateDb>,
    company_id: String,
    instance_id: String,
    corp_id: String,
    client_id: String,
}

impl TeamStore {
    pub fn bind(db: Arc<StateDb>, corp_id: &str, client_id: &str) -> Result<Self, TeamError> {
        if !db.has_exclusive_lease() {
            return Err(TeamError::Storage);
        }
        if !valid_external_id(corp_id) || !valid_external_id(client_id) {
            return Err(TeamError::ConfigInvalid);
        }
        let (company_id, instance_id) = {
            let mut conn = db.conn();
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let personal: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM service_instance)", [], |row| row.get(0),
            )?;
            if personal { return Err(TeamError::ConfigInvalid); }
            let existing: Option<(String, String, String, String)> = tx.query_row(
                "SELECT id,instance_id,corp_id,client_id FROM team_company WHERE singleton=1", [],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
            ).optional()?;
            let identity = match existing {
                Some((company, instance, stored_corp, stored_client)) => {
                    if stored_corp != corp_id || stored_client != client_id {
                        return Err(TeamError::ConfigInvalid);
                    }
                    (company, instance)
                }
                None => {
                    let legacy: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM source_session)
                         OR EXISTS(SELECT 1 FROM knowledge_item)
                         OR EXISTS(SELECT 1 FROM service_source_registration)
                         OR EXISTS(SELECT 1 FROM pipeline_job)
                         OR EXISTS(SELECT 1 FROM pipeline_run)", [], |row| row.get(0),
                    )?;
                    if legacy { return Err(TeamError::ConfigInvalid); }
                    let company = Uuid::new_v4().to_string();
                    let instance = Uuid::new_v4().to_string();
                    tx.execute(
                        "INSERT INTO team_company(singleton,id,instance_id,corp_id,client_id,created_at)
                         VALUES (1,?1,?2,?3,?4,?5)",
                        params![company,instance,corp_id,client_id,chrono::Utc::now().timestamp().max(0)],
                    )?;
                    (company, instance)
                }
            };
            tx.commit()?;
            identity
        };
        Ok(Self { db, company_id, instance_id, corp_id: corp_id.into(), client_id: client_id.into() })
    }

    pub fn company_id(&self) -> &str { &self.company_id }
    pub fn instance_id(&self) -> &str { &self.instance_id }
    pub fn corp_id(&self) -> &str { &self.corp_id }
    pub fn client_id(&self) -> &str { &self.client_id }
    pub fn db(&self) -> &Arc<StateDb> { &self.db }
}

fn valid_external_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}
