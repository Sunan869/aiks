-- V4.2 canonical deletion lifecycle
-- SQLite cannot ALTER a CHECK constraint in place, so rebuild knowledge_item
-- while preserving every column introduced through V10.

PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;

CREATE TABLE knowledge_item_v42_status (
    id TEXT PRIMARY KEY,
    source_session_id INTEGER,
    project_name TEXT,
    title TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'general',
    summary TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    confidence REAL NOT NULL DEFAULT 0.0,
    worth_extracting INTEGER NOT NULL DEFAULT 1,
    source_type TEXT NOT NULL DEFAULT 'conversation'
        CHECK(source_type IN ('conversation', 'manual')),
    managed_by TEXT NOT NULL DEFAULT 'pipeline'
        CHECK(managed_by IN ('pipeline', 'user')),
    status TEXT NOT NULL DEFAULT 'active'
        CHECK(status IN ('active', 'archived', 'deleted')),
    is_favorite INTEGER NOT NULL DEFAULT 0
        CHECK(is_favorite IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    siyuan_doc_id TEXT,
    generated_hash TEXT,
    current_remote_hash TEXT,
    migration_status TEXT NOT NULL DEFAULT 'pending',
    index_status TEXT NOT NULL DEFAULT 'pending',
    indexed_hash TEXT,
    indexed_at TEXT,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    index_chunk_count INTEGER NOT NULL DEFAULT 0,
    last_index_error TEXT,
    FOREIGN KEY(source_session_id) REFERENCES source_session(id)
);

INSERT INTO knowledge_item_v42_status (
    id, source_session_id, project_name, title, category, summary, content,
    tags, confidence, worth_extracting, source_type, managed_by, status,
    is_favorite, created_at, updated_at, siyuan_doc_id, generated_hash,
    current_remote_hash, migration_status, index_status, indexed_hash,
    indexed_at, embedding_model, embedding_dimensions, index_chunk_count,
    last_index_error
)
SELECT
    id, source_session_id, project_name, title, category, summary, content,
    tags, confidence, worth_extracting, source_type, managed_by, status,
    is_favorite, created_at, updated_at, siyuan_doc_id, generated_hash,
    current_remote_hash, migration_status, index_status, indexed_hash,
    indexed_at, embedding_model, embedding_dimensions, index_chunk_count,
    last_index_error
FROM knowledge_item;

DROP TABLE knowledge_item;
ALTER TABLE knowledge_item_v42_status RENAME TO knowledge_item;

CREATE INDEX IF NOT EXISTS idx_knowledge_item_session
ON knowledge_item(source_session_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_item_category
ON knowledge_item(category);

CREATE INDEX IF NOT EXISTS idx_knowledge_item_source_type
ON knowledge_item(source_type);

CREATE INDEX IF NOT EXISTS idx_knowledge_item_status
ON knowledge_item(status);

CREATE INDEX IF NOT EXISTS idx_knowledge_item_favorite
ON knowledge_item(is_favorite);

CREATE INDEX IF NOT EXISTS idx_knowledge_item_index_status
ON knowledge_item(index_status);

CREATE INDEX IF NOT EXISTS idx_knowledge_item_siyuan_index_status
ON knowledge_item(siyuan_doc_id, index_status);

COMMIT;
PRAGMA foreign_keys = ON;
