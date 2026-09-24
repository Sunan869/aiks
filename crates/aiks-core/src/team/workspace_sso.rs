//! One-time handoff from an authenticated AIKS session to the team SiYuan workspace.

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use super::{
    auth_types::{deadline, digest, valid_secret, AuthSecret},
    policy, Action, SessionStore, TeamContext, TeamError,
};

const WORKSPACE_TICKET_TTL_SECONDS: u64 = 60;
const MAX_ACTIVE_TICKETS_PER_SESSION: i64 = 8;

#[derive(Debug)]
pub struct WorkspaceTicket {
    pub ticket: AuthSecret,
    pub expires_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspacePrincipal {
    pub instance_id: String,
    pub company_id: String,
    pub user_id: String,
    pub space_id: String,
    pub session_id: String,
    pub auth_version: u64,
}

impl SessionStore {
    pub fn issue_workspace_ticket(
        &self,
        access_token: &str,
        now: u64,
    ) -> Result<WorkspaceTicket, TeamError> {
        if !valid_secret(access_token) || now > i64::MAX as u64 {
            return Err(TeamError::Unauthorized);
        }

        let mut conn = self.store.db().conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        cleanup_workspace_tickets(&tx, now)?;

        let access_hash = digest(access_token);
        let row: Option<(String, String, String, u64)> = tx
            .query_row(
                "SELECT id,user_id,space_id,auth_version
                 FROM team_auth_session
                 WHERE company_id=?1 AND access_hash=?2",
                params![self.store.company_id(), access_hash],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let (session_id, user_id, space_id, auth_version) = row.ok_or(TeamError::Unauthorized)?;

        let ctx = TeamContext {
            instance_id: self.store.instance_id().into(),
            company_id: self.store.company_id().into(),
            user_id: user_id.clone(),
            space_id,
            session_id: session_id.clone(),
            access_hash,
            directory_max_age: self.policy.directory_max_age,
        };
        self.identity_in_tx(&tx, &ctx, now)?;

        let active: i64 = tx.query_row(
            "SELECT COUNT(*) FROM team_workspace_ticket
             WHERE company_id=?1 AND session_id=?2 AND consumed_at IS NULL AND expires_at>?3",
            params![self.store.company_id(), session_id, now],
            |r| r.get(0),
        )?;
        if active >= MAX_ACTIVE_TICKETS_PER_SESSION {
            return Err(TeamError::Unavailable);
        }

        let ticket = AuthSecret::random()?;
        let expires_at = deadline(now, WORKSPACE_TICKET_TTL_SECONDS)?;
        tx.execute(
            "INSERT INTO team_workspace_ticket(
                token_hash,company_id,session_id,user_id,auth_version,created_at,expires_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                ticket.hash(),
                self.store.company_id(),
                session_id,
                user_id,
                auth_version,
                now,
                expires_at
            ],
        )?;
        tx.commit()?;

        Ok(WorkspaceTicket { ticket, expires_at })
    }

    pub fn consume_workspace_ticket(
        &self,
        ticket: &str,
        now: u64,
    ) -> Result<WorkspacePrincipal, TeamError> {
        if !valid_secret(ticket) || now > i64::MAX as u64 {
            return Err(TeamError::Unauthorized);
        }

        let mut conn = self.store.db().conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        cleanup_workspace_tickets(&tx, now)?;

        type TicketRow = (String, String, String, u64, String);
        let row: Option<TicketRow> = tx
            .query_row(
                "SELECT t.session_id,t.user_id,s.space_id,t.auth_version,s.access_hash
                 FROM team_workspace_ticket t
                 JOIN team_auth_session s ON s.id=t.session_id AND s.company_id=t.company_id
                 WHERE t.token_hash=?1 AND t.company_id=?2
                   AND t.consumed_at IS NULL AND t.created_at<=?3 AND t.expires_at>?3
                   AND s.user_id=t.user_id AND s.auth_version=t.auth_version",
                params![digest(ticket), self.store.company_id(), now],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        let (session_id, user_id, space_id, auth_version, access_hash) =
            row.ok_or(TeamError::Unauthorized)?;

        let ctx = TeamContext {
            instance_id: self.store.instance_id().into(),
            company_id: self.store.company_id().into(),
            user_id: user_id.clone(),
            space_id: space_id.clone(),
            session_id: session_id.clone(),
            access_hash,
            directory_max_age: self.policy.directory_max_age,
        };
        self.identity_in_tx(&tx, &ctx, now)?;

        let changed = tx.execute(
            "UPDATE team_workspace_ticket SET consumed_at=?2
             WHERE token_hash=?1 AND consumed_at IS NULL",
            params![digest(ticket), now],
        )?;
        if changed != 1 {
            return Err(TeamError::Unauthorized);
        }
        tx.commit()?;

        Ok(WorkspacePrincipal {
            instance_id: self.store.instance_id().into(),
            company_id: self.store.company_id().into(),
            user_id,
            space_id,
            session_id,
            auth_version,
        })
    }

    /// Revalidate a workspace session family without requiring the current short-lived access token.
    /// Refresh rotation preserves the session id/auth version, while logout, replay revocation,
    /// member deactivation and refresh-family expiry invalidate the principal.
    pub fn validate_workspace_principal(
        &self,
        principal: &WorkspacePrincipal,
        now: u64,
    ) -> Result<(), TeamError> {
        let mut conn = self.store.db().conn();
        let tx = conn.transaction()?;
        self.validate_workspace_principal_in_conn(&tx, principal, now)
    }

    /// Authorize one canonical SiYuan document for an already exchanged workspace principal.
    /// This is intentionally document-scoped: notebook/tree/search callers must never infer
    /// visibility from presence in the shared SiYuan workspace.
    pub fn authorize_workspace_document(
        &self,
        principal: &WorkspacePrincipal,
        siyuan_doc_id: &str,
        action: Action,
        now: u64,
    ) -> Result<(), TeamError> {
        if !valid_document_id(siyuan_doc_id) {
            return Err(TeamError::InvalidInput);
        }
        let mut conn = self.store.db().conn();
        let tx = conn.transaction()?;
        self.validate_workspace_principal_in_conn(&tx, principal, now)?;

        let mut stmt = tx.prepare(
            "SELECT o.owner_user_id=?3,
                    EXISTS(SELECT 1 FROM document_share_grant g
                           WHERE g.company_id=o.company_id AND g.knowledge_id=o.knowledge_id
                             AND g.target_user_id=?3),
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
             WHERE o.company_id=?1 AND ki.siyuan_doc_id=?2
             LIMIT 2",
        )?;
        let mut rows = stmt.query(params![
            self.store.company_id(),
            siyuan_doc_id,
            principal.user_id
        ])?;
        let Some(row) = rows.next()? else {
            return Err(TeamError::NotFound);
        };
        let access: (bool, bool, bool) = (row.get(0)?, row.get(1)?, row.get(2)?);
        // Duplicate canonical document mappings are an integrity failure. Hide them rather than
        // selecting an arbitrary ACL row.
        if rows.next()?.is_some() {
            return Err(TeamError::NotFound);
        }
        if policy::allows(action, true, true, true, access.0, access.1, access.2) {
            Ok(())
        } else if matches!(action, Action::Read) {
            Err(TeamError::NotFound)
        } else {
            Err(TeamError::Forbidden)
        }
    }

    fn validate_workspace_principal_in_conn(
        &self,
        conn: &Connection,
        principal: &WorkspacePrincipal,
        now: u64,
    ) -> Result<(), TeamError> {
        if now > i64::MAX as u64
            || principal.instance_id != self.store.instance_id()
            || principal.company_id != self.store.company_id()
        {
            return Err(TeamError::Unauthorized);
        }

        let active: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM team_auth_session
                 WHERE id=?1 AND company_id=?2 AND user_id=?3 AND space_id=?4 AND auth_version=?5
                   AND revoked_at IS NULL AND created_at<=?6 AND refresh_expires_at>?6",
                params![
                    principal.session_id,
                    self.store.company_id(),
                    principal.user_id,
                    principal.space_id,
                    principal.auth_version,
                    now
                ],
                |r| r.get(0),
            )
            .optional()?;
        if active.is_none() {
            return Err(TeamError::Unauthorized);
        }

        let identity = super::sessions::member(
            &self.store,
            conn,
            &principal.user_id,
            principal.auth_version,
            now,
            self.policy.directory_max_age,
        )?;
        if identity.space_id != principal.space_id {
            return Err(TeamError::Unauthorized);
        }
        Ok(())
    }
}

fn valid_document_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn cleanup_workspace_tickets(tx: &rusqlite::Transaction<'_>, now: u64) -> Result<(), TeamError> {
    tx.execute(
        "DELETE FROM team_workspace_ticket
         WHERE token_hash IN (
            SELECT token_hash FROM team_workspace_ticket
            WHERE expires_at<=?1 OR consumed_at IS NOT NULL
            LIMIT 128
         )",
        [now],
    )?;
    Ok(())
}
