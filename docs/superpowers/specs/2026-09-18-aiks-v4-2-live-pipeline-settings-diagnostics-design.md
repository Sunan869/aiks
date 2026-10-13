# AIKS V4.2 Live Pipeline, Settings, and Diagnostics Design

Date: 2026-09-18

## Goal

Fix three related V4.2 product inconsistencies:

1. Processing Center stays empty until a whole raw-session sync completes.
2. Help & Diagnostics still shows V4.1 copy and conflates Kernel reachability with Workbench/Bridge readiness.
3. Settings UI has multiple sources of truth and several controls that do not map to real runtime configuration.

## Pipeline behavior

Raw Session synchronization remains the first durable step. After each individual session is successfully created or updated in SiYuan, AIKS must immediately create/resolve its V3 `pipeline_run` and submit a `PipelineJob` when `ai.enabled && ai.auto_extract` is true. The full sync must still return aggregate `SyncStats`, including `extraction_candidates`, for compatibility and reporting.

Plain `sync()` and dry-run flows must not enqueue pipeline work. Only `sync_and_enqueue_extraction()` uses the live per-session candidate callback. A single candidate must never be submitted twice by the same sync run.

Processing Center polls every 2 seconds while the page is visible and stops polling when hidden/unmounted. Manual refresh remains available.

## Diagnostics behavior

The page copy is V4.2, not V4.1.

Display three distinct states:

- **SiYuan Kernel**: runtime URL/reachability exists.
- **Embedded Workbench**: whether a child/fallback workbench is currently mounted/available for UI use.
- **AIKS Bridge**: whether the mounted workbench has completed the `bridgeReady` handshake.

A healthy Kernel with no mounted workbench must display **未挂载**, not **等待 Bridge**. `等待 Bridge` is only valid after workbench mounting has begun and before the handshake completes. `Bridge 已连接` is shown only when `ready=true`.

Existing transport command names may remain for compatibility, but user-facing copy must say V4.2.

## Settings behavior

`<data-root>/config/aiks.toml` / `aiks_core::Config` is the single source of truth for engine settings. `get_settings` must derive its response from that config (or defaults if the file does not exist), not from stale `app.json` UI cache.

The settings DTO includes real values for:

- startup/autostart
- close-to-tray
- sync watch enabled
- scan interval
- include thinking
- include tool calls
- max tool-result chars
- secret redaction
- AI enabled
- AI auto-extract
- AI base URL
- AI model

The three unsupported pseudo-controls (`ai_extract_tags`, `ai_extract_problems`, `ai_extract_decisions`) are removed from the page and DTO.

Saving settings updates `aiks.toml` atomically and preserves fields not exposed by the UI. Desktop-only behavior is applied immediately where safe:

- autostart enable/disable via `tauri-plugin-autostart`
- close-to-tray flag stored in application state and respected by `CloseRequested`

Engine-owned settings (AI URL/model, sync interval/watch, content/security) are persisted for the next engine start. The page must explicitly say that these take effect after restarting AIKS and expose a save flow without falsely implying live engine reconfiguration.

`test_ai_connection` from Settings must test the values currently typed into the form rather than the engine's old in-memory AI configuration.

## Compatibility and safety

- Preserve private default AI endpoint the managed private endpoint defined by `AiModelConfig::default()` and model `Qwen3.8-27B`.
- Do not make dry-run create pipeline work.
- Do not hot-rebuild `AiksEngine`, Watcher, PipelineWorker, or SiYuan runtime in this change.
- Preserve all unexposed TOML fields when saving settings.
- Existing raw Session documents in SiYuan remain read-only/protected; their appearance before pipeline rows is expected only for the short interval between each raw write and immediate pipeline enqueue.

## Verification

Add regression coverage for:

- per-session candidate callback occurs before full sync completion and plain sync/dry-run do not enqueue;
- Processing Center polling lifecycle;
- diagnostics copy/state mapping (`未挂载` vs `等待 Bridge` vs connected);
- `get_settings` reads real `Config` defaults/file values, including AI base URL/model;
- saving preserves unrelated config fields;
- Settings connection test uses form AI endpoint/model;
- unsupported pseudo-controls are absent from Settings UI.
