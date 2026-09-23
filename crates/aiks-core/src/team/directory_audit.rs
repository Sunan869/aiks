//! Fixed audit events do not persist upstream messages or directory payloads.
use rusqlite::params;
use uuid::Uuid;

use super::{TeamError, TeamStore};

impl TeamStore {
    pub fn record_directory_refresh_failure(&self, now: u64) -> Result<(), TeamError> {
        if now > i64::MAX as u64 {
            return Err(TeamError::InvalidInput);
        }
        self.db.conn().execute(
            "INSERT INTO team_audit_event(id,company_id,action,occurred_at)
             VALUES (?1,?2,'directory_refresh_failed',?3)",
            params![Uuid::new_v4().to_string(), self.company_id(), now],
        )?;
        Ok(())
    }
}
