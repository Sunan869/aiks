//! Opaque credentials and trusted contexts. None can be deserialized from a request.
use std::fmt;

use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::TeamError;
use rusqlite::{Connection, OptionalExtension};

#[derive(Clone, Copy, Debug)]
pub struct AuthPolicy {
    pub login_ttl: u64,
    pub access_ttl: u64,
    pub refresh_ttl: u64,
    pub directory_max_age: u64,
}
impl Default for AuthPolicy {
    fn default() -> Self {
        Self {
            login_ttl: 300,
            access_ttl: 900,
            refresh_ttl: 604800,
            directory_max_age: 900,
        }
    }
}
impl AuthPolicy {
    pub fn validate(&self) -> Result<(), TeamError> {
        if !(60..=600).contains(&self.login_ttl)
            || !(60..=3600).contains(&self.access_ttl)
            || !(3600..=2592000).contains(&self.refresh_ttl)
            || !(1..=3600).contains(&self.directory_max_age)
        {
            return Err(TeamError::ConfigInvalid);
        }
        Ok(())
    }
}

/// Deliberately not Serialize or Clone; transport must explicitly expose a value.
pub struct AuthSecret(String);
impl AuthSecret {
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub(super) fn random() -> Result<Self, TeamError> {
        let mut bytes = [0_u8; 32];
        OsRng
            .try_fill_bytes(&mut bytes)
            .map_err(|_| TeamError::Unavailable)?;
        Ok(Self(hex::encode(bytes)))
    }
    pub(super) fn hash(&self) -> String {
        digest(&self.0)
    }
}
impl fmt::Debug for AuthSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Clone)]
pub struct TeamContext {
    pub(super) instance_id: String,
    pub(super) company_id: String,
    pub(super) user_id: String,
    pub(super) space_id: String,
    pub(super) session_id: String,
    pub(super) access_hash: String,
    pub(super) directory_max_age: u64,
}
impl TeamContext {
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub fn company_id(&self) -> &str {
        &self.company_id
    }
    pub fn user_id(&self) -> &str {
        &self.user_id
    }
    pub fn space_id(&self) -> &str {
        &self.space_id
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn access_hash(&self) -> &str {
        &self.access_hash
    }

    pub(crate) fn directory_max_age(&self) -> u64 {
        self.directory_max_age
    }

    /// Revalidate a trusted context in the same SQLite transaction as the
    /// business operation. This deliberately does not trust a previously
    /// successful bearer-token check across an await boundary.
    pub(crate) fn authorize_in_conn(
        &self,
        conn: &Connection,
        now: u64,
    ) -> Result<TeamIdentity, TeamError> {
        if now > i64::MAX as u64 || self.directory_max_age == 0 || self.directory_max_age > 3600 {
            return Err(TeamError::Unauthorized);
        }
        let display_name: Option<String> = conn
            .query_row(
                "SELECT u.display_name
                 FROM team_company c
                 JOIN team_auth_session s ON s.company_id=c.id
                 JOIN team_user u ON u.company_id=c.id AND u.id=s.user_id
                 JOIN team_user_state us ON us.company_id=c.id AND us.user_id=u.id
                 WHERE c.singleton=1 AND c.id=?1 AND c.instance_id=?2
                   AND s.id=?3 AND s.user_id=?4 AND s.space_id=?5 AND s.access_hash=?6
                   AND s.revoked_at IS NULL AND s.issued_at<=?7 AND s.access_expires_at>?7
                   AND u.active=1 AND u.private_space_id=s.space_id
                   AND us.auth_version=s.auth_version AND us.last_generation=c.directory_generation",
                rusqlite::params![
                    self.company_id,
                    self.instance_id,
                    self.session_id,
                    self.user_id,
                    self.space_id,
                    self.access_hash,
                    now
                ],
                |row| row.get(0),
            )
            .optional()?;
        let display_name = display_name.ok_or(TeamError::Unauthorized)?;
        let observed: Option<u64> = conn
            .query_row(
                "SELECT os.observed_at FROM team_company c JOIN team_org_snapshot os
                 ON os.company_id=c.id AND os.generation=c.directory_generation
                 WHERE c.id=?1",
                [self.company_id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        if !observed.is_some_and(|value| {
            now.checked_sub(value)
                .is_some_and(|age| age <= self.directory_max_age)
        }) {
            return Err(TeamError::DirectoryUnavailable);
        }
        Ok(TeamIdentity {
            instance_id: self.instance_id.clone(),
            company_id: self.company_id.clone(),
            user_id: self.user_id.clone(),
            space_id: self.space_id.clone(),
            display_name,
        })
    }
}
impl fmt::Debug for TeamContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeamContext")
            .field("company_id", &self.company_id)
            .field("user_id", &self.user_id)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct TeamIdentity {
    pub instance_id: String,
    pub company_id: String,
    pub user_id: String,
    pub space_id: String,
    pub display_name: String,
}
#[derive(Debug)]
pub struct SessionTokens {
    pub access_token: AuthSecret,
    pub refresh_token: AuthSecret,
    pub expires_in: u64,
    pub identity: TeamIdentity,
}

pub(super) fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
pub(super) fn valid_secret(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
pub(super) fn deadline(now: u64, ttl: u64) -> Result<u64, TeamError> {
    now.checked_add(ttl)
        .filter(|t| *t <= i64::MAX as u64)
        .ok_or(TeamError::InvalidInput)
}
