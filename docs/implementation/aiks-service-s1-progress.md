# SDD ledger — plan: docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md

## Execution

User approved the S1 plan and inline implementation on 2026-09-22. Work is isolated to `feature/aiks-service-extraction`, starting at `8101587725261f13f878d26776e5b41407096724`. No merge, release or external service access.

Ruling: this runtime has neither Rust nor network access for git/dependency installation. GitHub read/write actions work. Use branch-scoped Git objects and read-only GitHub Actions for compilation and tests; no branch-writing helper, hidden background agent or invented local verification. All fixtures are synthetic.

Pre-flight interfaces: Task1 validated snapshot feeds Task3 accept; Task2 exclusive DB owns the same connection used by Task3 transactional enqueue; Task3 run/snapshot binding feeds Task4; Task5 fencing is required at every derived write used by Task4; Task6 must not initialize the legacy engine; Tasks8/9 must keep upload target identity immutable and never run a second worker. Do not mark a task complete until its test run is inspected.

## Tasks

- Task 1: in progress — contract tests added before implementation. Awaiting RED and baseline evidence.
- Tasks 2–6: pending — independent service batch.
- Tasks 7–10: pending — adoption, desktop wiring and final acceptance.

The additional workflow has read-only contents permission, runs tests from the exact pushed branch head, and cannot commit files or access user models or SiYuan workspaces. Normal CI remains unchanged.
