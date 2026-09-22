# AIKS Service S1 — current execution status

Updated 2026-09-22. This is the current status index; the task-specific ledgers and Git history retain earlier RED/GREEN checkpoints. Do not treat their older pending-task lists as the current branch state.

Plan: `docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md`.
Spec: `docs/superpowers/specs/2026-09-22-aiks-service-extraction-design.md`.
Branch: `feature/aiks-service-extraction`; base `8101587725261f13f878d26776e5b41407096724`.

## Current milestone

Task 8 collector/transport/outbox behavior is implemented, linked into the actual Tauri library, and tested on Linux and Windows. The complete service-mode Desktop workflow is NOT implemented yet. Task 9 startup/supervisor/IPC/UI and Task 10 end-to-end acceptance remain. S1 must not be described as complete or ready for an installer release.

| Task | Actual implementation status |
| --- | --- |
| 1. Snapshot contracts and validation | Implemented; protocol, complete-input, identity and byte/depth budgets tested. |
| 2. Service identity and exclusive writer primitive | Implemented; real subprocess ownership/crash recovery tested. Legacy Desktop/CLI integration of this lease is still Task 9. |
| 3. Atomic snapshot ingestion and receipts | Implemented; idempotency, revision CAS, rollback, immutable receipts and ABA generations tested. |
| 4. Persisted-snapshot Worker | Implemented; no service-side Provider discovery or path fallback; source deletion and restart recovery tested. |
| 5. Transactional revision fences | Implemented; late model/vector results cannot overwrite newer derived state; current valid generations still complete. |
| 6. Independent personal Service | Implemented; authenticated numeric-loopback API, bounded inputs, process lifetime, scoped queries, fixed canonical-content access and opt-in model use tested. |
| 7. Explicit legacy-state adoption | Implemented internal Core operation; IDs, maps, published/user content, unavailable sources and rollback tested. No real user database has been adopted; this function does not create backups for the user. |
| 8. Native collector, transport and outbox | Implemented and linked; real Service tests and actual Tauri-library tests are green. User-facing conflict reconciliation and controlled invocation are not a delivered UI. |
| 9. Desktop first HTTP workflow | Pending: mutually exclusive startup modes, fixed-binary supervisor, bootstrap verification, legacy writer handoff, controlled IPC and real status/search/receipt UI. |
| 10. Final S1 integration and review | Pending. A full workspace regression has passed, but the final complete workflow, offline model/content scenario, final review and acceptance docs cannot precede Task 9. |

## Latest verified checkpoints

### c6c937ed24194fc221850df9bf696a932af61545 — real collector delivery

Linux Actions `35702330063`: Core 358 reported passing test executions / 0 failed; Service 29 / 0 failed; Core/CLI/Service all-target Clippy passed and Cargo.lock unchanged. The only failing step at this implementation checkpoint was formatting, subsequently corrected.

Windows Actions `35702329712`: service ownership/adoption/revision contracts, complete Service HTTP/process/client tests, and CLI compilation passed.

The three previously failing collector assertions at `5afd939` now pass: real Continue capture and pre-queue redaction followed by source-directory deletion, delivery/Worker completion/search; cached-registration offline collection with explicit exclusions; rejection of malformed Claude JSONL before any upload.

### c389d7d62218df751227c069dbb4f50d358917fa — actual native library validation

Actions `35703577335` completed successfully on both platforms:
- Linux and Windows: `cargo test --locked -p aiks-desktop --test service_client -- --test-threads=1` passed both tests. These link `aiks_desktop_lib::service_client`, not a path-imported substitute for the native library.
- Linux: `cargo clippy --locked --workspace --all-targets -- -D warnings` passed with no warning/error lines in its saved log.
- Linux: `cargo test --locked --no-fail-fast --workspace -- --test-threads=1` passed. Saved result summaries report 424 passing executions, 0 failed, 0 ignored; this includes the subprocess writer probe, not 424 claimed distinct test functions.
- Both runners: tracked source and Cargo.lock remained unchanged. Windows does not claim to have run the Linux-only full-workspace step.

Frontend Actions `35703577297` passed the existing frontend tests and production build. This verifies regression compatibility, not a new ServiceStatusPage: that page is Task 9.

The closing formatting change is the reviewed rustfmt-only blob for the native test file, generated from exactly c389d7d with check_exit=0. Use the subsequent canonical HEAD's Actions for its own final check status; do not label an in-progress run green based on the previous commit.

## Boundaries and remaining work

All work is on the feature branch. No main update, merge, release, real database adoption, private conversation scan, or live model/SiYuan call was performed. Rust commands run on GitHub Actions because this working environment has no Rust toolchain or direct repository clone access. Synthetic loopback services and temporary files are used by the tests.

The new outbox is a separate client database, refuses business StateDb files, fixes instance/space/submission/revision/hash at enqueue, and never changes that target on a connection switch. Transport rejects redirects/proxies/unsafe origins and verifies the expected personal instance before sending conversation bytes. Already registered sources can be captured while the Service is unavailable; first-time source registration still requires the intended Service.

A conflicting upload remains blocked and never blindly changes expected_revision. An unrecorded successful acknowledgement is recovered by replaying the immutable submission. Full user-facing receipt/version reconciliation is not implemented by pretending every HTTP 409 is a successful replay.

The native module export does not start a second Worker or change legacy startup. The Service-local Desktop supervisor and legacy Desktop/CLI exclusive-writer adoption must be wired before enabling the new mode. Do not run the old GUI concurrently against a Service-owned business database and claim the handoff is complete.

CI's SiYuan resource file is a test-only placeholder, not an installer or a graphical runtime acceptance test. Team ACLs, remote listeners, RAG, complete packaged deployment and automatic old-data migration are not S1 checkpoint claims. No independent reviewer or completed whole-branch review is claimed.
