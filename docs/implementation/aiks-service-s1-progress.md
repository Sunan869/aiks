# SDD ledger — plan: docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md

## Execution and boundaries

User approved the S1 plan and inline implementation on 2026-09-22. Work is isolated to `feature/aiks-service-extraction`, starting at `8101587725261f13f878d26776e5b41407096724`. No merge, release or external service access.

GitHub repository writes and Actions execution have been exercised successfully. This execution environment has no Rust toolchain and cannot fetch the repository for a local build; the exact pushed branch head is compiled and tested by a read-only GitHub Actions workflow. There is no branch-writing helper or autonomous background implementer. All fixtures use synthetic data and temporary directories.

Task interface dependencies: validated input feeds atomic ingestion; exclusive StateDb owns the transaction used to persist snapshot/run/job/receipt; immutable run-to-snapshot bindings feed the upcoming Worker input refactor; revision fencing must guard every derived write before snapshot Worker deployment; the HTTP entry must not initialize a second legacy engine; collector outboxes must remain bound to the original target instance and space.

## Implemented core increments

### Task 1 — snapshot contracts and validation

Implementation: `3c18a169b9fd90b06f69dc0141418a7c6f6195fe`.

RED: run 35683547771 first passed baseline Core tests and CLI check, then failed because the new tests referenced the absent service module. GREEN: run 35684190514 passed Core tests (including eight service-contract cases), Core/CLI Clippy and formatting.

The request rejects unsupported versions, incomplete snapshots, invalid IDs and exceeded message/block/byte/JSON budgets. Explicit recursive key ordering keeps fingerprints stable without relying on HashMap order. Source paths and URLs are inert data. Debug output does not print private payloads. Existing source keys and model serialization remain unchanged.

### Task 2 — persistent identity and cooperative writer lease

Implementation: `90a6a5ca0b044e60e42bba4d7a3e64f0006fde39`.

RED: run 35684494685 failed at missing ServiceStore/open_exclusive. Run 35684792367 passed Core tests, including the new identity/legacy-preservation and actual subprocess lock/crash tests, and passed Core/CLI Clippy. Its two rustfmt layout differences were corrected in `cd253813b1e24774f6275243671d05eaebb48f67`.

This adds migration 012 without changing old migrations, retains existing session IDs and SiYuan mappings, and does not implicitly adopt legacy sessions into a service space. A ServiceStore requires a held exclusive lease. The lock is cooperative; it does not stop unrelated SQLite programs. Runtime conversion of legacy Desktop/CLI to this ownership API is still Task 9. Windows service-specific lock/junction execution remains part of final platform verification, not the Linux result above.

### Task 3 — atomic ingestion and immutable receipts

Implementation: `d70a99691650ea12b1a6eeb195f4fd809bf2b70b`.

RED: run 35685016777 failed at missing accept/register_source methods. Run 35685524897 passed all Core tests, including the nine new ingestion cases, and Core/CLI Clippy; only the enqueue_in_tx signature formatting failed. The current closing commit applies that exact rustfmt correction. Use its subsequent run for the final checkpoint status, rather than describing the intermediate run as fully green.

A single IMMEDIATE transaction owns session identity, revision CAS, immutable snapshot, pipeline run, shared durable enqueue, job-input binding and receipt. Abort triggers at both job and receipt insertion prove whole-transaction rollback. Concurrent duplicate requests share a receipt. Reusing a submission ID with changed content/expected revision is a conflict. A receipt can be replayed after later revisions. Unchanged content retains its current work while assigning a separate receipt to a new submission. Registrations namespace upstream IDs and cannot use another service's trusted context. Mutated ValidatedSnapshot bytes are revalidated before persistence.

Implementation ruling: the accept boolean denotes newly queued work; an unchanged-content new receipt returns false. HTTP receipt status must not interpret this as extraction completion. Immutable revision runs intentionally do not call the legacy pipeline-run upsert, which rebinds runs by pipeline version. Durable enqueue itself remains one shared implementation with an exact-run variant for snapshot revisions. The A -> B -> A regression proves the third revision cannot attach to the first still-running job merely because hashes match.

## Not yet implemented / release gates

- Tasks 4–6: persisted-snapshot Worker input, same-transaction revision fencing, independent HTTP executable and authenticated read/search routes.
- Tasks 7–10: explicit old-data adoption, collector outbox, Desktop/CLI ownership and HTTP cutover, Windows/full workspace/frontend/deployment acceptance.
- There is still no `apps/aiks-service` executable. Existing legacy Worker input has not been converted; do not run newly ingested jobs with the legacy Provider loader and call it a working service.
- Cargo.lock still needs the exact Cargo-generated dependency delta committed and a --locked verification. Actions currently resolves fs2 0.4.3 plus the pre-existing Desktop base64/reqwest manifest drift and records the resulting lock diff. No locked/reproducible-build claim is made yet.
- No team listener, ACL, public SiYuan proxy, model credential distribution, installer release, full offline smoke test or RAG completion is claimed.

The `Service S1 contracts` workflow uses contents:read, checkout without persisted credentials, and tests the exact pushed head. It uploads synthetic test logs only, does not commit to a branch, and cannot access the user's models or SiYuan workspace. Normal CI remains unchanged. The upcoming HTTP service must remain loopback-only until later authenticated multi-user scope and internal-content authorization are implemented.
