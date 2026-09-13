/// Application lifecycle management.
///
/// Startup sequence (spec §37):
///  1. resolve data dir
///  2. create directories
///  3. open AIKS State DB
///  4. resolve embedded SiYuan runtime
///  5. validate runtime
///  6. allocate port
///  7. start Kernel
///  8. wait version endpoint
///  9. initialize SiYuan Sink
/// 10. ensure Notebook
/// 11. initialize Providers
/// 12. start Watcher
/// 13. perform background initial sync
use std::sync::Arc;

use aiks_core::bootstrap::{ensure_notebook, validate_runtime, BootstrapConfig, DevOverride};
use aiks_core::runtime::{SiyuanRuntime, SiyuanRuntimeConfig, RuntimeState};
use aiks_core::{AiksEngine, AiksEngineConfig};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::app_state::{data_dir, AppState};
use crate::bootstrap::find_runtime_root;

// ── Startup ────────────────────────────────────────────────────────────────────

pub async fn startup(app: AppHandle) -> anyhow::Result<()> {
    let data_dir = data_dir();
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(data_dir.join("data"))?;
    std::fs::create_dir_all(data_dir.join("config"))?;
    std::fs::create_dir_all(data_dir.join("logs"))?;
    std::fs::create_dir_all(data_dir.join("siyuan").join("workspace"))?;

    info!("Data directory: {}", data_dir.display());

    emit_progress(&app, "init", "正在初始化...");

    // ── Check developer override (spec §29) ────────────────────────────────────
    let dev_override = DevOverride::from_env();
    if dev_override.is_active() {
        warn!("Developer override active: SIYUAN_URL={:?}", dev_override.url);
        let base_url = dev_override.url.clone().unwrap_or_else(|| "http://127.0.0.1:6806".to_string());
        return startup_with_external_siyuan(app, base_url, dev_override.token, data_dir).await;
    }

    // ── Find runtime root ─────────────────────────────────────────────────────
    let runtime_root = match find_runtime_root(&app) {
        Ok(p) => p,
        Err(e) => {
            error!("Runtime not found: {}", e);
            emit_error(&app, &format!("SiYuan 运行环境未就绪：{}", e));
            return startup_without_siyuan(app, data_dir).await;
        }
    };
    info!("Runtime root: {}", runtime_root.display());

    // ── Validate runtime ──────────────────────────────────────────────────────
    emit_progress(&app, "validate", "正在验证知识引擎...");
    let bootstrap_cfg = BootstrapConfig::new(
        runtime_root.clone(),
        data_dir.clone(),
        "AI Knowledge",
    );
    if let Err(e) = validate_runtime(&bootstrap_cfg) {
        error!("Runtime validation failed: {}", e);
        emit_error(&app, &format!("知识引擎文件不完整：{}", e));
        return startup_without_siyuan(app, data_dir).await;
    }
    info!("Runtime validation passed");

    // ── Start SiYuan Kernel ───────────────────────────────────────────────────
    emit_progress(&app, "starting_siyuan", "正在启动知识引擎...");
    let runtime_cfg = bootstrap_cfg.runtime_config();
    let runtime = SiyuanRuntime::new(runtime_cfg);

    let runtime_info = match runtime.start().await {
        Ok(info) => {
            info!(port = info.port, version = %info.version, "SiYuan ready");
            emit_progress(&app, "siyuan_ready", &format!("知识引擎就绪 (v{})", info.version));
            info
        }
        Err(e) => {
            error!("SiYuan failed to start: {}", e);
            emit_error(&app, &format!("知识引擎启动失败：{}", e));
            // Show control center in error state
            show_control_center(&app);
            let runtime_arc = Arc::new(Mutex::new(runtime));
            let state = AppState {
                engine: None,
                siyuan_url: Arc::new(Mutex::new(None)),
                data_dir,
            };
            app.manage(state);
            app.manage(runtime_arc);
            return Ok(());
        }
    };

    let base_url = runtime_info.base_url.clone();

    // ── Ensure notebook ───────────────────────────────────────────────────────
    emit_progress(&app, "notebook", "正在准备知识库...");
    match ensure_notebook(&base_url, "AI Knowledge").await {
        Ok(nb) => info!(notebook_id = %nb.id, "Notebook ready"),
        Err(e) => warn!("Could not ensure notebook: {}", e),
    }

    // ── Initialize AIKS Engine ────────────────────────────────────────────────
    emit_progress(&app, "init_engine", "正在初始化采集引擎...");
    let engine_config = AiksEngineConfig {
        config_path: None,
        siyuan_base_url: Some(base_url.clone()),
        siyuan_token: None, // embedded mode — no token
    };
    let engine = match AiksEngine::initialize(engine_config) {
        Ok(e) => e,
        Err(e) => {
            error!("Engine init failed: {}", e);
            emit_error(&app, &format!("初始化失败：{}", e));
            return Ok(());
        }
    };

    // ── Register state ────────────────────────────────────────────────────────
    let runtime_arc = Arc::new(Mutex::new(runtime));
    let state = AppState {
        engine: Some(Arc::new(engine)),
        siyuan_url: Arc::new(Mutex::new(Some(base_url.clone()))),
        data_dir,
    };
    app.manage(state);
    app.manage(runtime_arc);

    // ── Show only Control Center (spec §40: single window) ───────────────────
    emit_progress(&app, "ready", "AIKS 已就绪");
    // Knowledge window is NOT auto-shown; user opens it via sidebar "知识库" button
    show_control_center(&app);

    // ── Background initial scan + sync ────────────────────────────────────────
    // This must actually run and write sessions to SiYuan (V2.5 fix)
    if let Some(state) = app.try_state::<AppState>() {
        if let Some(engine) = &state.engine {
            let engine = engine.clone();
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                tracing::info!("[STARTUP] Beginning initial scan...");
                let _ = app2.emit("sync-status", serde_json::json!({"status": "scanning", "message": "正在扫描 AI 工具会话..."}));

                let scan = engine.scan(None).await;
                tracing::info!(
                    "[SCAN] Discovered {} sessions (Codex={}, OpenCode={}, Gemini={}, Claude={})",
                    scan.total,
                    scan.by_source.get("Codex").unwrap_or(&0),
                    scan.by_source.get("OpenCode").unwrap_or(&0),
                    scan.by_source.get("Gemini CLI").unwrap_or(&0),
                    scan.by_source.get("Claude Code").unwrap_or(&0),
                );
                let _ = app2.emit("sync-status", serde_json::json!({
                    "status": "discovered",
                    "total": scan.total,
                    "by_source": scan.by_source,
                    "message": format!("发现 {} 条会话，开始同步...", scan.total)
                }));

                let opts = aiks_core::SyncOptions {
                    source_filter: None,
                    dry_run: false,
                    overwrite: false,
                };

                tracing::info!("[SYNC] Starting initial raw sync for {} sessions", scan.total);
                let _ = app2.emit("sync-status", serde_json::json!({"status": "syncing", "message": "正在同步到知识库..."}));

                match engine.sync(opts).await {
                    Ok(stats) => {
                        tracing::info!(
                            "[SYNC] Complete: discovered={} new={} updated={} unchanged={} skipped={} failed={}",
                            stats.discovered, stats.new_count, stats.updated_count,
                            stats.unchanged_count, stats.skipped_count, stats.failed_count
                        );
                        let _ = app2.emit("sync-complete", serde_json::json!({
                            "discovered": stats.discovered,
                            "new": stats.new_count,
                            "updated": stats.updated_count,
                            "unchanged": stats.unchanged_count,
                            "skipped": stats.skipped_count,
                            "failed": stats.failed_count,
                            "extraction_queued": stats.extraction_candidates.len()
                        }));

                        // Enqueue extraction for candidates
                        if engine.ai_config().enabled && !stats.extraction_candidates.is_empty() {
                            tracing::info!(
                                "[EXTRACT] Queuing {} sessions for AI extraction",
                                stats.extraction_candidates.len()
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!("[SYNC] Initial sync failed: {}", e);
                        let _ = app2.emit("sync-error", serde_json::json!({"error": e.to_string()}));
                    }
                }
            });
        }
    }

    // ── Start file watcher ────────────────────────────────────────────────────
    if let Some(state) = app.try_state::<AppState>() {
        if let Some(engine) = &state.engine {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let watcher = engine.create_watcher(tx);
            if let Ok(_handle) = watcher.start() {
                let engine = engine.clone();
                let app3 = app.clone();
                tauri::async_runtime::spawn(async move {
                    while let Some(event) = rx.recv().await {
                        let source = event.source.as_str().to_string();
                        let opts = aiks_core::SyncOptions {
                            source_filter: Some(source),
                            dry_run: false,
                            overwrite: false,
                        };
                        match engine.sync(opts).await {
                            Ok(s) if s.new_count + s.updated_count > 0 => {
                                let _ = app3.emit("sync-complete", serde_json::json!({
                                    "new": s.new_count,
                                    "updated": s.updated_count
                                }));
                            }
                            Err(e) => warn!("Watcher sync error: {}", e),
                            _ => {}
                        }
                    }
                });
            }
        }
    }

    info!("AIKS startup complete");
    Ok(())
}

/// Startup without SiYuan (runtime missing or failed to start).
/// Control Center is shown in an error/limited state.
async fn startup_without_siyuan(app: AppHandle, data_dir: std::path::PathBuf) -> anyhow::Result<()> {
    let state = AppState {
        engine: None,
        siyuan_url: Arc::new(Mutex::new(None)),
        data_dir,
    };
    app.manage(state);
    show_control_center(&app);
    Ok(())
}

/// Startup using a developer-overridden external SiYuan URL.
async fn startup_with_external_siyuan(
    app: AppHandle,
    base_url: String,
    token: Option<String>,
    data_dir: std::path::PathBuf,
) -> anyhow::Result<()> {
    info!("Using external SiYuan: {}", base_url);

    match ensure_notebook(&base_url, "AI Knowledge").await {
        Ok(nb) => info!(notebook_id = %nb.id, "Notebook ready"),
        Err(e) => warn!("Could not ensure notebook: {}", e),
    }

    let engine_config = AiksEngineConfig {
        config_path: None,
        siyuan_base_url: Some(base_url.clone()),
        siyuan_token: token,
    };

    let engine = AiksEngine::initialize(engine_config)?;
    let state = AppState {
        engine: Some(Arc::new(engine)),
        siyuan_url: Arc::new(Mutex::new(Some(base_url.clone()))),
        data_dir,
    };
    app.manage(state);

    show_knowledge_window(&app, &base_url);
    show_control_center(&app);
    Ok(())
}

// ── Shutdown ───────────────────────────────────────────────────────────────────

/// Graceful shutdown (spec §35).
pub async fn shutdown(app: &AppHandle) {
    info!("Shutting down AIKS...");

    if let Some(runtime) = app.try_state::<Arc<Mutex<SiyuanRuntime>>>() {
        runtime.lock().await.stop().await;
    }

    info!("Shutdown complete");
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn emit_progress(app: &AppHandle, step: &str, message: &str) {
    let _ = app.emit("startup-progress", serde_json::json!({
        "step": step,
        "message": message
    }));
}

fn emit_error(app: &AppHandle, message: &str) {
    let _ = app.emit("startup-error", serde_json::json!({
        "error": message
    }));
}

fn show_knowledge_window(app: &AppHandle, base_url: &str) {
    if let Some(window) = app.get_webview_window("knowledge") {
        if let Ok(url) = base_url.parse() {
            let _ = window.navigate(url);
        }
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn show_control_center(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("control") {
        let _ = window.show();
    }
}
