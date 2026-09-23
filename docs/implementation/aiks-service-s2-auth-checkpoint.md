# S2 authentication checkpoint — 2026-09-23

Plan: `docs/superpowers/plans/2026-09-23-aiks-team-sharing-dingtalk.md`.
Baseline: `b03455d4cac3b6e9f2b9cd3872118417795b1250`.
This is a Task 5 implementation checkpoint, not authorization to deploy a team service.

## Implemented in canonical code

- Core LoginStore and SessionStore with SQLite-atomic browser launch/state/nonce/native-verifier binding, one-time callback claims and code replay rejection.
- Independent AIKS access/refresh credentials, persisted only as SHA-256 digests. Refresh rotates credentials and revokes the family on consumed-token replay. Logout and directory auth-version changes invalidate saved contexts; reactivation does not restore old sessions.
- Additive migration 019, active-device/pending-login budgets and bounded expired-state cleanup. No user-created owner context is deserialized from request input.
- Real auth-only HTTP router: start, browser launch, DingTalk callback, native exchange, refresh, logout and me. Browser responses contain no business token; cookies are Secure/HttpOnly/SameSite=Lax, navigation uses fixed origins, and every auth response is no-store/no-referrer.
- Exact Host, no browser Origin on native APIs, bounded bodies, duplicate-header checks, rejected credentials in URL queries, socket-address rate limits and bounded concurrent upstream calls. X-Forwarded-For is not trusted to bypass limits.
- The existing DingTalk adapter and this router use the same authorize-URL constructor, not two providers.

The executable STILL rejects operational team mode. This router has not been mounted next to any unrestricted personal business route. Runtime request authorization, sharing/content APIs, native team connection and deployment verification are remaining tasks.

## Verification evidence

The baseline Core session target was RED both remotely and in the isolated local checkout because AuthPolicy/LoginStore/SessionStore and LoginPending did not exist. The unchanged assertions passed after implementation. The HTTP target first failed for the missing auth-router module; after that implementation, a new real HTTP assertion reproduced query-token acceptance (200 instead of 400), then passed after the boundary fix.

Local execution now uses an actual Rust 1.98.1 toolchain and matching public offline dependencies obtained from the repository's existing read-only S2 development artifact, with checksums verified. This is no longer an assertion that Rust cannot run in the working environment.

Commands run against the exact candidate files:
- `cargo test --locked -p aiks-core --test team_sessions`: 7 passing tests.
- `cargo test --locked -p aiks-service --test team_login`: 4 passing tests.
- `cargo test --locked --no-fail-fast -p aiks-core -p aiks-service -- --test-threads=1`: 458 reported passing executions, 0 failures; this includes subprocess probes and is not a claim of 458 unique functions.
- `cargo clippy --locked -p aiks-core -p aiks-cli -p aiks-service --all-targets -- -D warnings`: passed.
- `cargo fmt --all --check` and `git diff --check`: passed before canonical delivery.

Source transport commit bb4b110 contains the feature files and two small SHA-guarded wires. Existing format-review Actions 35849656528 emitted those wires and the formatting of the existing session tests without moving any branch. All emitted bytes and old/new blob identities were compared to the locally tested files. The closing commit applies these canonical blobs and removes the temporary script. Its own Actions results must be read before claiming remote or Windows acceptance; prepare success alone is not acceptance.

Self-review covered bearer/identity confusion, foreign company, expired/consumed transactions, deactivation/reactivation, concurrent exchange, SQL rollback, refresh replay, no credential echoes, and no external request while holding SQLite state. There was no independent reviewer available. Synthetic identity providers and temporary databases were used; no real DingTalk secret, user session, login, model endpoint or SiYuan deployment was accessed.

## Rulings and next tasks

- Task 4 directory worker exists at b034 and its existing complete/partial/coalesced-refresh/cancellation tests passed in the full Service suite. Do not reimplement it based on an older ledger paragraph.
- Task 5 callback/browser handoff is AIKS-owned; no unverified upstream PKCE/device-code capability is assumed.
- Environment-secret configuration/checking is implemented; protected secret-file support remains explicitly fail-closed rather than claiming Windows ACL validation exists.
- Task 6/7 will use the same SessionStore transaction check and Core resource policy for every business operation. A successfully issued token is not permission to call the current personal runtime.
- Deployment remains blocked until business isolation, owner-only writes, desktop connection and the final team startup gates pass. The user has no deployment or secret-sharing task at this checkpoint.

References consulted: DingTalk official browser OAuth guide and RFC 9700. Actual upstream/company authorization is a separate future integration check.
