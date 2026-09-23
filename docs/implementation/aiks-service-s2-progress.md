# SDD ledger — plan: docs/superpowers/plans/2026-09-23-aiks-team-sharing-dingtalk.md

Updated 2026-09-23. Current continuation index; use this and the current Git HEAD rather than the older c888983 checkpoint. The user has approved the complete S2 plan and wants continued implementation, with real DingTalk/model configuration entered manually later. No additional approval of the same plan is needed.

## Implemented foundations

- Server AI/Embedding credentials: independent environment references, disabled-model isolation, atomic resolution before business DB/listener; original personal settings retained.
- TeamSettings: explicit opt-in, bound HTTPS origin/callback, company/application identity, bounded directory/session policy, strict unknown-key rejection.
- Configuration check: real `--check-config` reads and validates only; no business DB, listener or DingTalk call. It does not enable team startup. Environment-secret resolution is implemented; file-secret sources fail closed with `secret_file_not_supported` until protected descriptor/ACL loading is implemented. Do not call the optional file path complete.
- Company binding and pure read-only sharing policy: additive migration017, stable company/instance identity, immutable CorpId/Client ID binding, rejection of personal DB adoption, same-company composite foreign keys and read-only grants.
- Directory publication: additive migration018, stable internal employee/org identities, direct/descendant membership, complete snapshot validation and atomic generation switch. Partial/cyclic/conflicting input and SQL failures preserve the old generation. Deactivation increments a persistent auth version; reactivation cannot revive earlier credentials. The upstream periodic refresh worker is not yet implemented.
- DingTalk adapter: fixed official origins, user/app token separation, company and employee/union checks, bounded real reqwest HTTP, no redirects/proxies, singleflight app-token refresh and complete paginated department collection. No production-configurable fake endpoint and no live enterprise request in these tests.

## Evidence

`d356e4d` / Actions35829163514 was the inspected missing-interface RED for directory and configuration-check APIs. `5f3af5e` / Actions35834483111 is canonical GREEN: Core381 and Service56 reported passing executions, no failures, Core/CLI/Service Clippy, rustfmt and unchanged Cargo.lock passed. Counts include the subprocess ownership probe.

`a7417d3` / Actions35835405733 was the inspected DingTalk RED: unit target failed for missing DingTalkClient, not a network/configuration problem. `0d7ab8b` / Actions35836085257 passed Core381/Service63 reported executions and Clippy; only rustfmt failed. All seven real-wire adapter tests passed, covering company/identity/inactive failures, complete pagination, repeated cursors, no upstream on invalid input, app-token singleflight, redirect/timeout/oversize/429/malformed responses and safe errors. The closing candidate consists of the exact five formatter blobs generated from this SHA, with old and new blob hashes checked against the downloaded canonical source. Its own final CI must still be checked.

## Rulings and boundaries

The normalized provider input is DirectoryUser (external IDs); stored UserRecord carries stable AIKS IDs and private-space identity. Neither is a caller-created authenticated TeamContext. One IdentityProvider interface in Core carries trusted results; the Service adapter does not implement a second identity database.

The DingTalk wire tests are unit tests inside the canonical library so cfg(test)-only loopback origins can test the exact reqwest transport. No public/production test-origin constructor is added. Official permission names and sources are recorded in dingtalk-api-contracts.md; real console permissions and application visibility remain deployment acceptance, not inferred from synthetic responses.

The local container has no Rust toolchain and direct clone fails DNS; Rust verification runs on GitHub Actions, with checked source/log archives used in an isolated local checkout. No independent reviewer was available; author review is not described as independent review. Preparation workflows only produce blobs; they do not move refs or claim final acceptance.

## Remaining work

Continue Task4 refresh lifecycle and Task5 one-time OAuth/native verifier handoff, revocable access/refresh sessions. Then Task6 trusted contexts throughout every Core ingestion/read/search path, Task7 owner-managed sharing, Task8 controlled owner editing/publication/assets, Task9 native HTTPS connection and identity-bound queue, Task10 sharing UI, and Task11 controlled deployment/end-to-end acceptance. Protected secret-file loading also remains from Task1.

The server still rejects actual `mode=team` startup and non-loopback personal mode. Configuration check success and adapter tests do NOT make this branch deployable as a company service. No main merge, release, real user scan/adoption, private data migration, real model/login call or server deployment was performed. The user has no deployment task at this checkpoint.
