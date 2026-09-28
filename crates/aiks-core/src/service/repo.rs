use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

use crate::{storage::StateDb, team::TeamContext};

use super::{validation::validate_identifier, ServiceError};

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

/// Verified remote-collector identity. The HTTP layer derives this only after
/// validating the caller against WeKnora; raw client-supplied owner IDs never
/// become a CollectorContext.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectorContext {
    instance_id: String,
    principal_id: String,
    space_id: String,
}

impl CollectorContext {
    pub fn verified(
        instance_id: &str,
        principal_id: &str,
        space_id: &str,
    ) -> Result<Self, ServiceError> {
        validate_identifier(instance_id)?;
        validate_identifier(principal_id)?;
        validate_identifier(space_id)?;
        Ok(Self {
            instance_id: instance_id.to_owned(),
            principal_id: principal_id.to_owned(),
            space_id: space_id.to_owned(),
        })
    }

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

/// Server-issued request identity. Neither variant is deserialized from an HTTP
/// payload. Collector identities are constructed only after upstream WeKnora
/// authentication succeeds.
#[derive(Clone, Debug)]
pub enum RequestContext {
    Personal(LocalContext),
    Collector(CollectorContext),
    Team(TeamContext),
}

impl RequestContext {
    pub fn instance_id(&self) -> &str {
        match self {
            Self::Personal(ctx) => ctx.instance_id(),
            Self::Collector(ctx) => ctx.instance_id(),
            Self::Team(ctx) => ctx.instance_id(),
        }
    }

    pub fn principal_id(&self) -> &str {
        match self {
            Self::Personal(ctx) => ctx.principal_id(),
            Self::Collector(ctx) => ctx.principal_id(),
            Self::Team(ctx) => ctx.user_id(),
        }
    }

    pub fn space_id(&self) -> &str {
        match self {
            Self::Personal(ctx) => ctx.space_id(),
            Self::Collector(ctx) => ctx.space_id(),
            Self::Team(ctx) => ctx.space_id(),
        }
    }

    pub fn company_id(&self) -> Option<&str> {
        match self {
            Self::Personal(_) | Self::Collector(_) => None,
            Self::Team(ctx) => Some(ctx.company_id()),
        }
    }

    pub fn is_team(&self) -> bool {
        matches!(self, Self::Team(_))
    }

    pub fn is_collector(&self) -> bool {
        matches!(self, Self::Collector(_))
    }

    pub(crate) fn team(&self) -> Option<&TeamContext> {
        match self {
            Self::Personal(_) | Self::Collector(_) => None,
            Self::Team(ctx) => Some(ctx),
        }
    }

    /// Team requests are revalidated against session revocation, directory
    /// generation and freshness in the caller's transaction. Personal mode is
    /// validated against its runtime identity before entering the DB operation.
    pub(crate) fn authorize_in_conn(
        &self,
        conn: &Connection,
        now: u64,
    ) -> Result<(), ServiceError> {
        if let Self::Team(ctx) = self {
            ctx.authorize_in_conn(conn, now)?;
        }
        Ok(())
    }
}

impl From<LocalContext> for RequestContext {
    fn from(value: LocalContext) -> Self {
        Self::Personal(value)
    }
}

impl From<CollectorContext> for RequestContext {
    fn from(value: CollectorContext) -> Self {
        Self::Collector(value)
    }
}

impl From<TeamContext> for RequestContext {
    fn from(value: TeamContext) -> Self {
        Self::Team(value)
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
            let company: bool = tx
                .query_row("SELECT EXISTS(SELECT 1 FROM team_company)", [], |row| {
                    row.get(0)
                })
                .map_err(|_| ServiceError::Internal)?;
            if company {
                return Err(ServiceError::Unavailable);
            }
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
