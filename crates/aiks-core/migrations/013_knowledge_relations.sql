-- S2.3: explicit, reviewable relationships between stable knowledge identities.
-- Never delete source knowledge or assume title similarity proves identity.
CREATE TABLE IF NOT EXISTS knowledge_relation (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    relation_type TEXT NOT NULL CHECK (relation_type IN ('related','supplements','corrects','supersedes','resolved_by')),
    status TEXT NOT NULL CHECK (status IN ('suggested','confirmed','rejected')),
    evidence TEXT NOT NULL,
    confidence REAL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (source_id != target_id)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_relation_identity
    ON knowledge_relation(source_id, target_id, relation_type);
CREATE INDEX IF NOT EXISTS idx_knowledge_relation_source
    ON knowledge_relation(source_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_knowledge_relation_target
    ON knowledge_relation(target_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS knowledge_relation_review (
    id TEXT PRIMARY KEY,
    relation_id TEXT NOT NULL,
    previous_status TEXT NOT NULL,
    next_status TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_knowledge_relation_review_history
    ON knowledge_relation_review(relation_id, created_at);
