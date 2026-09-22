# SDD ledger — plan: docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md

## Task 6 continuation, 2026-09-22

Implementation has progressed beyond the obsolete 36125a2 patch. Use this branch's canonical HEAD, never apply the old artifact to it.

- b714e54 / Actions 35692213531: real HTTP ingestion/query and child-process tests passed; Clippy identified two redundant result wrappers.
- 2134c26 / Actions 35692582386: RED for preserved knowledge inheriting a newer extraction's age, and missing content_unavailable/ai_disabled contracts. SiYuan fixture msg fields were corrected without weakening the parser.
- 956cf344 / Actions 35693332924: Core 350 reported executions and 17 Service cases passed. A duplicated fixture import was removed, not suppressed.
- 466e369 / Actions 35694123266: RED for partial AI configuration accidentally enabling legacy defaults and UTF-8 body budgets counting characters.
- cb038aa / Actions 35694568959: opt-in configuration tests passed; byte-budget candidate still needed applying. The old discovery-cache test also failed.
- f15e929 / Actions 35696177322: applied the exact UTF-8 byte-budget source blob. All 20 Service cases and Core/CLI/Service Clippy passed. A new gated discovery-cycle test reliably failed (2 scans instead of 1 while the first load is still blocked); formatting of that new test also required correction.
- This commit applies the reviewed minimal cache correction and rustfmt output, removes the one-shot transformer, and adds read-only Windows runtime verification. Its canonical CI result is still required before calling Task 6 accepted.

## Rulings

Per-item source provenance belongs to regenerated unpublished drafts only; preserved user/published knowledge must not inherit another item's age. Unknown legacy data stays unknown until explicit adoption.

Assist authorizes the resource first and rejects disabled AI before any upstream calls. Published-body failures return content_unavailable and never substitute SQLite projections. Partial model tables stay opt-in with explicit endpoint/model validation. Body limits use UTF-8 byte length.

A missing claimable job is not idle while the JoinSet still contains tasks. The cache now survives active loads and is cleared only after both work and pending claims drain. The gated regression also asserts a genuinely later idle cycle refreshes metadata; no infinite cache or timing-only assertion replaces correctness.

Task 6 tests exercise actual Worker, SQLite, loopback HTTP, fixed canonical content reads, scoped lexical/semantic recall and child processes. Synthetic vector tests place 4100 unauthorized candidates before authorized entries. No private external model/content store or user database is accessed.

Tasks 7–10 remain pending: explicit old-data adoption, collector outbox, Desktop handoff, full workspace and packaging acceptance. S1 is still personal numeric-loopback only; team roles and RAG are later milestones. No main change, merge or installer release. No independent/subagent review is claimed.
