use std::sync::Arc;

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

use crate::storage::StateDb;

use super::ServiceError;

/// Created only from the persistent service identity, never deserialized from
/// client-supplied owner IDs or forwarding headers. Not a team ACL yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContext {
    instance_id: String,
    principal_id: String,
    space_id: String,
}

impl LocalContext {
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn principal_id(&self) -> &str {
        &self.principal_id
    }

    pub fn space_id(&self) -> &str {
        &self.space_id
    }
}

pub struct ServiceStore {
    db: Arc<StateDb>,
    context: LocalContext,
}

impl ServiceStore {
    pub fn open(db: Arc<StateDb>) -> Result<Self, ServiceError> {
        if !db.has_exclusive_lease() {
            return Err(ServiceError::Unavailable);
        }
        let context = {
            let mut conn = db.conn();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|_| ServiceError::Internal)?;
            let existing = tx
                .query_row(
                    "SELECT instance_id, principal_id, personal_space_id
                     FROM service_instance WHERE singleton=1",
                    [],
                    |row| {
                        Ok(LocalContext {
                            instance_id: row.get(0)?,
                            principal_id: row.get(1)?,
                            space_id: row.get(2)?,
                        })
                    },
                )
                .optional()
                .map_err(|_| ServiceError::Internal)?;
            let context = match existing {
                Some(context) => context,
                None => {
                    let context = LocalContext {
                        instance_id: Uuid::new_v4().to_string(),
                        principal_id: Uuid::new_v4().to_string(),
                        space_id: Uuid::new_v4().to_string(),
                    };
                    tx.execute(
                        "INSERT INTO service_instance
                         (singleton, instance_id, principal_id, personal_space_id)
                         VALUES (1, ?1, ?2, ?3)",
                        params![context.instance_id, context.principal_id, context.space_id],
                    )
                    .map_err(|_| ServiceError::Internal)?;
                    context
                }
            };
            tx.commit().map_err(|_| ServiceError::Internal)?;
            context
        };
        Ok(Self { db, context })
    }

    pub fn local_context(&self) -> LocalContext {
        self.context.clone()
    }

    pub fn db(&self) -> &Arc<StateDb> {
        &self.db
    }
}
