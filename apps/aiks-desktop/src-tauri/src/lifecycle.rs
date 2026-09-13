/// Application lifecycle management.
///
/// Startup sequence:
///  1. resolve data dir + create directories
///  2. load config from file (B10)
///  3. find/validate runtime
///  4. start SiYuan Kernel
///  5. ensure Notebook
///  6. initialize AIKS Engine (with loaded config)
///  7. register AppState (including watcher handle B08)
///  8. background: scan + sync_and_enqueue (B09)
///  9. start Watcher → sync_and_enqueue on events (B08, B09)
use std::sync::Arc;

use aiks_core::bootstrap::{ensure_notebook, validate_runtime, BootstrapConfig, DevOverride};
use aiks_core::runtime::{SiyuanRuntime, SiyuanRuntimeConfig, RuntimeState};
use aiks_core::{AiksEngine, AiksEngineConfig};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::app_state::{config_file_path, data_dir, AppState};
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

    // ── Check developer override ───────────────────────────────────────────────
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
    let bootstrap_cfg = BootstrapConfig::new(runtime_root.clone(), data_dir.clone(), "AI Knowledge");
    if let Err(e) = validate_runtime(&bootstrap_cfg) {
        error!("Runtime validation failed: {}", e);
        emit_error(&app, &format!("知识引擎文件不完整：{}", e));
        return startup_without_siyuan(app, data_dir).await;
    }

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
            show_control_center(&app);
            let state = AppState {
                engine: None,
                siyuan_url: Arc::new(Mutex::new(None)),
                data_dir,
                _watcher_handle: Mutex::new(None),
            };
            app.manage(state);
            // B07: Only manage the runtime once via a shared container
            app.manage(Arc::new(Mutex::new(Some(runtime))));
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

    // ── B10: Load config from file ────────────────────────────────────────────
    let config_path = config_file_path();
    let engine_config = AiksEngineConfig {
        config_path: if config_path.exists() { Some(config_path) } else { None },
        siyuan_base_url: Some(base_url.clone()),
        siyuan_token: None, // embedded mode
    };

    // ── Initialize AIKS Engine ────────────────────────────────────────────────
    emit_progress(&app, "init_engine", "正在初始化采集引擎...");
    let engine = match AiksEngine::initialize(engine_config) {
        Ok(e) => e,
        Err(e) => {
            error!("Engine init failed: {}", e);
            emit_error(&app, &format!("初始化失败：{}", e));
            return Ok(());
        }
    };
    let engine = Arc::new(engine);

    // ── B08: Set up Watcher — hold handle in AppState ─────────────────────────
    let (watcher_tx, mut watcher_rx) = tokio::sync::mpsc::unbounded_channel();
    let watcher = engine.create_watcher(watcher_tx);
    let watcher_handle = match watcher.start() {
        Ok(h) => {
            info!("File watcher started");
            Some(h)
        }
        Err(e) => {
            warn!("File watcher failed to start: {}", e);
            None
        }
    };

    // ── Register state (B07: only one manage per type) ────────────────────────
    let state = AppState {
        engine: Some(engine.clone()),
        siyuan_url: Arc::new(Mutex::new(Some(base_url.clone()))),
        data_dir,
        _watcher_handle: Mutex::new(watcher_handle), // B08: Mutex wraps for Sync
    };
    app.manage(state);
    // B07: Manage runtime once via Option container
    app.manage(Arc::new(Mutex::new(Some(runtime))));

    emit_progress(&app, "ready", "AIKS 已就绪");
    show_control_center(&app);

    // ── B09: Background initial scan + sync_and_enqueue ──────────────────────
    {
        let engine_bg = engine.clone();
        let app_bg = app.clone();
        tauri::async_runtime::spawn(async move {
            info!("[STARTUP] Beginning initial scan...");
            let _ = app_bg.emit("sync-status", serde_json::json!({"status": "scanning"}));

            let opts = aiks_core::SyncOptions { source_filter: None, dry_run: false, overwrite: false };

            // B09: Use sync_and_enqueue_extraction (not just sync)
            match engine_bg.sync_and_enqueue_extraction(opts).await {
                Ok(stats) => {
                    info!(
                        "[SYNC] Complete: new={} updated={} unchanged={} failed={}",
                        stats.new_count, stats.updated_count, stats.unchanged_count, stats.failed_count
                    );
                    let _ = app_bg.emit("sync-complete", serde_json::json!({
                        "discovered": stats.discovered,
                        "new": stats.new_count,
                        "updated": stats.updated_count,
                        "unchanged": stats.unchanged_count,
                        "failed": stats.failed_count,
                    }));
                }
                Err(e) => {
                    error!("[SYNC] Initial sync failed: {}", e);
                    let _ = app_bg.emit("sync-error", serde_json::json!({"error": e.to_string()}));
                }
            }
        });
    }

    // ── B08: Watcher event loop ────────────────────────────────────────────────
    {
        let engine_w = engine.clone();
        let app_w = app.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(event) = watcher_rx.recv().await {
                let source = event.source.as_str().to_string();
                let opts = aiks_core::SyncOptions {
                    source_filter: Some(source),
                    dry_run: false,
                    overwrite: false,
                };
                // B09: Watcher also uses sync_and_enqueue_extraction
                match engine_w.sync_and_enqueue_extraction(opts).await {
                    Ok(s) if s.new_count + s.updated_count > 0 => {
                        let _ = app_w.emit("sync-complete", serde_json::json!({
                            "new": s.new_count, "updated": s.updated_count
                        }));
                    }
                    Err(e) => warn!("Watcher sync error: {}", e),
                    _ => {}
                }
            }
        });
    }

    info!("AIKS startup complete");
    Ok(())
}

/// Startup without SiYuan
async fn startup_without_siyuan(app: AppHandle, data_dir: std::path::PathBuf) -> anyhow::Result<()> {
    let state = AppState {
        engine: None,
        siyuan_url: Arc::new(Mutex::new(None)),
        data_dir,
        _watcher_handle: Mutex::new(None),
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

    let config_path = config_file_path();
    let engine_config = AiksEngineConfig {
        config_path: if config_path.exists() { Some(config_path) } else { None },
        siyuan_base_url: Some(base_url.clone()),
        siyuan_token: token,
    };

    let engine = AiksEngine::initialize(engine_config)?;
    let state = AppState {
        engine: Some(Arc::new(engine)),
        siyuan_url: Arc::new(Mutex::new(Some(base_url.clone()))),
        data_dir,
        _watcher_handle: Mutex::new(None),
    };
    app.manage(state);
    show_control_center(&app);
    Ok(())
}

// ── Shutdown ───────────────────────────────────────────────────────────────────

/// Graceful shutdown.
pub async fn shutdown(app: &AppHandle) {
    info!("Shutting down AIKS...");
    // B07: Stop the real runtime via the shared container
    if let Some(runtime_container) = app.try_state::<Arc<Mutex<Option<SiyuanRuntime>>>>() {
        let mut lock = runtime_container.lock().await;
        if let Some(runtime) = lock.take() {
            runtime.stop().await;
        }
    }
    info!("Shutdown complete");
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn emit_progress(app: &AppHandle, step: &str, message: &str) {
    let _ = app.emit("startup-progress", serde_json::json!({ "step": step, "message": message }));
}

fn emit_error(app: &AppHandle, message: &str) {
    let _ = app.emit("startup-error", serde_json::json!({ "error": message }));
}

fn show_control_center(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("control") {
        let _ = window.show();
    }
}
