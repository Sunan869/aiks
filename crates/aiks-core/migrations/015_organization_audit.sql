-- S3.4: durable, append-only provenance for user-reviewed AI organization.
-- Never overwrite source Knowledge or SiYuan content; archive/restore remains
-- an independent knowledge_item.status transition.
CREATE TABLE IF NOT EXISTS knowledge_organization_record (
    derived_knowledge_id TEXT PRIMARY KEY,
    operation TEXT NOT NULL CHECK (
        operation IN ('structure','rewrite','key_conclusions','compare','merge_draft')
    ),
    reviewed_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS knowledge_organization_source (
    derived_knowledge_id TEXT NOT NULL,
    source_knowledge_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0 AND position < 5),
    PRIMARY KEY (derived_knowledge_id,source_knowledge_id),
    UNIQUE (derived_knowledge_id,position)
);
CREATE INDEX IF NOT EXISTS idx_organization_source_lookup
    ON knowledge_organization_source(source_knowledge_id,derived_knowledge_id);
