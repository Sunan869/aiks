-- One-time SSO handoff from AIKS Service to the team SiYuan workspace.
-- Only ticket digests persist; access/refresh credentials are never copied into SiYuan.
CREATE TABLE IF NOT EXISTS team_workspace_ticket (
    token_hash TEXT NOT NULL PRIMARY KEY CHECK(length(token_hash)=64),
    company_id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES team_auth_session(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    auth_version INTEGER NOT NULL CHECK(auth_version>0),
    created_at INTEGER NOT NULL CHECK(created_at>=0),
    expires_at INTEGER NOT NULL CHECK(expires_at>created_at),
    consumed_at INTEGER CHECK(consumed_at IS NULL OR consumed_at>=created_at),
    FOREIGN KEY(company_id,user_id) REFERENCES team_user(company_id,id)
);
CREATE INDEX IF NOT EXISTS idx_team_workspace_ticket_session
ON team_workspace_ticket(company_id,session_id,expires_at,consumed_at);
