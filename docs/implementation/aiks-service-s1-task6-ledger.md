# SDD ledger — plan: docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md

## Task 6 continuation, 2026-09-22

- Baseline b714e54: Actions 35692213531 executed 349 reported Core tests and 5 real HTTP/lifecycle Service tests successfully. Formatting and unchanged lock passed. Clippy found two needless Ok/?. No main/merge/release changes.
- RED 2134c26: Actions 35692582386 reproduces preserved user knowledge incorrectly becoming current after a new extraction. It also reproduces generic unavailable instead of content_unavailable/ai_disabled. Published-content success/revocation fixtures initially omitted mandatory SiYuan msg; fix the fixture envelope rather than loosening the canonical parser.
- Ruling: per-item source revision is required — a session extraction can preserve old user-owned/published rows — only regenerated unpublished pipeline drafts advance; no guessed legacy backfill. Unknown remains stale until explicit adoption/publication proves origin.
- Ruling: authorize an assist resource before reporting ai_disabled, but test the disabled capability before any content/model call — non-existent remains 404, disabled existing resources make zero upstream requests.
- Ruling: canonical-read failures return content_unavailable, never SQLite projection as an apparent published body. All messages stay fixed/safe.
- Current patch is a SHA-guarded candidate; generated blobs must be explicitly reviewed and applied, then canonical tests/Clippy/fmt rerun. Prepare success is not acceptance.
- Task 6 remains in progress until the new tests and complete checks pass. Tasks 7–10 and full desktop/Windows/install acceptance are still pending.
