-- S2.2: local user feedback is append-only and survives knowledge re-extraction.
-- Deliberately no foreign key: a pipeline-owned knowledge row may be removed,
-- but the audit trail must remain for review and future reconciliation.
CREATE TABLE IF NOT EXISTS knowledge_feedback (
    id TEXT PRIMARY KEY,
    knowledge_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('useful','incorrect','duplicate','outdated','needs_detail')),
    note TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_knowledge_feedback_item_time
    ON knowledge_feedback(knowledge_id, created_at DESC);
