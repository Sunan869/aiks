-- V3 Pipeline Data Model Migration
-- Adds unified knowledge processing pipeline tables
-- Preserves all V2.5 tables for backward compatibility

PRAGMA foreign_keys = ON;

-- ── Session Chunk (LLM chunking for long sessions) ──────────────────────────────
CREATE TABLE IF NOT EXISTS session_chunk (
    id TEXT PRIMARY KEY,
    session_id INTEGER NOT NULL,
    chunk_index INTEGER NOT NULL,
    message_start INTEGER NOT NULL,
    message_end INTEGER NOT NULL,
    token_count INTEGER NOT NULL DEFAULT 0,
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES source_session(id),
    UNIQUE(session_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_session_chunk_session
ON session_chunk(session_id);

-- ── Knowledge Item (0~N per session, V3 replaces 1:1 extraction) ───────────────
CREATE TABLE IF NOT EXISTS knowledge_item (
    id TEXT PRIMARY KEY,
    source_session_id INTEGER NOT NULL,
    project_name TEXT,
    title TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'general',
    summary TEXT NOT NULL,
    content TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',  -- JSON array
    confidence REAL NOT NULL DEFAULT 0.0,
    worth_extracting INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(source_session_id) REFERENCES source_session(id)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_item_session
ON knowledge_item(source_session_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_item_category
ON knowledge_item(category);

-- ── Knowledge Chunk (Embedding chunks from knowledge items) ──────────────────────
CREATE TABLE IF NOT EXISTS knowledge_chunk (
    id TEXT PRIMARY KEY,
    knowledge_id TEXT NOT NULL,
    heading TEXT,
    chunk_index INTEGER NOT NULL,
    token_count INTEGER NOT NULL DEFAULT 0,
    text TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(knowledge_id) REFERENCES knowledge_item(id),
    UNIQUE(knowledge_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_chunk_knowledge
ON knowledge_chunk(knowledge_id);

-- ── Embedding Record ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS embedding_record (
    id TEXT PRIMARY KEY,
    chunk_id TEXT NOT NULL,
    model TEXT NOT NULL,
    dimensions INTEGER NOT NULL,
    -- Vector stored as blob (float32 array, little-endian)
    vector BLOB,
    created_at TEXT NOT NULL,
    FOREIGN KEY(chunk_id) REFERENCES knowledge_chunk(id),
    UNIQUE(chunk_id, model)
);

CREATE INDEX IF NOT EXISTS idx_embedding_chunk
ON embedding_record(chunk_id);

-- ── Pipeline Run ──────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS pipeline_run (
    id TEXT PRIMARY KEY,
    session_id INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'DISCOVERED',
    current_stage TEXT,
    pipeline_version TEXT NOT NULL DEFAULT 'v3',
    source_hash TEXT,
    started_at TEXT,
    finished_at TEXT,
    error_stage TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES source_session(id),
    UNIQUE(session_id, pipeline_version)
);

CREATE INDEX IF NOT EXISTS idx_pipeline_run_session
ON pipeline_run(session_id);

CREATE INDEX IF NOT EXISTS idx_pipeline_run_status
ON pipeline_run(status);

-- ── Pipeline Stage Run ────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS pipeline_stage_run (
    id TEXT PRIMARY KEY,
    pipeline_run_id TEXT NOT NULL,
    stage TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'PENDING',
    started_at TEXT,
    finished_at TEXT,
    input_count INTEGER,
    output_count INTEGER,
    latency_ms INTEGER,
    detail_json TEXT,
    error_message TEXT,
    FOREIGN KEY(pipeline_run_id) REFERENCES pipeline_run(id)
);

CREATE INDEX IF NOT EXISTS idx_pipeline_stage_run_run
ON pipeline_stage_run(pipeline_run_id);

CREATE INDEX IF NOT EXISTS idx_pipeline_stage_run_stage
ON pipeline_stage_run(stage);

-- ── AI Request Log ────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS ai_request_log (
    id TEXT PRIMARY KEY,
    pipeline_run_id TEXT,
    stage TEXT,
    model TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    prompt_version TEXT,
    input_tokens INTEGER,
    output_tokens INTEGER,
    latency_ms INTEGER,
    status TEXT NOT NULL DEFAULT 'PENDING',
    retry_count INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(pipeline_run_id) REFERENCES pipeline_run(id)
);

CREATE INDEX IF NOT EXISTS idx_ai_request_log_pipeline
ON ai_request_log(pipeline_run_id);

CREATE INDEX IF NOT EXISTS idx_ai_request_log_status
ON ai_request_log(status);

-- ── Full-Text Search (FTS5) ────────────────────────────────────────────────────
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
    knowledge_id UNINDEXED,
    title,
    summary,
    content,
    tags,
    tokenize = 'unicode61'
);

CREATE VIRTUAL TABLE IF NOT EXISTS session_fts USING fts5(
    session_id UNINDEXED,
    title,
    project_name,
    tokenize = 'unicode61'
);
