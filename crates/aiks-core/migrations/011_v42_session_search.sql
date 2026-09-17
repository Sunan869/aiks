-- V4.2 AI session lexical + semantic search.
-- Retrieval chunks are intentionally separate from the large LLM extraction
-- chunks: search needs smaller passages while the extraction pipeline keeps
-- its existing context-sized chunks.

CREATE TABLE IF NOT EXISTS session_search_chunk (
    id TEXT PRIMARY KEY,
    session_id INTEGER NOT NULL,
    chunk_index INTEGER NOT NULL,
    text TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES source_session(id),
    UNIQUE(session_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_session_search_chunk_session
ON session_search_chunk(session_id);

CREATE TABLE IF NOT EXISTS session_embedding_record (
    id TEXT PRIMARY KEY,
    chunk_id TEXT NOT NULL,
    model TEXT NOT NULL,
    dimensions INTEGER NOT NULL,
    vector BLOB NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(chunk_id) REFERENCES session_search_chunk(id),
    UNIQUE(chunk_id, model)
);

CREATE INDEX IF NOT EXISTS idx_session_embedding_chunk
ON session_embedding_record(chunk_id);

CREATE INDEX IF NOT EXISTS idx_session_embedding_model
ON session_embedding_record(model);

CREATE TABLE IF NOT EXISTS session_index_state (
    session_id INTEGER PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'indexing', 'ready', 'failed')),
    indexed_hash TEXT,
    indexed_at TEXT,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    chunk_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    FOREIGN KEY(session_id) REFERENCES source_session(id)
);

CREATE INDEX IF NOT EXISTS idx_session_index_status
ON session_index_state(status);

CREATE VIRTUAL TABLE IF NOT EXISTS session_search_fts USING fts5(
    session_id UNINDEXED,
    external_id,
    source,
    title,
    project_name,
    content,
    tokenize = 'unicode61'
);
