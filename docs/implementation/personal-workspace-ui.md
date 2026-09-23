# Personal independent Service workspace

This change stays on `feature/aiks-service-extraction`. It is not a team/company release. Numeric-loopback authentication, internal SiYuan content access, the separate service-local profile and existing model opt-in boundaries remain unchanged. No main merge, release, real user data migration or remote access enablement is part of this change.

## User flow

The Service shell always has Knowledge, Conversations, Collection/Sync, Tasks and Model/Service Settings navigation. The first saved preference is `unseen`; a non-blocking guide offers **跳过，进入知识库**. Skip/completion is saved in client-only collector metadata and later starts open Knowledge. Leaving the guide via navigation also dismisses it. Saving sources is not collection. No model configuration is required to browse existing content. A failed preference write shows a warning but does not prevent navigation; a failed Service connection is not rendered as an empty knowledge library.

Knowledge and conversation lists use bounded Service pages and corpus-specific search. Bodies are rendered as escaped text/code blocks or conversation message cards, not raw session JSON; remote attachments and embedded HTML are not loaded. Task diagnostics remain expandable. This is a reading/search workspace, not a claim that the complete original SiYuan editor, manual publication or RAG has been delivered.

For a genuinely new service-local installation the ordinary personal data root is selected without a compulsory storage modal. Explicit data-directory migration remains in Settings. Existing data roots, workspaces, databases, model configuration and legacy launch mode are retained; old knowledge is not silently moved into the new profile.

## Local session exclusions

In Collection/Sync choose a source and click **扫描本地会话**. Scan and preview use only the local Provider/ScopedReader path, not Service registration, snapshots, an upload queue or models. List entries show title, project, time and available message count; their opaque row keys map to native cached summaries, not caller-supplied paths or upstream IDs. Unknown/stale row keys are rejected. Partial scans are labeled partial. Paging is capped at 100 items and the current UI uses 30; very large discovery is capped at 10,000 entries. Preview is sanitized and bounded at 64 KiB/200 nonempty messages; rule-based redaction is not a guarantee that all sensitive material is removed.

Rules bind to verified instance, destination space, source kind and configured source-root identity. They persist across restarts and are honored by the collector without a frontend resubmitting raw IDs. Selecting exclusions pauses pending envelopes atomically. In-flight updates are refused rather than falsely claiming a request has been recalled. Excluding never deletes already accepted content. An earlier retry may already have been accepted even when its acknowledgement was lost; stopping future retries is not remote erasure. Restoring resumes only entries paused by this exclusion policy, not unrelated version/authentication conflicts. No automatic revision rebasing is added.

## Verification ledger

RED `582023e`: two real shell SSR assertions failed (no main navigation and raw ID input), and the native Service integration target failed because preferences/browse implementations did not exist. Existing Core and previous frontend tests remained intact.

At `5efde61`, frontend tests/build passed. Its exact SHA-checked native candidate passed all four new real Provider/outbox/HTTP preference tests. That preparation workflow only generated reviewable blobs; it did not update a branch. The candidate was reviewed and its hashes verified before applying canonical files and removing the temporary patch/workflow. Canonical whole-branch Actions must pass on the resulting commit before delivery; a preparer success is not final acceptance.

## Later team work

Service separation permits reusing processing/search and business API implementations. Team deployment still needs remote authenticated transport and login, identity/membership/ACL, shared spaces and publication rules, safe multi-user writes/conflicts, private SiYuan routing, and deployment/backup/operational controls. The current binary intentionally rejects public listeners and team mode. Do not promise that changing an address or adding a permission table alone creates the company edition.
