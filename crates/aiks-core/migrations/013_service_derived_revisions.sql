-- Additive provenance for derived state. Never guess a legacy result's revision.
-- A NULL or non-current revision is not a current result. Compare to binding in
-- the same query/transaction; accepting a revision immediately makes older data stale.
CREATE TABLE IF NOT EXISTS service_derived_state (
    session_id INTEGER PRIMARY KEY REFERENCES service_session_binding(session_id),
    indexed_revision INTEGER CHECK(indexed_revision BETWEEN 1 AND 4294967295),
    knowledge_revision INTEGER CHECK(knowledge_revision BETWEEN 1 AND 4294967295),
    completed_revision INTEGER CHECK(completed_revision BETWEEN 1 AND 4294967295)
);
