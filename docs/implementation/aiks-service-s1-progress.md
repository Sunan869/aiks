# AIKS Service S1 — implementation and verification index

Updated 2026-09-22. Branch `feature/aiks-service-extraction`; main synchronized by PR47/63c23a2. The S1 plan is `docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md`; architecture spec is the corresponding specs document. Older task ledgers remain historical evidence, not current pending lists.

## Implementation boundary

Tasks 1–8 supply snapshot contracts, service identity/writer ownership, atomic ingestion/receipts, durable snapshot Worker, revision-safe derived writes, personal HTTP runtime, explicit internal legacy adoption, and client-only outbox/transport/collector.

Task 9 now supplies strict legacy/service_local selection, mutually exclusive startup, fixed-path owned Service supervision with private bootstrap, native controlled actions, actual ServiceStatusPage, proper tray routes, and `scripts/dev.ps1` orchestration. Source scanning never occurs on the server. Startup cancellation and cleanup serialize on a lifecycle gate; locating Service runtime assets cannot install a bridge into the legacy workspace. Both legacy Desktop and CLI take the shared writer lease before business writes.

Task 10 now includes real child-process/client/Provider/model processing, source deletion, canonical publisher/index/body integration, real restart/identity behavior, and a loopback-only Linux network namespace test. Its final all-green claim must be checked against the final immutable HEAD after formatting/docs, not inferred from earlier runs.

User guide and scope: `docs/implementation/aiks-service-s1.md`.

## Observed checkpoints

- 63c23a2 / 35706022816: merged-main baseline verified before Task 9.
- 32ed6bb / 35707393821: new backend enum and owned shutdown behavior failed as expected before implementation.
- cf5756c / 35710705189: Core365/0, Service33/1. Only bounded collector progress failed: [3,0,0] instead of [3,3,1].
- 8ad3360: persisted a scan cursor separate from upload acknowledgement, preserving fixed instance/space/submission/revision semantics.
- 011eeff / 35711836122: Core365/0 and Service35/0; only rustfmt failed in this workflow. Real owned supervisor, parent-pipe lifetime and offline publisher flow passed.
- 011eeff / 35711836120: 101 frontend tests passed; four newly added wiring regressions failed for legacy-workspace side effects, startup cancellation, wrong tray routing and overbroad local-only wording.
- 095bc89 / 35712721461: those four frontend regressions passed with the complete frontend suite and production build.
- 095bc89 / 35712721489: Linux/Windows native client contracts and Linux full-workspace tests/Clippy passed. The native controller gained additional lifecycle/configuration tests exercised by the full suite.
- 095bc89 / 35712721455: real offline integration passed in an isolated namespace with no non-loopback route, AI and embeddings enabled against local fixtures.

The closing candidate applies only reviewed rustfmt blobs plus documentation/verification changes and removes the temporary generated-files workflow. The final verification workflows have `contents: read`; none can rewrite this branch. No transformation or auto-commit script remains.

## Review and rulings

Final review: self-review (no subagent tool); no independent reviewer is claimed. Reviewed current implementation against all S1 tasks and preserved S2–S4 boundaries. Critical/important findings addressed: source batch starvation; Service startup touching a legacy bridge; exit racing startup; service tray opening a legacy window; inaccurate local-only inference promise. Added regression evidence is listed above. Native filesystem guards reject linked/reparse profile paths and do not invent a sandbox against a malicious same-user process.

Ruling: retain config default legacy, but dev.ps1 explicitly selects new local mode and provides -Legacy. This preserves normal old configuration semantics while offering the requested one-command development workflow. Cost: direct npm/packaged startup follows config, not the script's mode.

Ruling: use an isolated service-local space for first validation, with no automatic adoption of real user DBs. Internal explicit adoption remains tested. Cost: old history is not automatically visible in the new page; avoids unintended schema migration or data disclosure.

Ruling: only an explicitly managed sidecar opts into parent-pipe EOF lifetime. Independent Service stdin EOF retains its previous contract. Cost: abrupt GUI death stops its owned Service, while persisted work resumes next launch; normal exit still drains it.

Ruling: a true version conflict stays blocked with its original payload and visible error; no blind expected_revision update or silent target switch. Exact lost acknowledgements are replayed idempotently. Cost: manual content reconciliation is required for real divergent histories; full shared editing is S2.

Ruling: the offline test's publication is an internal reuse of the canonical publisher, not a shipped HTTP write route. Cost: S1 UI reads drafts/published mapped content but has no generic publish/edit button; that remains S2.

Deferred minors: Service lists expose the recent bounded page and detailed session JSON rather than full legacy UI parity; optional model settings are edited in the local TOML and require restart; installer bundling and default migration remain S3. Existing npm dependency audit warnings are recorded as release-hardening debt, not falsely marked cleared by functional CI. Real graphical/runtime/model quality and data-volume acceptance belong to the user's local validation.

No main update after PR47, no release, no real database adoption, no private conversation scan or use of a real model account. Development and CI artifacts use synthetic sources and model/content fixtures. The previous patches based on 36125a2 must not be applied to this branch.
