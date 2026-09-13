-- V4: Persistent Pipeline Job Queue
-- Enables reliable job queuing, restart recovery, deduplication and attempt tracking

CREATE TABLE IF NOT EXISTS pipeline_job (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    external_session_id TEXT NOT NULL,
    source_hash TEXT NOT NULL DEFAULT '',
    generation INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'PENDING',
    attempt INTEGER NOT NULL DEFAULT 0,
    available_at TEXT,
    lease_until TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(source, external_session_id, generation)
);

CREATE INDEX IF NOT EXISTS idx_pipeline_job_status
ON pipeline_job(status, available_at);

CREATE INDEX IF NOT EXISTS idx_pipeline_job_session
ON pipeline_job(source, external_session_id);
