# SDD ledger — plan: docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md

## Task 6 continuation, 2026-09-22

- Baseline b714e54 / Actions 35692213531: 349 reported Core executions and 5 real HTTP/lifecycle Service tests passed; fmt/lock passed. Clippy reported two needless Ok/?.
- RED 2134c26 / Actions 35692582386: preserved user knowledge inherited a newer extraction's revision; dependency errors lacked content_unavailable/ai_disabled. Published-content fixtures omitted required SiYuan msg; the fixture was corrected instead of loosening the parser.
- GREEN runtime 956cf344 / Actions 35693332924: 350 reported Core executions and all 17 Service tests passed. Clippy then identified a duplicated test fixture module; no allow/suppression was added. The shared fixture is now imported once.
- RED 466e369 / Actions 35694123266, --no-fail-fast: Core 350/0; Service 17/3; Core+CLI+Service Clippy passed. Two new tests prove partial AI config can inherit legacy enablement/address and lacks explicit endpoint/model validation; a third proves CJK draft size was counted in SQLite characters rather than UTF-8 bytes. All three require fixes, not weaker expectations.

## Rulings implemented or under verification

Per-item source revision is required: only regenerated unpublished pipeline drafts advance. Preserved user-owned/published items do not inherit another item's age. Migration 015 does not guess legacy provenance; NULL remains unknown.

Assist authorizes the resource first, but checks disabled AI before any content/model request. Nonexistent resources remain 404. Canonical-read failures return content_unavailable without substituting SQLite content. Error text is fixed and safe.

The service configuration boundary is opt-in even for a partial [ai] table. It clears inherited model address/name and requires explicit enabled=true plus endpoint/model for activation. This does not change legacy Core/Desktop deployment defaults. Draft body budgets use byte length, including multibyte CJK/emoji.

Checks use real loopback HTTP, the actual Worker/AiStage, real SQLite transactions and real child processes. Semantic isolation is exercised with 4100 unauthorized vectors before authorized data in each corpus. No user-owned model, SiYuan or business database is accessed.

Task 6 final GREEN still requires applying the reviewed byte-budget blob and rerunning the canonical head. Tasks 7–10, Windows/full workspace/desktop service handoff, team authorization and installer acceptance remain pending. No main change, merge or release; no independent reviewer was available, and no subagent review is claimed.
