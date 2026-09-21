# Responsive Unified Search Implementation Plan

> Execute inline with regression tests and the existing CI gates. User approved this scope in the search-latency debugging conversation on 2026-09-21.

**Goal:** Correct the false disabled-semantic notice and show useful local results without waiting for a slow embedding endpoint.

**Architecture:** Keep retrieval and fusion in the canonical Core search service. Add a lexical-result callback, indexed FTS recall with parameterized substring fallback, and a query-only embedding deadline. The Desktop command owns request cancellation and an in-memory, provider-scoped query-vector cache. Caller-scoped Tauri channels deliver partial results; the existing final-result API remains compatible.

**Constraints:** No upstream data writes, destructive reset, dependency upgrades, or ANN replacement. Preserve Chinese/technical-term recall, project/source filters, explicit degradation, and bounded vector candidates. Never log query text, document contents, endpoint credentials, or cache keys.

## Tasks

- [ ] Reproduce the pending-state UI issue and Core fallback/deadline failures against main using synthetic fixtures.
- [ ] In `crates/aiks-core/src/search/`, introduce indexed lexical recall, independent corpus errors, a progressive callback, a query-only 8-second embedding wait budget, and content-free timings. Preserve existing RRF and semantic bounds.
- [ ] In `apps/aiks-desktop/src-tauri/src/search_commands.rs`, reuse a provider-scoped query cache, deliver partial results via Channel, and cancel obsolete requests per calling window. Register managed state and cancellation in `lib.rs`.
- [ ] In Desktop API/types and the search dialog, wire AbortSignal and partial-result callbacks. Unknown/loading/error must never mean disabled. Preserve partial lexical results if semantic enrichment fails.
- [ ] Add behavioral tests for progression, query caching, cancellation ordering, filters, Chinese/technical queries, FTS fallback, and dialog notices. Verify full workspace formatting/Clippy/tests, Windows compile, frontend tests/build and hygiene on the exact PR head.
- [ ] Record the verified SHA and CI result in PR #45. Leave main unchanged pending integration approval.

## Review focus

Cancellation can arrive before the corresponding invocation or after a newer one; an old completion must not clear a new request. Partial IPC messages may arrive after the final response and must be ignored. FTS syntax must never be built from unescaped input. A failed corpus must not discard another corpus's results. Query caching must not cache errors, cross provider configurations, or make document edits invisible.

## Acceptance

On a real installation, repeat the `llm` query and Chinese/technical queries. Check time to first lexical results independently from time to semantic completion; then exercise slow/unavailable embedding, rapid input changes, and closing the dialog. CI fixtures are not a claim of real-device speed. An 8-second embedding deadline does not bound synchronous database work, and dropping the HTTP future cannot guarantee cancellation of remote GPU execution.
