//! Atomic directory publication. No upstream I/O occurs while SQLite is locked.
use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

use super::{directory_validate, DirectorySnapshot, TeamError, TeamStore, UserRecord};

impl TeamStore {
    pub fn publish_directory(&self, input: DirectorySnapshot, now: u64) -> Result<u64, TeamError> {
        let closure = directory_validate::validate(&input, now)?;
        let company = self.company_id();
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: u64 = tx.query_row(
            "SELECT directory_generation FROM team_company WHERE id=?1",
            [company],
            |row| row.get(0),
        )?;
        let last: Option<u64> = tx
            .query_row(
                "SELECT observed_at FROM team_org_snapshot WHERE company_id=?1 AND generation=?2",
                params![company, current],
                |row| row.get(0),
            )
            .optional()?;
        let event: u64 = tx.query_row(
            "SELECT COALESCE(MAX(inactive_event_at),0) FROM team_user_state WHERE company_id=?1",
            [company],
            |row| row.get(0),
        )?;
        if last.is_some_and(|time| input.observed_at < time)
            || event > 0 && input.observed_at <= event
        {
            return Err(TeamError::Conflict);
        }
        let generation = current
            .checked_add(1)
            .filter(|g| *g <= i64::MAX as u64)
            .ok_or(TeamError::Storage)?;
        let scope = serde_json::to_string(&input.scope).map_err(|_| TeamError::InvalidInput)?;
        tx.execute(
            "INSERT INTO team_org_snapshot(company_id,generation,scope_json,observed_at,published_at)
             VALUES (?1,?2,?3,?4,?5)", params![company,generation,scope,input.observed_at,now],
        )?;
        let mut users = HashMap::new();
        for user in &input.users {
            let existing: Option<(String,String,bool)> = tx.query_row(
                "SELECT id,external_user_id,active FROM team_user WHERE company_id=?1 AND union_id=?2",
                params![company,user.union_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
            ).optional()?;
            let id = match existing {
                Some((id, external, active)) => {
                    if external != user.external_user_id {
                        return Err(TeamError::Conflict);
                    }
                    if active && !user.active {
                        deactivate(&tx, company, &id, now)?;
                    }
                    tx.execute("UPDATE team_user SET display_name=?3,active=?4 WHERE company_id=?1 AND id=?2",
                        params![company,id,user.display_name,user.active])?;
                    id
                }
                None => {
                    let occupied: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM team_user WHERE company_id=?1 AND external_user_id=?2)",
                        params![company,user.external_user_id], |row| row.get(0),
                    )?;
                    if occupied {
                        return Err(TeamError::Conflict);
                    }
                    let id = Uuid::new_v4().to_string();
                    let space = Uuid::new_v4().to_string();
                    tx.execute("INSERT INTO team_user(company_id,id,external_user_id,union_id,display_name,active,private_space_id)
                        VALUES (?1,?2,?3,?4,?5,?6,?7)",
                        params![company,id,user.external_user_id,user.union_id,user.display_name,user.active,space])?;
                    tx.execute("INSERT INTO identity_provider_binding(company_id,provider,external_id,user_id) VALUES (?1,'dingtalk',?2,?3)",
                        params![company,user.union_id,id])?;
                    id
                }
            };
            tx.execute("INSERT INTO team_user_state(company_id,user_id,last_generation) VALUES (?1,?2,?3)
                ON CONFLICT(company_id,user_id) DO UPDATE SET last_generation=excluded.last_generation",
                params![company,id,generation])?;
            users.insert(user.external_user_id.as_str(), id);
        }
        let missing = {
            let mut stmt = tx.prepare("SELECT u.id FROM team_user u LEFT JOIN team_user_state s
                ON s.company_id=u.company_id AND s.user_id=u.id
                WHERE u.company_id=?1 AND u.active=1 AND (s.last_generation IS NULL OR s.last_generation<>?2)")?;
            let rows =
                stmt.query_map(params![company, generation], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for id in missing {
            deactivate(&tx, company, &id, now)?;
        }
        let mut org_ids = HashMap::new();
        for org in &input.orgs {
            let existing: Option<String> = tx
                .query_row(
                    "SELECT id FROM team_org_identity WHERE company_id=?1 AND external_org_id=?2",
                    params![company, org.id],
                    |row| row.get(0),
                )
                .optional()?;
            let id = match existing {
                Some(id) => id,
                None => {
                    let id = Uuid::new_v4().to_string();
                    tx.execute("INSERT INTO team_org_identity(company_id,id,external_org_id) VALUES (?1,?2,?3)",params![company,id,org.id])?;
                    id
                }
            };
            org_ids.insert(org.id.as_str(), id);
        }
        for org in &input.orgs {
            let parent = org.parent_id.as_ref().and_then(|p| org_ids.get(p.as_str()));
            tx.execute("INSERT INTO team_org_unit(company_id,generation,org_id,parent_id,name) VALUES (?1,?2,?3,?4,?5)",
                params![company,generation,org_ids[org.id.as_str()],parent,org.name])?;
        }
        for membership in &input.memberships {
            tx.execute("INSERT INTO team_org_membership(company_id,generation,user_id,org_id) VALUES (?1,?2,?3,?4)",
                params![company,generation,users[membership.user_id.as_str()],org_ids[membership.org_id.as_str()]])?;
        }
        for (ancestor, descendant) in closure {
            tx.execute("INSERT INTO team_org_closure(company_id,generation,ancestor_id,descendant_id) VALUES (?1,?2,?3,?4)",
                params![company,generation,org_ids[ancestor.as_str()],org_ids[descendant.as_str()]])?;
        }
        tx.execute(
            "UPDATE team_company SET directory_generation=?2 WHERE id=?1",
            params![company, generation],
        )?;
        tx.execute("INSERT INTO team_audit_event(id,company_id,action,occurred_at) VALUES (?1,?2,'directory_published',?3)",
            params![Uuid::new_v4().to_string(),company,now])?;
        // Retain the previous complete snapshot for diagnostics; stable identities
        // and grants outlive snapshots and are never deleted by directory pruning.
        tx.execute(
            "DELETE FROM team_org_snapshot WHERE company_id=?1 AND generation<?2",
            params![company, generation.saturating_sub(1)],
        )?;
        tx.commit()?;
        Ok(generation)
    }

    pub fn directory_generation(&self) -> Result<u64, TeamError> {
        Ok(self.db.conn().query_row(
            "SELECT directory_generation FROM team_company WHERE id=?1",
            [self.company_id()],
            |row| row.get(0),
        )?)
    }

    pub fn directory_is_fresh(&self, now: u64, max_age: u64) -> Result<bool, TeamError> {
        directory_fresh(&self.db.conn(), self.company_id(), now, max_age)
    }

    pub fn user_by_union(&self, union: &str) -> Result<Option<UserRecord>, TeamError> {
        Ok(self.db.conn().query_row(
            "SELECT u.id,u.external_user_id,u.union_id,u.display_name,u.active,u.private_space_id,s.auth_version
             FROM team_user u JOIN team_user_state s ON s.company_id=u.company_id AND s.user_id=u.id
             WHERE u.company_id=?1 AND u.union_id=?2",
            params![self.company_id(),union], |row| Ok(UserRecord {
                id:row.get(0)?,external_user_id:row.get(1)?,union_id:row.get(2)?,display_name:row.get(3)?,
                active:row.get(4)?,private_space_id:row.get(5)?,auth_version:row.get(6)?,
            }),
        ).optional()?)
    }

    pub fn org_id_by_external(&self, external: &str) -> Result<Option<String>, TeamError> {
        Ok(self.db.conn().query_row(
            "SELECT i.id FROM team_org_identity i JOIN team_company c ON c.id=i.company_id
             JOIN team_org_unit o ON o.company_id=i.company_id AND o.org_id=i.id AND o.generation=c.directory_generation
             WHERE i.company_id=?1 AND i.external_org_id=?2",
            params![self.company_id(),external], |row| row.get(0),
        ).optional()?)
    }

    /// Membership helper, not authorization: callers must also validate a live
    /// authenticated session and a fresh directory in the same read transaction.
    pub fn is_org_member(
        &self,
        user: &str,
        org: &str,
        descendants: bool,
    ) -> Result<bool, TeamError> {
        Ok(self.db.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM team_org_membership m
             JOIN team_company c ON c.id=m.company_id AND c.directory_generation=m.generation
             JOIN team_user u ON u.company_id=m.company_id AND u.id=m.user_id AND u.active=1
             WHERE m.company_id=?1 AND m.user_id=?2 AND (m.org_id=?3 OR (?4 AND EXISTS(
                SELECT 1 FROM team_org_closure oc WHERE oc.company_id=m.company_id AND oc.generation=m.generation
                AND oc.ancestor_id=?3 AND oc.descendant_id=m.org_id))))",
            params![self.company_id(),user,org,descendants], |row| row.get(0),
        )?)
    }

    pub fn mark_member_inactive(&self, user: &str, now: u64) -> Result<(), TeamError> {
        if now > i64::MAX as u64 {
            return Err(TeamError::InvalidInput);
        }
        let mut conn = self.db.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        deactivate(&tx, self.company_id(), user, now)?;
        tx.commit()?;
        Ok(())
    }
}

pub(super) fn directory_fresh(
    conn: &Connection,
    company: &str,
    now: u64,
    max_age: u64,
) -> Result<bool, TeamError> {
    if max_age == 0 || max_age > 3600 {
        return Ok(false);
    }
    let observed: Option<u64> = conn
        .query_row(
            "SELECT s.observed_at FROM team_company c JOIN team_org_snapshot s
         ON s.company_id=c.id AND s.generation=c.directory_generation WHERE c.id=?1",
            [company],
            |row| row.get(0),
        )
        .optional()?;
    Ok(observed.is_some_and(|time| now.checked_sub(time).is_some_and(|age| age <= max_age)))
}

fn deactivate(tx: &Transaction<'_>, company: &str, user: &str, now: u64) -> Result<(), TeamError> {
    let active: bool = tx
        .query_row(
            "SELECT active FROM team_user WHERE company_id=?1 AND id=?2",
            params![company, user],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(TeamError::NotFound)?;
    // An explicit deactivation still advances the event fence when already
    // inactive, preventing an older in-progress scan from reactivating it.
    tx.execute("INSERT INTO team_user_state(company_id,user_id,auth_version,last_generation,inactive_event_at)
        VALUES (?1,?2,2,0,?3) ON CONFLICT(company_id,user_id) DO UPDATE SET
        auth_version=team_user_state.auth_version+?4,
        inactive_event_at=MAX(team_user_state.inactive_event_at,excluded.inactive_event_at)",
        params![company,user,now,i64::from(active)])?;
    tx.execute(
        "UPDATE team_user SET active=0 WHERE company_id=?1 AND id=?2",
        params![company, user],
    )?;
    tx.execute(
        "INSERT INTO team_audit_event(id,company_id,actor_user_id,action,occurred_at)
        VALUES (?1,?2,?3,'member_inactive',?4)",
        params![Uuid::new_v4().to_string(), company, user, now],
    )?;
    Ok(())
}
