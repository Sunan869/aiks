-- Stable publish routing captured with the durable operation intent.
-- This prevents a retry from changing the deterministic SiYuan path if
-- mutable knowledge metadata changes after the first remote write.
CREATE TABLE IF NOT EXISTS team_content_publish_target (
    company_id TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    category TEXT NOT NULL,
    PRIMARY KEY(company_id,operation_id),
    FOREIGN KEY(company_id,operation_id) REFERENCES team_content_operation(company_id,id) ON DELETE CASCADE
);
