-- Native login transactions and revocable opaque sessions. Only digests persist.
CREATE TABLE IF NOT EXISTS team_login_attempt (
    id TEXT NOT NULL PRIMARY KEY,
    company_id TEXT NOT NULL REFERENCES team_company(id),
    verifier_hash TEXT NOT NULL CHECK(length(verifier_hash)=64),
    launch_hash TEXT NOT NULL UNIQUE CHECK(length(launch_hash)=64),
    state_hash TEXT UNIQUE CHECK(state_hash IS NULL OR length(state_hash)=64),
    nonce_hash TEXT CHECK(nonce_hash IS NULL OR length(nonce_hash)=64),
    claim_hash TEXT CHECK(claim_hash IS NULL OR length(claim_hash)=64),
    status TEXT NOT NULL CHECK(status IN ('NEW','BROWSER','PROCESSING','COMPLETE','CONSUMED','FAILED')),
    user_id TEXT,
    auth_version INTEGER CHECK(auth_version IS NULL OR auth_version>0),
    created_at INTEGER NOT NULL CHECK(created_at>=0),
    expires_at INTEGER NOT NULL CHECK(expires_at>created_at),
    changed_at INTEGER NOT NULL CHECK(changed_at>=created_at),
    FOREIGN KEY(company_id,user_id) REFERENCES team_user(company_id,id)
);
CREATE INDEX IF NOT EXISTS idx_team_login_budget ON team_login_attempt(company_id,expires_at,status);
CREATE TABLE IF NOT EXISTS team_oauth_code_use (
    company_id TEXT NOT NULL REFERENCES team_company(id),
    code_hash TEXT NOT NULL CHECK(length(code_hash)=64),
    expires_at INTEGER NOT NULL CHECK(expires_at>=0),
    PRIMARY KEY(company_id,code_hash)
);
CREATE TABLE IF NOT EXISTS team_auth_session (
    id TEXT NOT NULL PRIMARY KEY,
    company_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    auth_version INTEGER NOT NULL CHECK(auth_version>0),
    access_hash TEXT NOT NULL UNIQUE CHECK(length(access_hash)=64),
    created_at INTEGER NOT NULL CHECK(created_at>=0),
    issued_at INTEGER NOT NULL CHECK(issued_at>=created_at),
    access_expires_at INTEGER NOT NULL CHECK(access_expires_at>issued_at),
    refresh_expires_at INTEGER NOT NULL CHECK(refresh_expires_at>created_at),
    revoked_at INTEGER CHECK(revoked_at IS NULL OR revoked_at>=0),
    FOREIGN KEY(company_id,user_id) REFERENCES team_user(company_id,id)
);
CREATE INDEX IF NOT EXISTS idx_team_session_member ON team_auth_session(company_id,user_id,revoked_at);
CREATE TABLE IF NOT EXISTS team_refresh_token (
    token_hash TEXT PRIMARY KEY CHECK(length(token_hash)=64),
    session_id TEXT NOT NULL REFERENCES team_auth_session(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL CHECK(created_at>=0),
    consumed_at INTEGER CHECK(consumed_at IS NULL OR consumed_at>=created_at)
);
CREATE INDEX IF NOT EXISTS idx_team_refresh_family ON team_refresh_token(session_id);
-- The user's persistent auth_version also fences stored/in-flight contexts.
CREATE TRIGGER IF NOT EXISTS team_deactivate_sessions AFTER UPDATE OF auth_version ON team_user_state
WHEN NEW.auth_version<>OLD.auth_version
BEGIN
    UPDATE team_auth_session SET revoked_at=COALESCE(revoked_at,NEW.inactive_event_at)
    WHERE company_id=NEW.company_id AND user_id=NEW.user_id AND auth_version<>NEW.auth_version;
    UPDATE team_login_attempt SET status='FAILED'
    WHERE company_id=NEW.company_id AND user_id=NEW.user_id
      AND status='COMPLETE' AND auth_version<>NEW.auth_version;
END;
CREATE TRIGGER IF NOT EXISTS service_epoch_auth_session_update AFTER UPDATE ON team_auth_session
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_auth_session_delete AFTER DELETE ON team_auth_session
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
