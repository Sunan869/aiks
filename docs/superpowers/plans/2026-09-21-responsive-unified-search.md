# Responsive Unified Search Implementation

User-approved scope: correct the pending-search disabled notice, return useful lexical hits before semantic completion, reduce unnecessary database work, cancel obsolete interactive searches, cache query embeddings, and expose stage timings.

## Implemented design

The Core `UnifiedSearchService` owns lexical recall, semantic recall and final RRF fusion. `search_with_progress` publishes a lexical snapshot before awaiting the query embedding. Existing `search` callers delegate to that same method without a callback. Owned database reads run on blocking workers rather than the async executor.

Knowledge and session lexical recall use FTS MATCH with escaped prefix terms. The FTS virtual table drives an indexed primary-key lookup, replacing the unindexed session LEFT JOIN. Project/source filters precede the candidate limit. CJK/technical identifiers and zero-match queries retain parameterized substring fallback; missing FTS explicitly reports degradation. One corpus failure no longer erases results from another corpus.

Desktop transports partial results through a caller-scoped Tauri Channel. Optional command arguments preserve existing callers. A process-local coordinator keeps a monotonic request watermark per calling window; new searches and explicit cancellation drop old search futures. A cancellation received before its search prevents that search from starting. Late completions cannot clear a newer request.

A provider-configuration-scoped, process-local cache stores at most 64 successful query vectors for five minutes. Failed, empty, non-finite and dimension-mismatched vectors are not cached. Document results are never cached, so edits are not hidden by query-vector caching. Configuration identities and query text are not logged.

The query embedding wait budget is eight seconds; timeout returns available lexical hits with an explicit warning. Index rebuild/pipeline timeouts are unchanged. The existing 4096 vector candidates per corpus remain bounded; this patch is not an ANN implementation.

The dialog represents unknown semantic state by an absent flag, not false. It only shows the disabled notice for a completed successful current query. During enrichment it retains the lexical results and uses a distinct progress message; late partials and obsolete results are ignored.

## Verification

Behavioral RED: CI #524, commit cec78bfc128d9af0565111d6e0070ed76c5eeb67. The three new Core regressions failed on the intended assertions (FTS warning, corpus isolation and query deadline), and the loading-state regression failed on the old false default. Formatting and Clippy passed at this baseline.

Additional tests cover progressive callback delivery, Chinese/technical recall, filters, cache capacity/expiry/isolation, cancellation ordering/window scope, and late frontend messages. See PR #45 for the latest exact-head full CI result; do not infer that result from this document.

A synthetic SQLite 3.46.1 probe with 1500 sessions compared the old session SQL against the new FTS query. One local run took 517.682 ms for the old SQL (1500 full rows) and 1.520 ms for indexed recall (75 matching snippets). The new plan used FTS MATCH plus `SEARCH ss USING INTEGER PRIMARY KEY`; the old plan scanned an unindexed virtual table for each session. This is a SQL-stage synthetic experiment, not a real-device or end-to-end speed claim.

## Remaining limitations and acceptance

Substring fallback can still scan text, particularly for Chinese queries; it is not an indexed Chinese tokenizer. Vector recall still uses the existing bounded exact rerank, not all-library ANN. Cancellation cannot preempt an already-running SQLite statement or guarantee that a remote GPU stopped inference. The eight-second deadline bounds embedding wait, not database work.

On a real installation, test `llm`, Chinese and technical identifiers, compare time to first lexical results with time to final hybrid results, repeat a query to exercise cache, and try fast input changes or closing the dialog. `[SEARCH_TIMING]` records lexical/embedding/vector/fusion/total durations without query or document content. No data reset, model-default change, dependency upgrade or installer release is part of this patch.
