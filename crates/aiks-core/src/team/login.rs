//! Browser-bound OAuth transactions and a separate native-client proof exchange.
//! Upstream HTTP always happens outside SQLite transactions.
use std::sync::Arc;

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

use super::{
    auth_types::{deadline, digest, valid_id, valid_secret, AuthSecret},
    directory::directory_fresh,
    sessions::{issue, member},
    AuthPolicy, ExternalLogin, SessionTokens, TeamError, TeamStore,
};

#[derive(Debug)]
pub struct LoginStart {
    pub attempt_id: String,
    pub launch_token: AuthSecret,
    pub expires_at: u64,
}
#[derive(Debug)]
pub struct BrowserLogin {
    pub state: AuthSecret,
    pub nonce: AuthSecret,
    pub expires_at: u64,
}
#[derive(Debug)]
pub struct CallbackClaim {
    attempt_id: String,
    capability: AuthSecret,
}

pub struct LoginStore {
    store: Arc<TeamStore>,
    policy: AuthPolicy,
}
impl LoginStore {
    pub fn new(store: Arc<TeamStore>, policy: AuthPolicy) -> Result<Self, TeamError> {
        policy.validate()?;
        Ok(Self { store, policy })
    }
    pub fn start(&self, verifier_hash: &str, now: u64) -> Result<LoginStart, TeamError> {
        if !valid_secret(verifier_hash) {
            return Err(TeamError::InvalidInput);
        }
        let expires = deadline(now, self.policy.login_ttl)?;
        let launch = AuthSecret::random()?;
        let id = Uuid::new_v4().to_string();
        let mut conn = self.store.db().conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        cleanup(&tx, now)?;
        let pending: i64 = tx.query_row(
            "SELECT COUNT(*) FROM team_login_attempt WHERE company_id=?1 AND expires_at>?2",
            params![self.store.company_id(), now],
            |r| r.get(0),
        )?;
        if pending >= 1024 {
            return Err(TeamError::Unavailable);
        }
        tx.execute("INSERT INTO team_login_attempt(id,company_id,verifier_hash,launch_hash,status,created_at,expires_at,changed_at)
            VALUES (?1,?2,?3,?4,'NEW',?5,?6,?5)",params![id,self.store.company_id(),verifier_hash,launch.hash(),now,expires])?;
        tx.commit()?;
        Ok(LoginStart {
            attempt_id: id,
            launch_token: launch,
            expires_at: expires,
        })
    }
    /// The launch secret proves the browser received the native client's launch.
    /// The browser receives a separate HttpOnly nonce; neither is a business token.
    pub fn open_browser(
        &self,
        id: &str,
        launch: &str,
        now: u64,
    ) -> Result<BrowserLogin, TeamError> {
        if !valid_id(id) || !valid_secret(launch) || now > i64::MAX as u64 {
            return Err(TeamError::Unauthorized);
        }
        let state = AuthSecret::random()?;
        let nonce = AuthSecret::random()?;
        let mut conn = self.store.db().conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let expires:u64 = tx.query_row("SELECT expires_at FROM team_login_attempt WHERE id=?1 AND company_id=?2 AND launch_hash=?3
            AND status='NEW' AND created_at<=?4 AND expires_at>?4",params![id,self.store.company_id(),digest(launch),now],|r|r.get(0))
            .optional()?.ok_or(TeamError::Unauthorized)?;
        tx.execute("UPDATE team_login_attempt SET status='BROWSER',state_hash=?2,nonce_hash=?3,changed_at=?4 WHERE id=?1",
            params![id,state.hash(),nonce.hash(),now])?;
        tx.commit()?;
        Ok(BrowserLogin {
            state,
            nonce,
            expires_at: expires,
        })
    }
    pub fn claim_callback(
        &self,
        state: &str,
        nonce: &str,
        code: Option<&str>,
        now: u64,
    ) -> Result<CallbackClaim, TeamError> {
        if !valid_secret(state) || !valid_secret(nonce) || now > i64::MAX as u64 {
            return Err(TeamError::Unauthorized);
        }
        if code.is_some_and(|v| {
            v.is_empty() || v.len() > 4096 || v.trim() != v || v.chars().any(char::is_control)
        }) {
            return Err(TeamError::InvalidInput);
        }
        let capability = AuthSecret::random()?;
        let mut conn = self.store.db().conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id:String = tx.query_row("SELECT id FROM team_login_attempt WHERE company_id=?1 AND state_hash=?2 AND nonce_hash=?3
            AND status='BROWSER' AND changed_at<=?4 AND expires_at>?4",
            params![self.store.company_id(),digest(state),digest(nonce),now],|r|r.get(0))
            .optional()?.ok_or(TeamError::Unauthorized)?;
        if let Some(code) = code {
            let changed = tx.execute(
                "INSERT INTO team_oauth_code_use(company_id,code_hash,expires_at) VALUES (?1,?2,?3)
                ON CONFLICT(company_id,code_hash) DO NOTHING",
                params![self.store.company_id(), digest(code), deadline(now, 86400)?],
            )?;
            if changed == 0 {
                tx.execute(
                    "UPDATE team_login_attempt SET status='FAILED',changed_at=?2 WHERE id=?1",
                    params![id, now],
                )?;
                tx.commit()?;
                return Err(TeamError::Unauthorized);
            }
        }
        tx.execute("UPDATE team_login_attempt SET status='PROCESSING',claim_hash=?2,changed_at=?3 WHERE id=?1",params![id,capability.hash(),now])?;
        tx.commit()?;
        Ok(CallbackClaim {
            attempt_id: id,
            capability,
        })
    }
    pub fn finish_callback(
        &self,
        claim: &CallbackClaim,
        external: ExternalLogin,
        now: u64,
    ) -> Result<(), TeamError> {
        if now > i64::MAX as u64 {
            return Err(TeamError::Unauthorized);
        }
        let mut conn = self.store.db().conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let created = self.require_claim(&tx, claim, now)?;
        let result = (|| {
            if external.corp_id != self.store.corp_id()
                || !valid_id(&external.union_id)
                || !valid_id(&external.external_user_id)
            {
                return Err(TeamError::Unauthorized);
            }
            if !directory_fresh(
                &tx,
                self.store.company_id(),
                now,
                self.policy.directory_max_age,
            )? {
                return Err(TeamError::DirectoryUnavailable);
            }
            let (id, version, event): (String, u64, u64) = tx
                .query_row(
                    "SELECT u.id,s.auth_version,s.inactive_event_at FROM team_user u
                 JOIN team_user_state s ON s.company_id=u.company_id AND s.user_id=u.id
                 WHERE u.company_id=?1 AND u.union_id=?2 AND u.external_user_id=?3 AND u.active=1",
                    params![
                        self.store.company_id(),
                        external.union_id,
                        external.external_user_id
                    ],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?
                .ok_or(TeamError::Unauthorized)?;
            if event > 0 && event >= created {
                return Err(TeamError::Unauthorized);
            }
            member(
                &self.store,
                &tx,
                &id,
                version,
                now,
                self.policy.directory_max_age,
            )?;
            Ok((id, version))
        })();
        match result {
            Ok((id, version)) => {
                tx.execute("UPDATE team_login_attempt SET status='COMPLETE',user_id=?2,auth_version=?3,changed_at=?4 WHERE id=?1",
                    params![claim.attempt_id,id,version,now])?;
                tx.commit()?;
                Ok(())
            }
            Err(TeamError::Storage) => Err(TeamError::Storage),
            Err(error) => {
                tx.execute(
                    "UPDATE team_login_attempt SET status='FAILED',changed_at=?2 WHERE id=?1",
                    params![claim.attempt_id, now],
                )?;
                tx.commit()?;
                Err(error)
            }
        }
    }
    pub fn fail_callback(&self, claim: &CallbackClaim, now: u64) -> Result<(), TeamError> {
        let mut conn = self.store.db().conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.require_claim(&tx, claim, now)?;
        tx.execute(
            "UPDATE team_login_attempt SET status='FAILED',changed_at=?2 WHERE id=?1",
            params![claim.attempt_id, now],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn exchange(&self, id: &str, verifier: &str, now: u64) -> Result<SessionTokens, TeamError> {
        if !valid_id(id) || !valid_secret(verifier) || now > i64::MAX as u64 {
            return Err(TeamError::Unauthorized);
        }
        let mut conn = self.store.db().conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (status,user,version):(String,Option<String>,Option<u64>) = tx.query_row(
            "SELECT status,user_id,auth_version FROM team_login_attempt WHERE id=?1 AND company_id=?2 AND verifier_hash=?3
             AND changed_at<=?4 AND expires_at>?4",params![id,self.store.company_id(),digest(verifier),now],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
        ).optional()?.ok_or(TeamError::Unauthorized)?;
        match status.as_str() {
            "NEW" | "BROWSER" | "PROCESSING" => return Err(TeamError::LoginPending),
            "COMPLETE" => {}
            _ => return Err(TeamError::Unauthorized),
        }
        let user = user.ok_or(TeamError::Storage)?;
        let version = version.ok_or(TeamError::Storage)?;
        let tokens = issue(&tx, &self.store, self.policy, &user, version, now)?;
        tx.execute(
            "UPDATE team_login_attempt SET status='CONSUMED',changed_at=?2 WHERE id=?1",
            params![id, now],
        )?;
        tx.commit()?;
        Ok(tokens)
    }
    fn require_claim(
        &self,
        tx: &Transaction<'_>,
        claim: &CallbackClaim,
        now: u64,
    ) -> Result<u64, TeamError> {
        if now > i64::MAX as u64 {
            return Err(TeamError::Unauthorized);
        }
        tx.query_row("SELECT created_at FROM team_login_attempt WHERE id=?1 AND company_id=?2 AND status='PROCESSING'
            AND claim_hash=?3 AND changed_at<=?4 AND expires_at>?4",
            params![claim.attempt_id,self.store.company_id(),claim.capability.hash(),now],|r|r.get(0))
            .optional()?.ok_or(TeamError::Unauthorized)
    }
}
fn cleanup(tx: &Transaction<'_>, now: u64) -> Result<(), TeamError> {
    tx.execute("DELETE FROM team_login_attempt WHERE id IN (SELECT id FROM team_login_attempt WHERE expires_at<=?1 LIMIT 128)",[now])?;
    tx.execute("DELETE FROM team_oauth_code_use WHERE rowid IN (SELECT rowid FROM team_oauth_code_use WHERE expires_at<=?1 LIMIT 128)",[now])?;
    tx.execute("DELETE FROM team_auth_session WHERE id IN (SELECT id FROM team_auth_session WHERE refresh_expires_at<=?1 LIMIT 32)",[now])?;
    Ok(())
}
