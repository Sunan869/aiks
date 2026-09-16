/// Application lifecycle management.
///
/// Startup sequence:
///  1. resolve data dir + create directories
///  2. load config from file (B10)
///  3. find/validate runtime
///  4. install/update AIKS bridge plugin into the workspace
///  5. start SiYuan Kernel
///  6. ensure Notebook
///  7. initialize AIKS Engine (with loaded config)
///  8. register AppState (including watcher handle B08)
///  9. background: migrate V4.1 canonical content, then scan + sync_and_enqueue
/// 10. start Watcher → sync_and_enqueue on events
use std::sync::Arc;

use aiks_core::bootstrap::{ensure_notebook, validate_runtime, BootstrapConfig, DevOverride};
use aiks_core::knowledge::ContentMigrationService;
use aiks_core::runtime::SiyuanRuntime;
use aiks_core::sink::SiYuanSink;
use aiks_core::watcher::WatchEvent;
use aiks_core::{AiksEngine, AiksEngineConfig};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::app_state::{config_file_path, data_dir, AppState};
use crate::bootstrap::find_runtime_root;
use crate::workbench::plugin::install_bridge_plugin;

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
        warn!(
            "Developer override active: SIYUAN_URL={:?}",
            dev_override.url
        );
        let base_url = dev_override
            .url
            .clone()
            .unwrap_or_else(|| "http://127.0.0.1:6806".to_string());
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
    let bootstrap_cfg =
        BootstrapConfig::new(runtime_root.clone(), data_dir.clone(), "AI Knowledge");
    if let Err(e) = validate_runtime(&bootstrap_cfg) {
        error!("Runtime validation failed: {}", e);
        emit_error(&app, &format!("知识引擎文件不完整：{}", e));
        return startup_without_siyuan(app, data_dir).await;
    }

    // ── V4.1: install/update Bridge Plugin before the kernel starts ───────────
    let bridge_source = runtime_root
        .join("data")
        .join("plugins")
        .join("aiks-bridge");
    match install_bridge_plugin(&bridge_source, &bootstrap_cfg.workspace) {
        Ok(target) => info!(path = %target.display(), "AIKS bridge plugin ready"),
        Err(e) => {
            warn!(error = %e, "AIKS bridge plugin could not be installed; workbench will degrade");
            emit_error(&app, &format!("知识工作台桥接组件未就绪：{}", e));
        }
    }

    // ── Start SiYuan Kernel ───────────────────────────────────────────────────
    emit_progress(&app, "starting_siyuan", "正在启动知识引擎...");
    let runtime_cfg = bootstrap_cfg.runtime_config();
    let runtime = SiyuanRuntime::new(runtime_cfg);

    let runtime_info = match runtime.start().await {
        Ok(info) => {
            info!(port = info.port, version = %info.version, "SiYuan ready");
            emit_progress(
                &app,
                "siyuan_ready",
                &format!("知识引擎就绪 (v{})", info.version),
            );
            info
        }
        Err(e) => {
            error!("SiYuan failed to start: {}", e);
            emit_error(&app, &format!("知识引擎启动失败：{}", e));
            show_control_center(&app);
            // R08: keep the collection engine alive even without SiYuan so the
            // watcher and periodic scanner can run; syncs will retry once the
            // kernel becomes available (e.g. after restart_siyuan).
            let engine = AiksEngine::initialize(AiksEngineConfig {
                config_path: config_file_path().exists().then_some(config_file_path()),
                siyuan_base_url: None,
                siyuan_token: None,
            })
            .map(Arc::new)
            .ok();
            let engine_ref = engine.clone();
            let (watcher_handle, watcher_rx) = start_watcher_for_engine(&engine_ref);
            let state = AppState {
                engine,
                siyuan_url: Arc::new(Mutex::new(None)),
                data_dir,
                _watcher_handle: Mutex::new(watcher_handle),
            };
            app.manage(state);
            set_runtime(&app, Some(runtime)).await;
            spawn_background_tasks(&app, engine_ref, watcher_rx);
            return Ok(());
        }
    };

    let base_url = runtime_info.base_url.clone();

    emit_progress(&app, "notebook", "正在准备知识库...");
    match ensure_notebook(&base_url, "AI Knowledge").await {
        Ok(nb) => info!(notebook_id = %nb.id, "Notebook ready"),
        Err(e) => warn!("Could not ensure notebook: {}", e),
    }

    let config_path = config_file_path();
    let engine_config = AiksEngineConfig {
        config_path: if config_path.exists() {
            Some(config_path)
        } else {
            None
        },
        siyuan_base_url: Some(base_url.clone()),
        siyuan_token: None,
    };

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

    let (watcher_handle, watcher_rx) = start_watcher_for_engine(&Some(engine.clone()));

    let state = AppState {
        engine: Some(engine.clone()),
        siyuan_url: Arc::new(Mutex::new(Some(base_url.clone()))),
        data_dir,
        _watcher_handle: Mutex::new(watcher_handle),
    };
    app.manage(state);
    set_runtime(&app, Some(runtime)).await;

    emit_progress(&app, "ready", "AIKS 已就绪");
    show_control_center(&app);

    spawn_startup_migration_then_tasks(&app, engine, None, watcher_rx);

    info!("AIKS startup complete");
    Ok(())
}

fn start_watcher_for_engine(
    engine: &Option<Arc<AiksEngine>>,
) -> (
    Option<aiks_core::watcher::WatcherHandle>,
    tokio::sync::mpsc::UnboundedReceiver<WatchEvent>,
) {
    let (watcher_tx, watcher_rx) = tokio::sync::mpsc::unbounded_channel();
    match engine {
        Some(engine) => {
            if !engine.config().sync.watch_enabled {
                info!("[WATCHER] Disabled by config (sync.watch_enabled = false)");
                return (None, watcher_rx);
            }
            let watcher = engine.create_watcher(watcher_tx);
            match watcher.start() {
                Ok(h) => {
                    info!("File watcher started");
                    (Some(h), watcher_rx)
                }
                Err(e) => {
                    warn!("File watcher failed to start: {}", e);
                    (None, watcher_rx)
                }
            }
        }
        None => (None, watcher_rx),
    }
}

fn spawn_startup_migration_then_tasks(
    app: &AppHandle,
    engine: Arc<AiksEngine>,
    external_token: Option<String>,
    watcher_rx: tokio::sync::mpsc::UnboundedReceiver<WatchEvent>,
) {
    let app_bg = app.clone();
    tauri::async_runtime::spawn(async move {
        let config = engine.config();
        let sink_result = if let Some(token) = external_token {
            let mut siyuan = config.siyuan.clone();
            siyuan.base_url = engine.siyuan_base_url().to_string();
            siyuan.token = token;
            SiYuanSink::new(siyuan)
        } else {
            SiYuanSink::embedded(engine.siyuan_base_url(), &config.siyuan.notebook_name)
        };

        match sink_result {
            Ok(sink) if sink.health_check().await => {
                let db = engine.db();
                let migration = ContentMigrationService::new(db.as_ref());
                let _ = app_bg.emit(
                    "content-migration",
                    serde_json::json!({"status": "running"}),
                );
                match migration.migrate(&sink).await {
                    Ok(stats) => {
                        info!(
                            total = stats.total,
                            migrated = stats.migrated,
                            reused = stats.reused,
                            conflicts = stats.conflicts,
                            failed = stats.failed,
                            "[MIGRATION] V4.1 canonical content migration complete"
                        );
                        let _ = app_bg.emit(
                            "content-migration",
                            serde_json::json!({
                                "status": "completed",
                                "total": stats.total,
                                "migrated": stats.migrated,
                                "reused": stats.reused,
                                "conflicts": stats.conflicts,
                                "failed": stats.failed,
                            }),
                        );
                    }
                    Err(e) => {
                        warn!(error = %e, "[MIGRATION] V4.1 content migration failed");
                        let _ = app_bg.emit(
                            "content-migration",
                            serde_json::json!({"status": "failed", "error": e.to_string()}),
                        );
                    }
                }
            }
            Ok(_) => {
                warn!("[MIGRATION] SiYuan health check failed; migration deferred");
                let _ = app_bg.emit(
                    "content-migration",
                    serde_json::json!({"status": "deferred", "reason": "siyuan_unhealthy"}),
                );
            }
            Err(e) => {
                warn!(error = %e, "[MIGRATION] could not initialize SiYuan sink");
                let _ = app_bg.emit(
                    "content-migration",
                    serde_json::json!({"status": "deferred", "reason": e.to_string()}),
                );
            }
        }

        spawn_background_tasks(&app_bg, Some(engine), watcher_rx);
    });
}

fn spawn_background_tasks(
    app: &AppHandle,
    engine: Option<Arc<AiksEngine>>,
    mut watcher_rx: tokio::sync::mpsc::UnboundedReceiver<WatchEvent>,
) {
    let Some(engine) = engine else {
        return;
    };

    {
        let engine_bg = engine.clone();
        let app_bg = app.clone();
        tauri::async_runtime::spawn(async move {
            info!("[STARTUP] Beginning initial scan...");
            let _ = app_bg.emit("sync-status", serde_json::json!({"status": "scanning"}));

            let opts = aiks_core::SyncOptions {
                source_filter: None,
                dry_run: false,
                overwrite: false,
            };

            match engine_bg.sync_and_enqueue_extraction(opts).await {
                Ok(stats) => {
                    info!(
                        "[SYNC] Complete: new={} updated={} unchanged={} failed={}",
                        stats.new_count,
                        stats.updated_count,
                        stats.unchanged_count,
                        stats.failed_count
                    );
                    let _ = app_bg.emit(
                        "sync-complete",
                        serde_json::json!({
                            "discovered": stats.discovered,
                            "new": stats.new_count,
                            "updated": stats.updated_count,
                            "unchanged": stats.unchanged_count,
                            "failed": stats.failed_count,
                        }),
                    );
                }
                Err(e) => {
                    error!("[SYNC] Initial sync failed: {}", e);
                    let _ = app_bg.emit("sync-error", serde_json::json!({"error": e.to_string()}));
                }
            }
        });
    }

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
                match engine_w.sync_and_enqueue_extraction(opts).await {
                    Ok(s) if s.new_count + s.updated_count > 0 => {
                        let _ = app_w.emit(
                            "sync-complete",
                            serde_json::json!({
                                "new": s.new_count, "updated": s.updated_count
                            }),
                        );
                    }
                    Err(e) => warn!("Watcher sync error: {}", e),
                    _ => {}
                }
            }
        });
    }

    {
        let engine_p = engine.clone();
        tauri::async_runtime::spawn(async move {
            let interval_secs = engine_p.config().sync.scan_interval_seconds.max(30);
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                info!("[SCAN] Periodic scan triggered (every {}s)", interval_secs);
                let opts = aiks_core::SyncOptions {
                    source_filter: None,
                    dry_run: false,
                    overwrite: false,
                };
                match engine_p.sync_and_enqueue_extraction(opts).await {
                    Ok(s) => {
                        info!(
                            "[SCAN] Periodic sync: new={} updated={} unchanged={} failed={}",
                            s.new_count, s.updated_count, s.unchanged_count, s.failed_count
                        );
                    }
                    Err(e) => warn!("[SCAN] Periodic sync failed: {}", e),
                }
            }
        });
    }
}

async fn startup_without_siyuan(
    app: AppHandle,
    data_dir: std::path::PathBuf,
) -> anyhow::Result<()> {
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
        config_path: if config_path.exists() {
            Some(config_path)
        } else {
            None
        },
        siyuan_base_url: Some(base_url.clone()),
        siyuan_token: token.clone(),
    };

    let engine = Arc::new(AiksEngine::initialize(engine_config)?);
    let (watcher_handle, watcher_rx) = start_watcher_for_engine(&Some(engine.clone()));

    let state = AppState {
        engine: Some(engine.clone()),
        siyuan_url: Arc::new(Mutex::new(Some(base_url.clone()))),
        data_dir,
        _watcher_handle: Mutex::new(watcher_handle),
    };
    app.manage(state);
    show_control_center(&app);

    spawn_startup_migration_then_tasks(&app, engine, token, watcher_rx);
    Ok(())
}

pub async fn shutdown(app: &AppHandle) {
    info!("Shutting down AIKS...");
    if let Some(runtime_container) = app.try_state::<Arc<Mutex<Option<SiyuanRuntime>>>>() {
        let mut guard = runtime_container.lock().await;
        if let Some(runtime) = guard.as_mut() {
            if let Err(e) = runtime.stop().await {
                error!("Error stopping SiYuan: {}", e);
            }
        }
        *guard = None;
    }
    info!("AIKS shutdown complete");
}

async fn set_runtime(app: &AppHandle, runtime: Option<SiyuanRuntime>) {
    if let Some(runtime_container) = app.try_state::<Arc<Mutex<Option<SiyuanRuntime>>>>() {
        *runtime_container.lock().await = runtime;
    }
}

fn emit_progress(app: &AppHandle, stage: &str, message: &str) {
    let _ = app.emit(
        "startup-progress",
        serde_json::json!({"stage": stage, "message": message}),
    );
}

fn emit_error(app: &AppHandle, message: &str) {
    let _ = app.emit("startup-error", serde_json::json!({"message": message}));
}

fn show_control_center(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("control") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
