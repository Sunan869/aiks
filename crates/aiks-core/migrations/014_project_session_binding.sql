-- S3.1: explicit user-only project-memory reassignment.
-- This is a local display/grouping override; never rewrite provider source
-- metadata, canonical Session IDs or SiYuan document content.
CREATE TABLE IF NOT EXISTS project_session_binding (
    session_id INTEGER PRIMARY KEY,
    project_path TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES source_session(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_project_session_binding_path
    ON project_session_binding(project_path);
