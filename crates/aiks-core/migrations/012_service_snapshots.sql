-- Additive Service S1 metadata. Run inside the caller's IMMEDIATE transaction.
-- Legacy identities and source_session uniqueness remain unchanged.
CREATE TABLE IF NOT EXISTS service_instance (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    instance_id TEXT NOT NULL UNIQUE,
    principal_id TEXT NOT NULL,
    personal_space_id TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS service_source_registration (
    id TEXT PRIMARY KEY,
    principal_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    source TEXT NOT NULL,
    registration_key TEXT NOT NULL,
    UNIQUE(principal_id, space_id, source, registration_key)
);

CREATE TABLE IF NOT EXISTS service_session_binding (
    session_id INTEGER PRIMARY KEY REFERENCES source_session(id),
    principal_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    registration_id TEXT NOT NULL REFERENCES service_source_registration(id),
    upstream_id TEXT NOT NULL,
    current_revision INTEGER NOT NULL DEFAULT 0
        CHECK(current_revision BETWEEN 0 AND 4294967295),
    UNIQUE(space_id, registration_id, upstream_id)
);

CREATE TABLE IF NOT EXISTS service_session_snapshot (
    id TEXT PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES service_session_binding(session_id),
    revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 4294967295),
    parser_version TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    canonical_json BLOB NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(session_id, revision)
);

CREATE TABLE IF NOT EXISTS service_job_input (
    pipeline_run_id TEXT PRIMARY KEY REFERENCES pipeline_run(id),
    snapshot_id TEXT NOT NULL REFERENCES service_session_snapshot(id),
    durable_job_id TEXT NOT NULL UNIQUE REFERENCES pipeline_job(id)
);

CREATE TABLE IF NOT EXISTS service_ingest_receipt (
    id TEXT PRIMARY KEY,
    principal_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    registration_id TEXT NOT NULL REFERENCES service_source_registration(id),
    upstream_id TEXT NOT NULL,
    submission_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    snapshot_id TEXT NOT NULL REFERENCES service_session_snapshot(id),
    pipeline_run_id TEXT NOT NULL REFERENCES pipeline_run(id),
    durable_job_id TEXT NOT NULL REFERENCES pipeline_job(id),
    created_at TEXT NOT NULL,
    UNIQUE(principal_id, space_id, registration_id, upstream_id, submission_id)
);

CREATE INDEX IF NOT EXISTS idx_service_binding_scope
    ON service_session_binding(principal_id, space_id);
CREATE INDEX IF NOT EXISTS idx_service_snapshot_session
    ON service_session_snapshot(session_id, revision);
