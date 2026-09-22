-- Per-item source provenance. A later extraction may preserve a user-owned or
-- already published item, so session-wide extraction freshness is not its age.
-- Unknown legacy rows remain unknown; adoption/publication must prove a version.
CREATE TABLE IF NOT EXISTS service_knowledge_revision (
    knowledge_id TEXT PRIMARY KEY REFERENCES knowledge_item(id) ON DELETE CASCADE,
    session_id INTEGER NOT NULL REFERENCES service_session_binding(session_id),
    revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 4294967295)
);
CREATE INDEX IF NOT EXISTS idx_service_knowledge_revision_session
ON service_knowledge_revision(session_id, revision);
CREATE TRIGGER IF NOT EXISTS service_epoch_knowledge_revision_insert AFTER INSERT ON service_knowledge_revision
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_knowledge_revision_update AFTER UPDATE ON service_knowledge_revision
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_knowledge_revision_delete AFTER DELETE ON service_knowledge_revision
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
