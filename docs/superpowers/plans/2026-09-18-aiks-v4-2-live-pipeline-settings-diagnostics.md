# AIKS V4.2 Live Pipeline, Settings, and Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Processing Center populate during raw sync, make V4.2 diagnostics truthful, and make Settings read/write the real engine configuration.

**Architecture:** Keep Raw Sync and the durable V3 Pipeline separate, but add a synchronous per-candidate hook so `sync_and_enqueue_extraction()` can enqueue each session immediately after its successful raw write. Treat `aiks.toml` as the single source of truth for engine settings, with desktop-only autostart/close-to-tray behavior applied separately and immediately. Diagnostics reports Kernel, Workbench mount, and Bridge handshake as distinct states.

**Tech Stack:** Rust, Tokio, rusqlite, Tauri 2, tauri-plugin-autostart, React 18, TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-09-18-aiks-v4-2-live-pipeline-settings-diagnostics-design.md`

## Global Constraints

- Preserve the managed private default AI endpoint defined by `AiModelConfig::default()` and model `Qwen3.8-27B`.
- Plain sync and dry-run must not enqueue pipeline jobs.
- Do not hot-rebuild `AiksEngine`, Watcher, PipelineWorker, or SiYuan runtime.
- Preserve unexposed `aiks.toml` fields on save.
- User-facing diagnostics copy is V4.2.

---

### Task 1: Stream pipeline enqueue during raw sync

**Files:**
- Modify: `crates/aiks-core/src/sync/engine.rs`
- Modify: `crates/aiks-core/src/engine/mod.rs`
- Test: `crates/aiks-core/tests/` existing sync/pipeline regression tests plus a focused new test if needed

**Interfaces:**
- Produces: a `SyncEngine` entry point that accepts a synchronous `ExtractionCandidate` handler invoked immediately after each successful Created/Updated session.
- Consumes: existing `ExtractionCandidate`, `PipelineOrchestrator`, and `PipelineWorker::submit`.

- [x] Write a failing Rust regression test proving candidate A is observable before candidate B/full-run completion, while dry-run/plain sync do not submit executable work.
- [x] Run the focused Rust test and confirm RED for missing immediate callback behavior.
- [x] Add `run_sync_with_candidate_handler` (or equivalent internal API) while keeping `run_sync` compatibility.
- [x] Make `sync_and_enqueue_extraction()` enqueue inside the per-candidate handler and remove the post-run duplicate enqueue loop.
- [ ] Run focused tests and the existing B03/R09 regression set; confirm GREEN.

### Task 2: Make Processing Center live and diagnostics truthful

**Files:**
- Modify: `apps/aiks-desktop/src/pages/ProcessingPage.tsx`
- Modify: `apps/aiks-desktop/src/pages/DiagnosticsPage.tsx`
- Modify: `apps/aiks-desktop/src-tauri/src/diagnostics.rs`
- Modify if needed: `apps/aiks-desktop/src-tauri/src/workbench/controller.rs`
- Test: frontend Vitest files under `apps/aiks-desktop/src/api/` or new page behavior tests

**Interfaces:**
- Processing page polls `getPipelineRuns/getPipelineStats` every 2000 ms only while `document.visibilityState === "visible"` and prevents overlapping loads.
- Diagnostics distinguishes kernel-ready, workbench-mounted/available, and bridge-ready.

- [x] Add failing Vitest coverage for 2-second polling cleanup and V4.2 diagnostics state labels.
- [x] Run focused frontend tests and confirm RED.
- [x] Implement visibility-aware Processing Center polling without overlapping loads.
- [x] Rename user-facing diagnostics copy to V4.2 and distinguish `未挂载`, `等待 Bridge`, and `已连接`.
- [ ] Run focused frontend/Rust diagnostics tests and confirm GREEN.

### Task 3: Make Settings use real Config and real desktop controls

**Files:**
- Modify: `apps/aiks-desktop/src-tauri/src/commands.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/app_state.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/lib.rs`
- Modify: `apps/aiks-desktop/src/pages/SettingsPage.tsx`
- Modify: `apps/aiks-desktop/src/api/types.ts` / `index.ts` / `tauri.ts` if shared API types are used
- Test: Rust command/config tests and frontend Settings Vitest coverage

**Interfaces:**
- `AppSettings` includes `ai_base_url`, `ai_model`, and `close_to_tray`; removes three unsupported extraction pseudo-flags.
- `get_settings` loads `Config::from_file(config_file_path())` or `Config::default()` and overlays actual autostart/desktop state.
- `save_settings` atomically updates exposed `Config` fields and preserves the rest.
- Add a settings-specific AI connection command accepting `{ base_url, model }` and using an ephemeral AI config derived from current config/defaults.

- [x] Add Rust coverage for AI URL/model values and preservation of unrelated Config fields.
- [x] Add failing frontend tests proving obsolete hardcoded URL/model and unsupported pseudo-controls are gone.
- [x] Implement the real Settings DTO/config load-save mapping.
- [x] Wire autostart via `tauri-plugin-autostart` and a shared atomic close-to-tray flag respected by `CloseRequested`.
- [x] Make Settings connection test use current form endpoint/model.
- [x] Add explicit copy that engine-owned settings take effect after restart.
- [ ] Run focused Rust/frontend tests and confirm GREEN.

### Task 4: Full verification and merge readiness

Implementation is now on the PR branch and temporary patch workflows/scripts have been removed. The remaining work is formal CI verification and any fixes required by those results.

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run `cargo test --workspace -- --test-threads=1`.
- [ ] Run desktop frontend `npm test` and `npm run build`.
- [ ] Run Windows desktop compile through GitHub CI.
- [x] Review diff for temporary workflows/scripts and duplicate post-sync pipeline enqueue paths.
- [ ] Review final diff for stale user-facing V4.1 copy and obsolete frontend AI defaults.
- [ ] Wait for all required CI jobs to pass, then merge to `main`.
