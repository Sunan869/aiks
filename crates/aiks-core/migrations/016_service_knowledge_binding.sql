-- Explicit ownership for legacy standalone documents; never infer ownership
-- from titles, paths or model output. It cannot override a session binding.
CREATE TABLE IF NOT EXISTS service_knowledge_binding (
    knowledge_id TEXT PRIMARY KEY REFERENCES knowledge_item(id) ON DELETE CASCADE,
    principal_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_service_knowledge_binding_scope ON service_knowledge_binding(principal_id,space_id);
CREATE TRIGGER IF NOT EXISTS service_epoch_document_binding_insert AFTER INSERT ON service_knowledge_binding
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_document_binding_update AFTER UPDATE ON service_knowledge_binding
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_document_binding_delete AFTER DELETE ON service_knowledge_binding
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
