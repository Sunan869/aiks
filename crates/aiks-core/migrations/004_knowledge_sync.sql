-- ── Knowledge Sync Target (knowledge_item → SiYuan doc mapping) ─────────────────
-- Tracks the remote SiYuan document for each distilled knowledge item so that
-- repeated runs update in place instead of creating duplicates, and manual
-- edits in SiYuan can be detected as conflicts.

CREATE TABLE IF NOT EXISTS knowledge_sync_target (
    knowledge_id TEXT PRIMARY KEY,
    sink TEXT NOT NULL DEFAULT 'siyuan',
    target_id TEXT,
    target_path TEXT,
    synced_hash TEXT,
    status TEXT NOT NULL DEFAULT 'PENDING',
    error_message TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_knowledge_sync_status
ON knowledge_sync_target(status);
