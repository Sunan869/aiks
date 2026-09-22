# S1 Task 5 checkpoint and Task 6 execution

Work remains on feature/aiks-service-extraction. No main changes, merge, release or real user model/SiYuan access.

Task 5 runtime RED: bb54844 / Actions 35687815540 reproduced late AI results incorrectly becoming DONE, both changed-content and title-only cases. c310ca1 added missing guarded-write API tests.
Task 5 implementation: 069d92c / Actions 35688888258 passed Core tests (347 reported executions including the subprocess lease probe), Core/CLI Clippy, rustfmt and unchanged locked Cargo.lock. Tests cover guarded knowledge/chunk writes, initial/shortcut/final index writes, delayed embeddings, safe error typing, transactional rollback and valid current completion.
Additional retry/expired lease restart and stable status tests passed Core tests and Clippy at a44816c / Actions 35689142829; that run only failed formatting. This commit applies exactly the generated rustfmt blob. Desktop existing-page SSR status contracts, all frontend tests and production build passed Actions 35689142788 at a44816c.

Ruling: source revisions and derived revisions are separate; migration 013 records indexed/knowledge/completed revisions, NULL means unknown, and accepting a newer revision makes older provenance stale by comparison. Do not delete already published or user-owned knowledge. Task 6 must not expose old snippets combined with new titles as current.
Ruling: HTTP new receipt versus replay cannot use the existing accept bool, which means newly queued work. Expose receipt-creation classification from the same transaction; never infer it with a racy pre-read.
Ruling: authenticated S1 search must apply the trusted personal-space predicate before candidate limits in all existing lexical/substring/metadata/semantic paths. Reuse existing algorithms; do not call unrestricted search then hide unauthorized rows in the browser.
Ruling: source code was obtained through a read-only Actions git-archive artifact into an isolated local Git working copy after ordinary clone failed DNS. Compilation remains on GitHub Actions; no claim of a local Rust run is made.

Task 6 is in RED setup: apps/aiks-service is only a failing scaffold until the real router/runtime/auth/ingestion/query and lifecycle tests pass. The existence of its manifest or health endpoint alone is not completion. Tasks 7–10 remain pending; Windows, full desktop service cutover, adoption and installer acceptance are not inferred from Linux Core tests.
