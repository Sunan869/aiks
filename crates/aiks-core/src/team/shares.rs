//! Owner-only knowledge authorization and read-only person/organization grants.
use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use uuid::Uuid;

use super::{policy, Action, TeamContext, TeamError, TeamStore};

const MAX_GRANTS_PER_DOCUMENT: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrantTarget {
    User(String),
    Org { id: String, descendants: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantInput {
    pub target: GrantTarget,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ShareGrant {
    pub target_type: &'static str,
    pub target_id: String,
    pub include_descendants: bool,
    pub permission: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ShareState {
    pub grant_version: u64,
    pub grants: Vec<ShareGrant>,
}

impl TeamStore {
    pub fn knowledge_access(
        &self,
        ctx: &TeamContext,
        knowledge_id: &str,
        action: Action,
        now: u64,
    ) -> Result<(), TeamError> {
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        knowledge_access_in_conn(&tx, ctx, knowledge_id, action, now)
    }

    pub fn list_grants(
        &self,
        ctx: &TeamContext,
        knowledge_id: &str,
        now: u64,
    ) -> Result<ShareState, TeamError> {
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        knowledge_access_in_conn(&tx, ctx, knowledge_id, Action::ManageShares, now)?;
        read_grants_in_tx(&tx, self.company_id(), knowledge_id)
    }

    pub fn replace_grants(
        &self,
        ctx: &TeamContext,
        knowledge_id: &str,
        expected: u64,
        grants: &[GrantInput],
        now: u64,
    ) -> Result<u64, TeamError> {
        if grants.len() > MAX_GRANTS_PER_DOCUMENT || now > i64::MAX as u64 {
            return Err(TeamError::InvalidInput);
        }
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        knowledge_access_in_conn(&tx, ctx, knowledge_id, Action::ManageShares, now)?;
        let current: u64 = tx.query_row(
            "SELECT grant_version FROM team_knowledge_owner WHERE company_id=?1 AND knowledge_id=?2",
            params![self.company_id(), knowledge_id],
            |row| row.get(0),
        )?;
        if current != expected {
            return Err(TeamError::Conflict);
        }

        let generation: u64 = tx.query_row(
            "SELECT directory_generation FROM team_company WHERE id=?1",
            [self.company_id()],
            |row| row.get(0),
        )?;
        let mut canonical = BTreeMap::<String, GrantInput>::new();
        for grant in grants {
            match &grant.target {
                GrantTarget::User(id) => {
                    if !valid_id(id) || !active_user(&tx, self.company_id(), id, generation)? {
                        return Err(TeamError::InvalidInput);
                    }
                    canonical.insert(format!("user:{id}"), grant.clone());
                }
                GrantTarget::Org { id, descendants } => {
                    if !valid_id(id) || !current_org(&tx, self.company_id(), id, generation)? {
                        return Err(TeamError::InvalidInput);
                    }
                    canonical.insert(
                        format!("org:{id}:{}", u8::from(*descendants)),
                        grant.clone(),
                    );
                }
            }
        }

        tx.execute(
            "DELETE FROM document_share_grant WHERE company_id=?1 AND knowledge_id=?2",
            params![self.company_id(), knowledge_id],
        )?;
        for grant in canonical.into_values() {
            match grant.target {
                GrantTarget::User(id) => {
                    tx.execute(
                        "INSERT INTO document_share_grant(company_id,id,knowledge_id,target_user_id,include_descendants,permission) VALUES (?1,?2,?3,?4,0,'read')",
                        params![self.company_id(), Uuid::new_v4().to_string(), knowledge_id, id],
                    )?;
                }
                GrantTarget::Org { id, descendants } => {
                    tx.execute(
                        "INSERT INTO document_share_grant(company_id,id,knowledge_id,target_org_id,include_descendants,permission) VALUES (?1,?2,?3,?4,?5,'read')",
                        params![self.company_id(), Uuid::new_v4().to_string(), knowledge_id, id, descendants],
                    )?;
                }
            }
        }
        let next = current.checked_add(1).ok_or(TeamError::Storage)?;
        let changed = tx.execute(
            "UPDATE team_knowledge_owner SET grant_version=?3 WHERE company_id=?1 AND knowledge_id=?2 AND grant_version=?4",
            params![self.company_id(), knowledge_id, next, current],
        )?;
        if changed != 1 {
            return Err(TeamError::Conflict);
        }
        tx.execute(
            "INSERT INTO team_audit_event(id,company_id,actor_user_id,action,resource_id,occurred_at) VALUES (?1,?2,?3,'knowledge_shares_replaced',?4,?5)",
            params![Uuid::new_v4().to_string(), self.company_id(), ctx.user_id(), knowledge_id, now],
        )?;
        tx.commit()?;
        Ok(next)
    }
}

pub(crate) fn knowledge_access_in_conn(
    conn: &Connection,
    ctx: &TeamContext,
    knowledge_id: &str,
    action: Action,
    now: u64,
) -> Result<(), TeamError> {
    ctx.authorize_in_conn(conn, now)?;
    let access: Option<(bool, bool, bool)> = conn
        .query_row(
            "SELECT o.owner_user_id=?3,
                    EXISTS(SELECT 1 FROM document_share_grant g
                           WHERE g.company_id=o.company_id AND g.knowledge_id=o.knowledge_id AND g.target_user_id=?3),
                    EXISTS(SELECT 1 FROM document_share_grant g
                           JOIN team_company c ON c.id=g.company_id
                           JOIN team_org_membership m ON m.company_id=g.company_id
                                AND m.generation=c.directory_generation AND m.user_id=?3
                           WHERE g.company_id=o.company_id AND g.knowledge_id=o.knowledge_id
                             AND g.target_org_id IS NOT NULL
                             AND (m.org_id=g.target_org_id OR (g.include_descendants=1 AND EXISTS(
                                 SELECT 1 FROM team_org_closure oc
                                 WHERE oc.company_id=m.company_id AND oc.generation=m.generation
                                   AND oc.ancestor_id=g.target_org_id AND oc.descendant_id=m.org_id))))
             FROM team_knowledge_owner o
             JOIN knowledge_item ki ON ki.id=o.knowledge_id AND ki.status='active'
             WHERE o.company_id=?1 AND o.knowledge_id=?2",
            params![ctx.company_id(), knowledge_id, ctx.user_id()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((owner, direct, org)) = access else {
        return Err(TeamError::NotFound);
    };
    if policy::allows(action, true, true, true, owner, direct, org) {
        Ok(())
    } else if matches!(action, Action::Read) {
        Err(TeamError::NotFound)
    } else {
        Err(TeamError::Forbidden)
    }
}

fn read_grants_in_tx(
    tx: &Transaction<'_>,
    company_id: &str,
    knowledge_id: &str,
) -> Result<ShareState, TeamError> {
    let version: u64 = tx.query_row(
        "SELECT grant_version FROM team_knowledge_owner WHERE company_id=?1 AND knowledge_id=?2",
        params![company_id, knowledge_id],
        |row| row.get(0),
    )?;
    let mut stmt = tx.prepare(
        "SELECT target_user_id,target_org_id,include_descendants
         FROM document_share_grant WHERE company_id=?1 AND knowledge_id=?2
         ORDER BY CASE WHEN target_user_id IS NOT NULL THEN 0 ELSE 1 END,
                  COALESCE(target_user_id,target_org_id),include_descendants",
    )?;
    let grants = stmt
        .query_map(params![company_id, knowledge_id], |row| {
            let user: Option<String> = row.get(0)?;
            let org: Option<String> = row.get(1)?;
            let descendants: bool = row.get(2)?;
            Ok(if let Some(id) = user {
                ShareGrant {
                    target_type: "user",
                    target_id: id,
                    include_descendants: false,
                    permission: "read",
                }
            } else {
                ShareGrant {
                    target_type: "org",
                    target_id: org.unwrap_or_default(),
                    include_descendants: descendants,
                    permission: "read",
                }
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ShareState {
        grant_version: version,
        grants,
    })
}

fn active_user(
    tx: &Transaction<'_>,
    company: &str,
    user: &str,
    generation: u64,
) -> Result<bool, TeamError> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM team_user u JOIN team_user_state s
         ON s.company_id=u.company_id AND s.user_id=u.id
         WHERE u.company_id=?1 AND u.id=?2 AND u.active=1 AND s.last_generation=?3)",
        params![company, user, generation],
        |row| row.get(0),
    )?)
}

fn current_org(
    tx: &Transaction<'_>,
    company: &str,
    org: &str,
    generation: u64,
) -> Result<bool, TeamError> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM team_org_unit WHERE company_id=?1 AND org_id=?2 AND generation=?3)",
        params![company, org, generation],
        |row| row.get(0),
    )?)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
