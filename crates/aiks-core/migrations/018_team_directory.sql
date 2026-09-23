-- Published directory generations and stable per-user revocation counters.
-- This is additive: do not rewrite migration 017 or infer legacy ownership.
CREATE TABLE IF NOT EXISTS team_user_state (
    company_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    auth_version INTEGER NOT NULL DEFAULT 1 CHECK(auth_version>0),
    last_generation INTEGER NOT NULL CHECK(last_generation>=0),
    inactive_event_at INTEGER NOT NULL DEFAULT 0 CHECK(inactive_event_at>=0),
    PRIMARY KEY(company_id,user_id),
    FOREIGN KEY(company_id,user_id) REFERENCES team_user(company_id,id)
);
CREATE TABLE IF NOT EXISTS team_org_snapshot (
    company_id TEXT NOT NULL REFERENCES team_company(id),
    generation INTEGER NOT NULL CHECK(generation>0),
    scope_json TEXT NOT NULL,
    observed_at INTEGER NOT NULL CHECK(observed_at>=0),
    published_at INTEGER NOT NULL CHECK(published_at>=observed_at),
    PRIMARY KEY(company_id,generation)
);
CREATE TABLE IF NOT EXISTS team_org_unit (
    company_id TEXT NOT NULL,
    generation INTEGER NOT NULL,
    org_id TEXT NOT NULL,
    parent_id TEXT,
    name TEXT NOT NULL,
    PRIMARY KEY(company_id,generation,org_id),
    FOREIGN KEY(company_id,generation) REFERENCES team_org_snapshot(company_id,generation) ON DELETE CASCADE,
    FOREIGN KEY(company_id,org_id) REFERENCES team_org_identity(company_id,id),
    FOREIGN KEY(company_id,generation,parent_id) REFERENCES team_org_unit(company_id,generation,org_id) DEFERRABLE INITIALLY DEFERRED
);
CREATE TABLE IF NOT EXISTS team_org_membership (
    company_id TEXT NOT NULL,
    generation INTEGER NOT NULL,
    user_id TEXT NOT NULL,
    org_id TEXT NOT NULL,
    PRIMARY KEY(company_id,generation,user_id,org_id),
    FOREIGN KEY(company_id,user_id) REFERENCES team_user(company_id,id),
    FOREIGN KEY(company_id,generation,org_id) REFERENCES team_org_unit(company_id,generation,org_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_team_members_in_org ON team_org_membership(company_id,generation,org_id,user_id);
CREATE TABLE IF NOT EXISTS team_org_closure (
    company_id TEXT NOT NULL,
    generation INTEGER NOT NULL,
    ancestor_id TEXT NOT NULL,
    descendant_id TEXT NOT NULL,
    PRIMARY KEY(company_id,generation,ancestor_id,descendant_id),
    FOREIGN KEY(company_id,generation,ancestor_id) REFERENCES team_org_unit(company_id,generation,org_id) ON DELETE CASCADE,
    FOREIGN KEY(company_id,generation,descendant_id) REFERENCES team_org_unit(company_id,generation,org_id) ON DELETE CASCADE
);
CREATE TRIGGER IF NOT EXISTS service_epoch_directory_publish AFTER UPDATE OF directory_generation ON team_company
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_member_revocation AFTER UPDATE OF auth_version ON team_user_state
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
