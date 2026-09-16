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
