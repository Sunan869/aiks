-- P1 durable queue identity upgrade.
-- Columns are added conditionally by StateDb::run_migrations before this SQL runs.

-- Backfill any rows created by the previously dormant/early pipeline_job shape.
UPDATE pipeline_job
SET session_id = (
    SELECT ss.id
    FROM source_session ss
    WHERE ss.source = pipeline_job.source
      AND ss.external_session_id = pipeline_job.external_session_id
    ORDER BY ss.updated_at DESC
    LIMIT 1
)
WHERE session_id IS NULL;

UPDATE pipeline_job
SET pipeline_run_id = (
    SELECT pr.id
    FROM pipeline_run pr
    WHERE pr.session_id = pipeline_job.session_id
      AND pr.pipeline_version = 'v3'
    ORDER BY pr.updated_at DESC
    LIMIT 1
)
WHERE pipeline_run_id IS NULL AND session_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_pipeline_job_session_id
ON pipeline_job(session_id);

CREATE INDEX IF NOT EXISTS idx_pipeline_job_run_id
ON pipeline_job(pipeline_run_id);
