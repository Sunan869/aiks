# SDD ledger — plan: docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md

Task 7 canonical 6026370: Linux Actions 35698888356 reports Core 358/0 and Service 20/0, Core/CLI/Service Clippy passed. Windows 35698888318 includes adoption tests, service real HTTP/process tests and CLI check, all passed. No production database was adopted; migration tests use a legacy-style fixture, not a real customer backup.

Task 8 RED setup: the exact client module lives in Desktop service_client; service integration tests compile this same source (not copied implementation) against the real HTTP runtime. It is deliberately nonfunctional until the behavioral tests pass. Full Tauri package testing and export/wiring remain required for Desktop delivery.

Ruling: client outbox uses its own application_id/schema and writer lease, rejects business databases before any schema writes. Credentials are runtime-only and never serialized into the upload queue. Each pending envelope fixes target instance/space, submission ID, revision and hash; retries never rebase. Only one unfinished generation per source session; changed scans defer until its predecessor is acknowledged or explicitly reconciled. Completed bodies are discarded but receipt/cursor metadata remains bounded.

Ruling: first-stage transport permits only numeric loopback personal origins, no redirect/proxy. Every upload verifies the expected capabilities identity before sending conversation bytes. A URL is not sufficient identity. Team connections and permission changes are not implemented by relaxing this policy.

Task 8 tests prove durable unrecorded-ack retry, connection-switch isolation, bounded backoff, duplicate scan coalescing and business DB refusal. Current fixtures exercise a crash between server receipt and outbox ack (not a fabricated wire-disconnect claim).

## 2026-09-22 collector checkpoint

RED observed on immutable 5afd939, Linux run 35700808625: Core 358 reported passing executions; Service 26 passing and three failing collector cases. The failures were the Retryable collector/delivery placeholders, not external model availability. All three assertion failures were inspected before applying the implementation.

Implementation c6c937ed24194fc221850df9bf696a932af61545 replaced those placeholders, wired the canonical nested module in the Service contract tests, and added persistent source-registration mappings in the client-only outbox. No transformation script remains in this commit.

GREEN on c6c937e, Linux run 35702330063: Core 358 reported passing executions / 0 failed; Service 29 / 0 failed; Core/CLI/Service all-target Clippy passed; Cargo.lock unchanged. The only failed workflow step was rustfmt. Counts include the storage subprocess probe, so they are reported executions rather than a claimed count of unique test functions.

Windows run 35702329712 on c6c937e completed successfully: service storage/ownership/adoption/revision contracts, the complete Service HTTP/process/client test suite, and CLI compilation. This is not a graphical desktop or installer test.

The three collector cases now passing are:
- real_provider_is_sanitized_before_queueing_and_survives_source_removal: real Continue capture, sanitization before durable queueing, removal of local source/project paths, coalesced duplicate scan, source directory deletion, real Service delivery/worker completion/search and persisted acknowledgement.
- a_known_registration_can_queue_offline_and_excluded_sessions_are_not_uploaded: cached registration works while the service is unavailable; explicit exclusions are not queued.
- malformed_legacy_jsonl_is_not_labeled_a_complete_snapshot: partial Claude JSONL is rejected before any upload, preserving server state.

Formatter candidate was generated from exactly c6c937e, check_exit=0; all old/new blob hashes were checked against the source snapshot and downloaded candidate. Only the three reviewed Rust formatting changes are applied at this checkpoint; no protocol or test expectations are changed.

## Remaining acceptance gates

Task 8 transport/outbox/collector behavior is green through the real Service harness. The native Tauri package still needs module export/wiring and full package verification; it is not yet a usable service-mode desktop workflow. Do not mark the whole Task 8 desktop deliverable or S1 complete solely from the shared-source harness.

Task 9 remains: controlled native service supervisor and bootstrap, mutually exclusive legacy/service_local startup, legacy Desktop/CLI writer ownership, controlled IPC actions, real Service status/search/receipt UI with no legacy/mock fallback.

Task 10 remains: full workspace and frontend quality gates at an immutable HEAD, complete offline/model/content integration, final branch review and delivery documentation. No main merge, release, real database adoption, live model call or user-data scan has been performed.
