-- Idempotent personal-to-team knowledge imports.
-- The source fingerprint is scoped to the authenticated owner and never grants sharing.
CREATE TABLE IF NOT EXISTS team_knowledge_import (
    company_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    source_fingerprint TEXT NOT NULL,
    payload_hash TEXT NOT NULL CHECK(length(payload_hash)=64),
    knowledge_id TEXT NOT NULL,
    created_at INTEGER NOT NULL CHECK(created_at>=0),
    PRIMARY KEY(company_id,owner_user_id,operation_id),
    UNIQUE(company_id,owner_user_id,source_fingerprint),
    UNIQUE(company_id,knowledge_id),
    FOREIGN KEY(company_id,owner_user_id) REFERENCES team_user(company_id,id),
    FOREIGN KEY(company_id,knowledge_id)
        REFERENCES team_knowledge_owner(company_id,knowledge_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_team_import_knowledge
ON team_knowledge_import(company_id,knowledge_id);
