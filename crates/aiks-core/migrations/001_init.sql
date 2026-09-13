PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS source_session (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    external_session_id TEXT NOT NULL,
    source_path TEXT,
    project_path TEXT,
    project_name TEXT,
    title TEXT,
    source_updated_at TEXT,
    content_hash TEXT,
    parser_version TEXT,
    last_seen_at TEXT NOT NULL,
    is_missing INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(source, external_session_id)
);

CREATE TABLE IF NOT EXISTS sync_target (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL,
    sink TEXT NOT NULL,
    target_id TEXT,
    target_path TEXT,
    synced_hash TEXT,
    target_hash TEXT,
    last_synced_at TEXT,
    status TEXT NOT NULL,
    last_error TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(session_id) REFERENCES source_session(id),
    UNIQUE(session_id, sink)
);

CREATE TABLE IF NOT EXISTS source_file_state (
    path TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    file_size INTEGER,
    modified_at TEXT,
    last_offset INTEGER,
    file_hash TEXT,
    parser_version TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sync_run (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    trigger_type TEXT,
    discovered INTEGER DEFAULT 0,
    changed INTEGER DEFAULT 0,
    synced INTEGER DEFAULT 0,
    failed INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_source_session_source
ON source_session(source);

CREATE INDEX IF NOT EXISTS idx_source_session_updated
ON source_session(source_updated_at);

CREATE INDEX IF NOT EXISTS idx_sync_target_status
ON sync_target(status);

-- Knowledge extraction table (spec §24)
CREATE TABLE IF NOT EXISTS knowledge_extraction (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    external_session_id TEXT NOT NULL,
    source_content_hash TEXT NOT NULL,
    extractor_version TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    model TEXT NOT NULL,
    model_endpoint TEXT,
    knowledge_score REAL,
    category TEXT,
    status TEXT NOT NULL DEFAULT 'PENDING',
    knowledge_document_id TEXT,
    knowledge_hash TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(source, external_session_id)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_status
ON knowledge_extraction(status);
