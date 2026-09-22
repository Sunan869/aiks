# SDD ledger — plan: docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md

## Execution

User approved the S1 plan and inline implementation on 2026-09-22. Work is isolated to `feature/aiks-service-extraction`, starting at `8101587725261f13f878d26776e5b41407096724`. No merge, release or external service access.

Ruling: this runtime has no usable Rust toolchain/network for local compilation. GitHub read/write actions work. Use branch-scoped Git objects and read-only GitHub Actions for compilation and tests; no branch-writing helper, hidden background agent or invented local verification. All fixtures are synthetic. A missing local toolchain is not a missing repository permission.

Pre-flight interfaces: Task1 validated snapshot feeds Task3 accept; Task2 exclusive DB owns the same connection used by Task3 transactional enqueue; Task3 run/snapshot binding feeds Task4; Task5 fencing is required at every derived write used by Task4; Task6 must not initialize the legacy engine; Tasks8/9 must keep upload target identity immutable and never run a second worker.

## Tasks

- Task 1: complete — implementation `3c18a169b9fd90b06f69dc0141418a7c6f6195fe`. Before implementation, run 35683547771 passed baseline Core tests/CLI check and failed at the missing `aiks_core::service` contract. After implementation, run 35684190514 passed all Core tests (including the eight new contract cases), Core/CLI Clippy and formatting. This is not Service executable verification.
- Task 2: in progress — real SQLite identity/legacy-preservation and cross-process lease tests precede implementation. Awaiting RED.
- Tasks 3–6: pending — independent service batch.
- Tasks 7–10: pending — adoption, desktop wiring and final acceptance.

Ruling: explicit recursive key ordering and pre-serialization JSON depth/node budgets avoid depending on HashMap iteration or allocating an unbounded serialized body. Source paths and URLs remain inert data. Existing model serialization/source keys are untouched.

The additional workflow has read-only contents permission, runs tests from the exact pushed branch head, and cannot commit files or access user models or SiYuan workspaces. Normal CI remains unchanged.
