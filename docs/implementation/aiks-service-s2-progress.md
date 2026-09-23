# SDD ledger — plan: docs/superpowers/plans/2026-09-23-aiks-team-sharing-dingtalk.md

## 2026-09-23, baseline e9316a7

The user approved the S2 implementation plan and explicitly added that AI and embedding configuration belongs on the server. Team Service owns shared model endpoints, names, keys and index policy; the local Service retains independent personal configuration. No desktop transmission of company model credentials. No team/public listener is enabled at intermediate checkpoints.

Baseline Actions 35823094909 passed; archive summary reports Core 369 and Service 39 passing executions. The source archive was read into an isolated container checkout because direct git clone failed DNS and this container has no Rust toolchain. Subsequent Rust checks run on GitHub Actions, not claimed as local runs.

Pre-flight: Task 1 validated settings feed Tasks 3/5/11; Task 2 trusted identity and grants feed 4/5/6/7/8; Task 5 authenticated context must reach every Task 6 read and every Task 7/8 write; Task 9 must bind queues to instance/company/user. No subsequent task may substitute a caller-controlled principal for these contracts.

Task 1 in progress: start with the user's model-configuration addition. Add independent server-only environment references for AI and embedding; preserve existing personal inline credentials and no-auth local models. Resolving configured active references must be atomic and fail before database open/listen. Disabled models do not read environment. The original Task 1 DingTalk static settings, protected file-secret reads and full --check-config remain pending; this model increment alone is not Task 1 or S2 completion.

Ruling: `[model_credentials]` holds environment variable NAMES, separate from the existing Core `[ai]` and `[embedding]` DTOs — this preserves personal/Core compatibility and avoids deserializing secret values into client-visible configuration. Team templates recommend references; existing personal inline fields are retained but cannot be mixed with an active environment reference. This is a project configuration contract, not a DingTalk option.

Ruling: vector model/dimension/chunking changes need a controlled reindex; do not describe changing model settings as automatically migrating old vectors. Existing originals must be preserved and future Task 6 must isolate incompatible index generations. No unrelated pipeline/model implementation changes in this configuration increment.

## Model-configuration increment verification

RED 0498265, Actions 35824064763: the new Service test target fails to compile specifically because `model_credentials` and `resolve_model_credentials_with` do not exist; baseline Core still passes. This is a missing-interface RED, not an observed model inference failure.

Implementation fa1cd09, Actions 35824559930: Core 369 and Service 49 reported passing executions / 0 failed. All nine new configuration tests and the actual-process bootstrap/capabilities test pass. Core/CLI/Service all-target Clippy and unchanged Cargo.lock pass. Only rustfmt failed: a condition brace and one test write_all expression. The closing commit applies exactly those two formatting differences; its own CI must still be checked before delivery.

Self-review: resolution validates both references before lookup, applies keys only after both succeed, and preserves old personal inline/no-auth behavior. A missing active key is tested against the real binary with a fixed safe error and no business database creation. The real-process test verifies capabilities/handshake/diagnostics omit synthetic model secrets; it does not make model inference requests and is not a DingTalk or company deployment acceptance test. No independent reviewer was available.

Remaining: Task 1 DingTalk settings/secret-file checks and Tasks 2–11. Standalone Service configuration now supports the model credential block; current Desktop models.toml remains its existing separate personal configuration and is not instructed to contain this new block. The entire team template still cannot start team mode. No migration, main update, release, real user scan or live model/login request was performed.
