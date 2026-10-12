-- S3.2 local report drafts. Additive and idempotent on old personal databases.
-- No SiYuan document is overwritten or sent externally by this schema.
CREATE TABLE IF NOT EXISTS project_report_draft (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    from_date TEXT NOT NULL,
    through_date TEXT NOT NULL,
    source_filter TEXT,
    current_revision INTEGER NOT NULL DEFAULT 0 CHECK(current_revision >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_project_report_draft_project
    ON project_report_draft(project_id,updated_at DESC,id DESC);
CREATE TABLE IF NOT EXISTS project_report_revision (
    draft_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision >= 1),
    body TEXT NOT NULL,
    saved_at TEXT NOT NULL,
    PRIMARY KEY(draft_id,revision),
    FOREIGN KEY(draft_id) REFERENCES project_report_draft(id)
);
