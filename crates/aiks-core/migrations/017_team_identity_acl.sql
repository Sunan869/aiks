-- Additive single-company identity and read-grant foundation.
-- No legacy ownership backfill; no login session or permission is created here.
CREATE TABLE IF NOT EXISTS team_company (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    id TEXT NOT NULL UNIQUE,
    instance_id TEXT NOT NULL UNIQUE,
    provider TEXT NOT NULL DEFAULT 'dingtalk' CHECK(provider='dingtalk'),
    corp_id TEXT NOT NULL UNIQUE,
    client_id TEXT NOT NULL,
    directory_generation INTEGER NOT NULL DEFAULT 0 CHECK(directory_generation>=0),
    created_at INTEGER NOT NULL CHECK(created_at>=0)
);
CREATE TABLE IF NOT EXISTS team_user (
    company_id TEXT NOT NULL REFERENCES team_company(id),
    id TEXT NOT NULL,
    external_user_id TEXT NOT NULL,
    union_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 0 CHECK(active IN (0,1)),
    private_space_id TEXT UNIQUE,
    PRIMARY KEY(company_id,id),
    UNIQUE(company_id,external_user_id),
    UNIQUE(company_id,union_id)
);
CREATE TABLE IF NOT EXISTS identity_provider_binding (
    company_id TEXT NOT NULL,
    provider TEXT NOT NULL CHECK(provider='dingtalk'),
    external_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    PRIMARY KEY(company_id,provider,external_id),
    UNIQUE(company_id,provider,user_id),
    FOREIGN KEY(company_id,user_id) REFERENCES team_user(company_id,id)
);
CREATE TABLE IF NOT EXISTS team_org_identity (
    company_id TEXT NOT NULL REFERENCES team_company(id),
    id TEXT NOT NULL,
    external_org_id TEXT NOT NULL,
    PRIMARY KEY(company_id,id),
    UNIQUE(company_id,external_org_id)
);
CREATE TABLE IF NOT EXISTS team_knowledge_owner (
    company_id TEXT NOT NULL,
    knowledge_id TEXT NOT NULL REFERENCES knowledge_item(id) ON DELETE CASCADE,
    owner_user_id TEXT NOT NULL,
    content_revision INTEGER NOT NULL DEFAULT 1 CHECK(content_revision>0),
    grant_version INTEGER NOT NULL DEFAULT 0 CHECK(grant_version>=0),
    PRIMARY KEY(knowledge_id),
    UNIQUE(company_id,knowledge_id),
    FOREIGN KEY(company_id,owner_user_id) REFERENCES team_user(company_id,id)
);
CREATE TABLE IF NOT EXISTS document_share_grant (
    company_id TEXT NOT NULL,
    id TEXT NOT NULL PRIMARY KEY,
    knowledge_id TEXT NOT NULL,
    target_user_id TEXT,
    target_org_id TEXT,
    include_descendants INTEGER NOT NULL DEFAULT 0 CHECK(include_descendants IN (0,1)),
    permission TEXT NOT NULL DEFAULT 'read' CHECK(permission='read'),
    CHECK((target_user_id IS NOT NULL AND target_org_id IS NULL AND include_descendants=0)
       OR (target_user_id IS NULL AND target_org_id IS NOT NULL)),
    FOREIGN KEY(company_id,knowledge_id) REFERENCES team_knowledge_owner(company_id,knowledge_id) ON DELETE CASCADE,
    FOREIGN KEY(company_id,target_user_id) REFERENCES team_user(company_id,id),
    FOREIGN KEY(company_id,target_org_id) REFERENCES team_org_identity(company_id,id)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_team_grant_person ON document_share_grant(company_id,knowledge_id,target_user_id) WHERE target_user_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_team_grant_org ON document_share_grant(company_id,knowledge_id,target_org_id) WHERE target_org_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_team_owned_knowledge ON team_knowledge_owner(company_id,owner_user_id);
CREATE TABLE IF NOT EXISTS team_audit_event (
    id TEXT NOT NULL PRIMARY KEY,
    company_id TEXT NOT NULL REFERENCES team_company(id),
    actor_user_id TEXT,
    action TEXT NOT NULL,
    resource_id TEXT,
    occurred_at INTEGER NOT NULL CHECK(occurred_at>=0),
    FOREIGN KEY(company_id,actor_user_id) REFERENCES team_user(company_id,id)
);
-- Conservative consistency fence for future reads which await external I/O.
CREATE TRIGGER IF NOT EXISTS service_epoch_team_user_insert AFTER INSERT ON team_user
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_team_user_update AFTER UPDATE ON team_user
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_team_owner_insert AFTER INSERT ON team_knowledge_owner
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_team_owner_update AFTER UPDATE ON team_knowledge_owner
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_team_owner_delete AFTER DELETE ON team_knowledge_owner
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_team_grant_insert AFTER INSERT ON document_share_grant
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_team_grant_update AFTER UPDATE ON document_share_grant
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_team_grant_delete AFTER DELETE ON document_share_grant
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
