# SDD ledger — plan: docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md

Resume from immutable 63c23a2, which includes main via PR47. Core364/0 and Service29/0 baseline verified in Actions35706022816. Local source archive tree55f25fb matches GitHub exactly; clone DNS and local Rust unavailable, so compilation/testing run in existing Actions. No changes to main, no release.

Task9 in progress: behavioral RED contracts for backend mode, owned stdin shutdown, and frontend service transport/status. Followed by native supervisor, mutually exclusive startup, controlled command/UI wiring, and dev.ps1 integration.

Ruling: retain legacy as the config default per approved plan. dev.ps1 will explicitly select service_local for this feature test path, with -Legacy as escape hatch. This is not automatic migration of user data; Service uses an isolated local state directory until explicit adoption is chosen. No separate public SiYuan proxy or team listener is introduced.

Ruling: the new Service owns its shutdown pipe only when bootstrap explicitly opts in; closing stdin alone never stops independent Service instances. Supervisor may shut down only its own Child handle, not a PID read from stale metadata or a user URL.

Task10 remains integration and review after Task9 passes. No new team/RAG/install completion claims are made by this S1 implementation.
