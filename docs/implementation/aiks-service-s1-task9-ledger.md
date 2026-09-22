# SDD ledger — plan: docs/superpowers/plans/2026-09-22-aiks-service-extraction-s1.md

Task9 started at 63c23a2, which includes main via PR47. No main mutation, release or real user-data action.

RED backend/owner controls at 32ed6bb (35707393821), followed by validated typed config, actual child bootstrap/owner shutdown, module export and controlled IPC/page. Native lifecycle and UI route wiring were then compiled and exercised on Linux and Windows.

RED bounded source batches at cf5756c (35710705189): only first three of seven were reachable across capped scans. GREEN at 011eeff (35711836122): persisted sorted scan cursor progresses [3,3,1] across outbox restarts, independent from immutable receipt/revision state.

RED four source wiring constraints at 011eeff (35711836120). GREEN 095bc89 frontend suite/build (35712721461), native full workspace tests/Clippy and Windows native compilation/contracts (35712721489): read-only runtime locator, lifecycle-gated startup cancellation/cleanup, Service-aware tray routes, accurate local storage versus model endpoint wording.

095bc89 isolated-offline run35712721455 also passed with actual Service binary, local model/embedding fixtures, no external network route, deleted original session files, canonical publisher and restarted canonical read/search.

Rulings: default legacy config remains; dev.ps1 selects service_local/-Legacy explicitly. First validation uses an isolated service-local workspace, not automatic real-user migration. Managed sidecars alone opt into owner EOF lifetime. Genuine version conflicts remain immutable/blocked, exact receipt replay covers response loss; full shared-content merge UX is S2. Known GUI list-size/JSON-detail and local TOML settings are bounded S1 UI, not full legacy UI parity.

Closing change applies the exact checked rustfmt blobs generated from095bc89 and removes the temporary generated-files workflow. Final canonical HEAD must run all read-only checks, including newly added Windows PowerShell5.1 entrypoint orchestration and native controller cancellation/config tests. Final review is self-review, not an independent agent report. See current progress index for explicit remaining S2–S4 scope and release-security debt.
