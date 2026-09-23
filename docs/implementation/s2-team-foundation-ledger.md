# SDD ledger — plan: docs/superpowers/plans/2026-09-23-aiks-team-sharing-dingtalk.md

Continuation from c888983, 2026-09-23. S2 is not ready for deployment. Do not replace the last working personal installation with an intermediate failing test commit.

Baseline: c888983 passed Core/Service, Linux workspace and Windows native/process CI. This working container has no Rust toolchain and direct clone failed DNS; verified Actions source archives are used in an isolated local source mirror. Mirror commit identities are not remote commit identities. Rust verification remains on Actions.

Ruling: the first test-write request, which included modifying permissions of temporary secret files, was blocked by the tool safety check. That request was not committed and those permission-changing tests were not resubmitted through another mechanism. Continue with pure settings/policy/storage work; protected file-secret validation stays pending rather than weakening access checks or claiming it passed.

Task 1 partial: added TeamSettings and pure static validation only. Disabled by default, HTTPS canonical origin, exact callback path, explicit company/application, one secret reference, bounded positive directory scope and bounded durations; diagnostics contain fixed fields/codes. It does not yet wire TeamSettings into ServiceConfig, resolve DingTalk secrets, run --check-config or enable team listeners.

Task 2 partial: added owner-only policy and company/identity/read-grant storage. Pure policy inputs must come from trusted server state; the boolean predicate is not itself an authenticated API. Company identity cannot be rebound to another CorpId/Client ID. Personal and team ServiceStore initialization refuse each other's databases. Legacy rows are never implicitly assigned to a company.

RED 69a08c1 / Actions 35827223684: test targets failed with missing aiks_core::team and aiks_service::team. Implementation 01e9ed8 / Actions 35827568697 passed Core/Service tests and related Clippy; only rustfmt failed.

RED 0527ef6 / Actions 35827888450: new team_storage target failed with missing TeamError/TeamStore, while all four TeamSettings tests and previous Service tests passed. The formatting candidate from 0527ef6 was downloaded; every old/new blob hash and the complete diff were checked before applying its four formatting-only changes.

Ruling: migration 017 contains the tested company/user/organization identity, owner, read grants and audit foundation. Directory snapshots and login/session/content-intent tables will be added by their owning task in subsequent additive migrations, rather than precreating untested future schema. The plan's later migration numbers must be reallocated accordingly; do not rewrite 001–017 after use.

The company binding test rejects personal/legacy data after opening a fixture with current StateDb migrations. It proves old rows and identities are not adopted or overwritten, not a byte-for-byte no-write preflight. The future team bootstrap must perform its separate read-only mode/ownership preflight before opening a real database.

Current implementation is awaiting canonical compilation and tests. No main update, release, company deployment, real data scan/adoption, DingTalk API request or public listener has been performed. Configuration/ACL foundations do not satisfy the full Task 1/2 completion contracts or the S2 end-to-end acceptance gate.
