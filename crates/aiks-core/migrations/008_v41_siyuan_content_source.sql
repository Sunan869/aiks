CREATE TABLE IF NOT EXISTS content_migration (
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'migrated', 'reused', 'conflict', 'failed')),
    target_doc_id TEXT,
    source_hash TEXT,
    target_hash TEXT,
    error_message TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(entity_type, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_content_migration_status
ON content_migration(status);

-- Keep the canonical Raw Session document mapping current for every future
-- SiYuan sync path. Existing rows are backfilled by the V4.1 startup migration.
CREATE TRIGGER IF NOT EXISTS trg_v41_sync_session_siyuan_doc_insert
AFTER INSERT ON sync_target
WHEN NEW.sink = 'siyuan' AND NEW.target_id IS NOT NULL AND NEW.target_id <> ''
BEGIN
    UPDATE source_session
    SET siyuan_doc_id = NEW.target_id
    WHERE id = NEW.session_id;
END;

CREATE TRIGGER IF NOT EXISTS trg_v41_sync_session_siyuan_doc_update
AFTER UPDATE OF target_id ON sync_target
WHEN NEW.sink = 'siyuan' AND NEW.target_id IS NOT NULL AND NEW.target_id <> ''
BEGIN
    UPDATE source_session
    SET siyuan_doc_id = NEW.target_id
    WHERE id = NEW.session_id;
END;
