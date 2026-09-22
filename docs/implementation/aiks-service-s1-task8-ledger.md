# SDD ledger — plan: docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md

Task 7 canonical 6026370: Linux Actions 35698888356 reports Core 358/0 and Service 20/0, Core/CLI/Service Clippy passed. Windows 35698888318 includes adoption tests, service real HTTP/process tests and CLI check, all passed. No production database was adopted; migration tests use a legacy-style fixture, not a real customer backup.

Task 8 RED setup: the exact client module lives in Desktop service_client; service integration tests compile this same source (not copied implementation) against the real HTTP runtime. It is deliberately nonfunctional until the behavioral tests pass. Full Tauri package testing and export/wiring remain required for Desktop delivery.

Ruling: client outbox uses its own application_id/schema and writer lease, rejects business databases before any schema writes. Credentials are runtime-only and never serialized into the upload queue. Each pending envelope fixes target instance/space, submission ID, revision and hash; retries never rebase. Only one unfinished generation per source session; changed scans defer until its predecessor is acknowledged or explicitly reconciled. Completed bodies are discarded but receipt/cursor metadata remains bounded.

Ruling: first-stage transport permits only numeric loopback personal origins, no redirect/proxy. Every upload verifies the expected capabilities identity before sending conversation bytes. A URL is not sufficient identity. Team connections and permission changes are not implemented by relaxing this policy.

Task 8 tests will prove durable unrecorded-ack retry, connection-switch isolation, bounded backoff, duplicate scan coalescing and business DB refusal. Current fixtures exercise a crash between server receipt and outbox ack (not a fabricated wire-disconnect claim). Task 9 supervisor/actual Tauri UI and Task 10 full integration remain pending.
