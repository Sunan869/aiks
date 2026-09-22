# S1 continuation — Task 8 native checkpoint, Task 9 next

Current authoritative status: `docs/implementation/aiks-service-s1-progress.md`. Earlier Task 5/6 continuation text is historical and retained in Git; do not reapply its patches or reimplement already committed service code.

Continue only from the current remote `feature/aiks-service-extraction` HEAD. No main changes, merge or installer publication are authorized by this checkpoint.

## Verified work

Tasks 1–7 have implemented contracts and real automated tests. Task 8 now contains canonical Desktop `service_client/{mod,transport,outbox,collector}.rs`, exported through the actual Tauri library. `c389d7d` passed native client tests on Linux and Windows, Linux full-workspace Clippy/tests, and frontend regression tests/build. Workspace result summaries report 424 passing executions / 0 failed / 0 ignored, including the subprocess ownership probe. The subsequent closing commit changes only native-test formatting and these status records; verify its own CI rather than inferring a result.

The real collector tests prove sanitized Continue capture, immutable retry/acknowledgement, source deletion before processing, cached-registration offline collection, exclusions and rejection of malformed legacy JSONL. No private user source or real model/content service is used.

## Next implementation: Task 9

1. Add strict backend.mode parsing: legacy by default, service_local as explicit opt-in, unknown values rejected. Branch before any legacy AiksEngine/Provider Worker or SiYuan startup; do not silently fall back if the Service cannot start.
2. Native supervisor starts only a fixed native-selected binary path. Pass the random bootstrap credential through stdin; validate the bounded stdout protocol/instance/nonce/address, then authenticated capabilities. Secrets and arbitrary executable paths/URLs never go to the webview.
3. Close-to-tray keeps the owned Service; full application exit stops collection and bounded-drains the owned child. Never kill an independent or foreign service. Acquire Task 2 ownership in legacy Desktop/CLI before business writes and require the Service to stop before returning to legacy.
4. Wire controlled service_status/service_collect_selected/service_get_receipt/service_get_job/service_search actions, DesktopPlatformApi vs ServiceApi and ServiceStatusPage. Show accepted vs queued/processed/failed correctly; empty search differs from transport failure; no legacy-write or production-mock fallback. Include explicit conflict/receipt/version handling without blind revision rebasing.
5. Add behavioral RED tests before the native supervisor and frontend changes, verify GREEN on the exact branch HEAD, then proceed to Task 10.

Task 10 still needs the full complete-workflow offline/model/content integration and final branch review. Current GUI startup remains legacy; merely compiling the native client module is not a service-mode GUI or installer delivery.
