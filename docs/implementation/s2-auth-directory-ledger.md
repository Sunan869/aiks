# SDD ledger — plan: docs/superpowers/plans/2026-09-23-aiks-team-sharing-dingtalk.md

Resume 2026-09-23 from remote d356e4d, NOT c888983. The remote contains five later S2 commits: configuration validation, owner-only pure ACL, company binding/migration 017, and new RED tests. The earlier top-level progress page predates them; do not reimplement already committed primitives.

Read baseline archive d356e4d from Actions 35829163514. Its Core team_directory target fails specifically for missing DirectorySnapshot/DirectoryUser/Membership/OrgRecord and directory operations. Its Service team_config_check target fails for missing secrets and check_configuration_with. These missing-interface failures are the current RED evidence, not a failed live DingTalk call.

Implementing the pending directory/configuration contracts on the existing isolated feature branch. Four existing-file wiring edits are exact SHA-guarded; a preparation workflow only emits reviewable blobs. The canonical source must be explicitly updated and verified afterwards; the preparation workflow never changes a ref and is not an acceptance gate.

Ruling: environment-secret loading is usable now; file-secret sources explicitly return secret_file_not_supported until descriptor-based ownership/ACL checks are implemented and tested on each supported OS. Do not claim Task 1 complete while this optional planned path remains unavailable. Never silently substitute an insecure read.
Ruling: normalized directory input separates external DirectoryUser from internal UserRecord; adapter input is not authenticated TeamContext. Membership IDs in input are external, resolved to stable random internal IDs in one transaction. Reject inconsistent stable identity rebinding rather than assigning old knowledge to a new external account.
Ruling: pure directory publication precedes the upstream adapter to complete the already committed RED suite. The later adapter/refresh Worker remain pending. Published snapshots require completeness, bounded acyclic scope, consistent references and time; partial input never deactivates absent users.
Ruling: retain two full generations, keep user/org identities permanently, and keep a separate monotonic auth_version per user. Deactivation fences snapshots observed before or at the event, so an in-flight stale scan cannot revive a user or old credentials.

No team listener, real login, user-data scan/migration, main merge or release. Current work remains internal foundation, not deployable team acceptance.
