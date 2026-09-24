-- Durable owner-only content operations and managed asset bytes.
-- Additive only: 017-019 are already deployed and must never be renumbered.
CREATE TABLE IF NOT EXISTS team_content_operation (
    company_id TEXT NOT NULL,
    id TEXT NOT NULL,
    knowledge_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    origin_session_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('content','publish')),
    base_revision INTEGER NOT NULL CHECK(base_revision>0),
    directory_max_age INTEGER NOT NULL CHECK(directory_max_age BETWEEN 1 AND 3600),
    payload_hash TEXT NOT NULL CHECK(length(payload_hash)=64),
    target_title TEXT NOT NULL,
    target_markdown TEXT NOT NULL,
    target_hash TEXT NOT NULL CHECK(length(target_hash)=64),
    expected_remote_hash TEXT,
    target_doc_id TEXT,
    state TEXT NOT NULL CHECK(state IN ('pending','applying','verifying','done','conflict','failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK(attempt_count BETWEEN 0 AND 1000),
    lease_token TEXT,
    lease_expires_at INTEGER,
    result_revision INTEGER CHECK(result_revision IS NULL OR result_revision>0),
    failure_code TEXT,
    created_at INTEGER NOT NULL CHECK(created_at>=0),
    updated_at INTEGER NOT NULL CHECK(updated_at>=created_at),
    PRIMARY KEY(company_id,id),
    FOREIGN KEY(company_id,knowledge_id) REFERENCES team_knowledge_owner(company_id,knowledge_id) ON DELETE CASCADE,
    FOREIGN KEY(company_id,owner_user_id) REFERENCES team_user(company_id,id),
    FOREIGN KEY(origin_session_id) REFERENCES team_auth_session(id),
    CHECK((lease_token IS NULL AND lease_expires_at IS NULL) OR (lease_token IS NOT NULL AND lease_expires_at IS NOT NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_team_content_one_open
ON team_content_operation(company_id,knowledge_id)
WHERE state IN ('pending','applying','verifying');
CREATE INDEX IF NOT EXISTS idx_team_content_claim
ON team_content_operation(company_id,state,lease_expires_at,created_at);

CREATE TABLE IF NOT EXISTS team_managed_asset (
    company_id TEXT NOT NULL,
    id TEXT NOT NULL,
    knowledge_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    content_hash TEXT NOT NULL CHECK(length(content_hash)=64),
    byte_len INTEGER NOT NULL CHECK(byte_len>=0),
    bytes BLOB NOT NULL,
    created_at INTEGER NOT NULL CHECK(created_at>=0),
    PRIMARY KEY(company_id,id),
    FOREIGN KEY(company_id,knowledge_id) REFERENCES team_knowledge_owner(company_id,knowledge_id) ON DELETE CASCADE,
    FOREIGN KEY(company_id,owner_user_id) REFERENCES team_user(company_id,id)
);
CREATE INDEX IF NOT EXISTS idx_team_asset_document ON team_managed_asset(company_id,knowledge_id);

CREATE TRIGGER IF NOT EXISTS service_epoch_team_content_insert AFTER INSERT ON team_content_operation
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_team_content_update AFTER UPDATE ON team_content_operation
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_team_asset_insert AFTER INSERT ON team_managed_asset
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_team_asset_delete AFTER DELETE ON team_managed_asset
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
