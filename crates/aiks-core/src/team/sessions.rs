//! SQLite-atomic opaque sessions and refresh families, not DingTalk access tokens.
use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

use super::{
    auth_types::{deadline, digest, valid_secret, AuthSecret},
    directory::directory_fresh,
    AuthPolicy, SessionTokens, TeamContext, TeamError, TeamIdentity, TeamStore,
};

pub struct SessionStore {
    pub(crate) store: Arc<TeamStore>,
    pub(crate) policy: AuthPolicy,
}
impl SessionStore {
    pub fn new(store: Arc<TeamStore>, policy: AuthPolicy) -> Result<Self, TeamError> {
        policy.validate()?;
        Ok(Self { store, policy })
    }
    pub fn authenticate(&self, token: &str, now: u64) -> Result<TeamContext, TeamError> {
        if !valid_secret(token) || now > i64::MAX as u64 {
            return Err(TeamError::Unauthorized);
        }
        let mut conn = self.store.db().conn();
        let tx = conn.transaction()?;
        let hash = digest(token);
        let (id, user, space): (String, String, String) = tx.query_row(
            "SELECT id,user_id,space_id FROM team_auth_session WHERE company_id=?1 AND access_hash=?2",
            params![self.store.company_id(), hash], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).optional()?.ok_or(TeamError::Unauthorized)?;
        let ctx = TeamContext {
            instance_id: self.store.instance_id().into(),
            company_id: self.store.company_id().into(),
            user_id: user,
            space_id: space,
            session_id: id,
            access_hash: hash,
        };
        self.identity_in_tx(&tx, &ctx, now)?;
        Ok(ctx)
    }
    pub fn identity(&self, ctx: &TeamContext, now: u64) -> Result<TeamIdentity, TeamError> {
        let mut conn = self.store.db().conn();
        let tx = conn.transaction()?;
        self.identity_in_tx(&tx, ctx, now)
    }
    /// Recheck this inside the same transaction as every business read or write.
    pub(crate) fn identity_in_tx(
        &self,
        tx: &Transaction<'_>,
        ctx: &TeamContext,
        now: u64,
    ) -> Result<TeamIdentity, TeamError> {
        if now > i64::MAX as u64
            || ctx.instance_id() != self.store.instance_id()
            || ctx.company_id() != self.store.company_id()
        {
            return Err(TeamError::Unauthorized);
        }
        let valid: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM team_auth_session WHERE id=?1 AND company_id=?2 AND user_id=?3
             AND space_id=?4 AND access_hash=?5 AND revoked_at IS NULL AND issued_at<=?6 AND access_expires_at>?6)",
            params![ctx.session_id(),ctx.company_id(),ctx.user_id(),ctx.space_id(),ctx.access_hash,now], |r| r.get(0),
        )?;
        if !valid {
            return Err(TeamError::Unauthorized);
        }
        let version: u64 = tx.query_row(
            "SELECT auth_version FROM team_auth_session WHERE id=?1",
            [ctx.session_id()],
            |r| r.get(0),
        )?;
        member(
            &self.store,
            tx,
            ctx.user_id(),
            version,
            now,
            self.policy.directory_max_age,
        )
    }
    pub fn refresh(&self, token: &str, now: u64) -> Result<SessionTokens, TeamError> {
        if !valid_secret(token) || now > i64::MAX as u64 {
            return Err(TeamError::Unauthorized);
        }
        let mut conn = self.store.db().conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let hash = digest(token);
        type Row = (String, String, u64, u64, u64, Option<u64>, Option<u64>);
        let row: Row = tx.query_row(
            "SELECT s.id,s.user_id,s.auth_version,s.issued_at,s.refresh_expires_at,s.revoked_at,t.consumed_at
             FROM team_refresh_token t JOIN team_auth_session s ON s.id=t.session_id
             WHERE t.token_hash=?1 AND s.company_id=?2",
            params![hash,self.store.company_id()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)),
        ).optional()?.ok_or(TeamError::Unauthorized)?;
        let (id, user, version, issued, refresh_expiry, revoked, consumed) = row;
        if consumed.is_some() {
            // Commit revocation before returning an error; rollback would revive the family.
            revoke(&tx, &self.store, &id, &user, now, "refresh_replay")?;
            tx.commit()?;
            return Err(TeamError::Unauthorized);
        }
        if revoked.is_some() || now < issued || now >= refresh_expiry {
            return Err(TeamError::Unauthorized);
        }
        let identity = member(
            &self.store,
            &tx,
            &user,
            version,
            now,
            self.policy.directory_max_age,
        )?;
        let access = AuthSecret::random()?;
        let refresh = AuthSecret::random()?;
        let expiry = deadline(now, self.policy.access_ttl)?.min(refresh_expiry);
        tx.execute(
            "UPDATE team_refresh_token SET consumed_at=?2 WHERE token_hash=?1",
            params![hash, now],
        )?;
        tx.execute("UPDATE team_auth_session SET access_hash=?2,issued_at=?3,access_expires_at=?4 WHERE id=?1",
            params![id,access.hash(),now,expiry])?;
        tx.execute(
            "INSERT INTO team_refresh_token(token_hash,session_id,created_at) VALUES (?1,?2,?3)",
            params![refresh.hash(), id, now],
        )?;
        tx.commit()?;
        Ok(SessionTokens {
            access_token: access,
            refresh_token: refresh,
            expires_in: expiry - now,
            identity,
        })
    }
    pub fn logout(&self, token: &str, now: u64) -> Result<(), TeamError> {
        if !valid_secret(token) || now > i64::MAX as u64 {
            return Err(TeamError::Unauthorized);
        }
        let mut conn = self.store.db().conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let session: Option<(String, String)> = tx
            .query_row(
                "SELECT id,user_id FROM team_auth_session WHERE company_id=?1 AND access_hash=?2",
                params![self.store.company_id(), digest(token)],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((id, user)) = session {
            revoke(&tx, &self.store, &id, &user, now, "session_logout")?;
        }
        tx.commit()?;
        Ok(())
    }
}

pub(super) fn member(
    store: &TeamStore,
    conn: &Connection,
    user: &str,
    version: u64,
    now: u64,
    max_age: u64,
) -> Result<TeamIdentity, TeamError> {
    let found: Option<(String, String)> = conn
        .query_row(
            "SELECT u.private_space_id,u.display_name FROM team_user u
         JOIN team_user_state s ON s.company_id=u.company_id AND s.user_id=u.id
         JOIN team_company c ON c.id=u.company_id AND c.directory_generation=s.last_generation
         WHERE u.company_id=?1 AND u.id=?2 AND u.active=1 AND s.auth_version=?3",
            params![store.company_id(), user, version],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (space, name) = found.ok_or(TeamError::Unauthorized)?;
    if !directory_fresh(conn, store.company_id(), now, max_age)? {
        return Err(TeamError::DirectoryUnavailable);
    }
    Ok(TeamIdentity {
        instance_id: store.instance_id().into(),
        company_id: store.company_id().into(),
        user_id: user.into(),
        space_id: space,
        display_name: name,
    })
}

pub(super) fn issue(
    tx: &Transaction<'_>,
    store: &TeamStore,
    policy: AuthPolicy,
    user: &str,
    version: u64,
    now: u64,
) -> Result<SessionTokens, TeamError> {
    let identity = member(store, tx, user, version, now, policy.directory_max_age)?;
    let access = AuthSecret::random()?;
    let refresh = AuthSecret::random()?;
    let id = Uuid::new_v4().to_string();
    let access_expiry = deadline(now, policy.access_ttl)?;
    let refresh_expiry = deadline(now, policy.refresh_ttl)?;
    // Keep active sessions bounded per member; refuse instead of discarding devices.
    let active: i64 = tx.query_row(
        "SELECT COUNT(*) FROM team_auth_session
        WHERE company_id=?1 AND user_id=?2 AND revoked_at IS NULL AND refresh_expires_at>?3",
        params![store.company_id(), user, now],
        |r| r.get(0),
    )?;
    if active >= 32 {
        return Err(TeamError::Unavailable);
    }
    tx.execute("INSERT INTO team_auth_session(id,company_id,user_id,space_id,auth_version,access_hash,
        created_at,issued_at,access_expires_at,refresh_expires_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?7,?8,?9)",
        params![id,store.company_id(),user,identity.space_id,version,access.hash(),now,access_expiry,refresh_expiry])?;
    tx.execute(
        "INSERT INTO team_refresh_token(token_hash,session_id,created_at) VALUES (?1,?2,?3)",
        params![refresh.hash(), id, now],
    )?;
    tx.execute(
        "INSERT INTO team_audit_event(id,company_id,actor_user_id,action,resource_id,occurred_at)
        VALUES (?1,?2,?3,'session_started',?4,?5)",
        params![
            Uuid::new_v4().to_string(),
            store.company_id(),
            user,
            id,
            now
        ],
    )?;
    Ok(SessionTokens {
        access_token: access,
        refresh_token: refresh,
        expires_in: policy.access_ttl,
        identity,
    })
}
fn revoke(
    tx: &Transaction<'_>,
    store: &TeamStore,
    id: &str,
    user: &str,
    now: u64,
    action: &str,
) -> Result<(), TeamError> {
    let count = tx.execute(
        "UPDATE team_auth_session SET revoked_at=?2 WHERE id=?1 AND revoked_at IS NULL",
        params![id, now],
    )?;
    if count != 0 {
        tx.execute("INSERT INTO team_audit_event(id,company_id,actor_user_id,action,resource_id,occurred_at)
            VALUES (?1,?2,?3,?4,?5,?6)",params![Uuid::new_v4().to_string(),store.company_id(),user,action,id,now])?;
    }
    Ok(())
}
