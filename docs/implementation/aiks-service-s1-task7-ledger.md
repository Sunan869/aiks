# SDD ledger — plan: docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md

Task 6 Linux canonical b534a7e / Actions 35696712280 passed Core, Service, Clippy, fmt and locked dependency checks. Windows #1 found an oversized-body TCP reset; the bounded authenticated drain is tested by the original real-HTTP assertion, not by permitting connection errors. Unix-only test imports were scoped, not silenced.

Task 7 RED: 5b50328 / Actions 35697659360 ran the real migration fixtures. All seven adoption assertions failed against an explicit nonfunctional contract stub (Unavailable); all 351 existing reported Core executions and 20 Service cases passed; Core/CLI/Service Clippy passed. This is behavioral RED, not just a missing import.

Ruling: adoption has one transaction and reuses ingestion's receipt/revision/job transaction primitive. Caller must stop legacy runtimes and back up the coherent database + SiYuan workspace before invoking it. Core requires an exclusive lease, matching persistent local identity and no RUNNING jobs. This internal function is not an HTTP admin endpoint and does not claim to produce backups.

Ruling: unavailable source data keeps old IDs/maps/jobs and an explicitly unverified historical projection; it never manufactures messages or source revisions. Optional complete input uses the original source_session ID, retires old pending generations through the established enqueue algorithm, and records their disposition.

Ruling: standalone manual/published documents need explicit ownership independent of a nonexistent source session. The new mapping only authorizes source_session_id IS NULL; it never overrides a foreign session binding. Old per-item source provenance remains unknown, never automatically revision one. Old/unverified documents are readable but not advertised as current search evidence before reindex/adoption proves freshness.

Task 7 implementation and exact shared-path wiring are under verification. Tasks 8–10 and team/RAG remain pending. No main, merge, release, user database mutation, remote model call or independent-review claim.
