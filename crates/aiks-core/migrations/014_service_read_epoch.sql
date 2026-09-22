-- Conservative generation fence for queries which await a model/content store.
-- This is not a second queue. No counters are inferred from mutable timestamps.
CREATE TABLE IF NOT EXISTS service_read_epoch (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    generation INTEGER NOT NULL DEFAULT 0 CHECK(generation >= 0)
);
INSERT INTO service_read_epoch(singleton,generation) VALUES(1,0) ON CONFLICT DO NOTHING;
CREATE TRIGGER IF NOT EXISTS service_epoch_binding_insert AFTER INSERT ON service_session_binding
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_binding_update AFTER UPDATE ON service_session_binding
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_binding_delete AFTER DELETE ON service_session_binding
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_derived_insert AFTER INSERT ON service_derived_state
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_derived_update AFTER UPDATE ON service_derived_state
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_derived_delete AFTER DELETE ON service_derived_state
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_knowledge_insert AFTER INSERT ON knowledge_item
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_knowledge_update AFTER UPDATE ON knowledge_item
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_knowledge_delete AFTER DELETE ON knowledge_item
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER IF NOT EXISTS service_epoch_session_update AFTER UPDATE ON source_session
BEGIN UPDATE service_read_epoch SET generation=generation+1 WHERE singleton=1; END;
