mod app_state;
mod bootstrap;
mod commands;
mod lifecycle;
mod tray;

use std::sync::Arc;
use tokio::sync::Mutex;

use aiks_core::runtime::SiyuanRuntime;
use aiks_core::runtime::SiyuanRuntimeConfig;
use tauri::{Manager, WindowEvent};
use tracing_subscriber::EnvFilter;

use app_state::data_dir;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("aiks=debug,info"))
        .with_target(false)
        .compact()
        .init();

    // Create a placeholder runtime that we'll replace after bootstrap
    // We need to manage it before setup because commands reference it
    let placeholder_runtime = SiyuanRuntime::new(SiyuanRuntimeConfig::new(
        std::path::PathBuf::from("."),
        std::path::PathBuf::from("."),
        &data_dir(),
    ));

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(Arc::new(Mutex::new(placeholder_runtime)))
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Set up system tray
            tray::setup_tray(app)?;

            // Bootstrap in background
            tauri::async_runtime::spawn(async move {
                match lifecycle::startup(app_handle.clone()).await {
                    Ok(()) => tracing::info!("AIKS startup complete"),
                    Err(e) => {
                        tracing::error!("Startup failed: {}", e);
                        if let Some(window) = app_handle.get_webview_window("control") {
                            let _ = window.show();
                        }
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::scan_sources,
            commands::sync_now,
            commands::get_settings,
            commands::save_settings,
            commands::get_doctor,
            commands::open_data_folder,
            commands::restart_siyuan,
            commands::get_siyuan_url,
            commands::get_sessions,
            commands::get_sync_history,
            // AI Knowledge commands
            commands::get_ai_status,
            commands::test_ai_connection,
            commands::extract_session_now,
            commands::get_knowledge_stats,
            commands::get_recent_knowledge,
            commands::open_knowledge_window,
            // V2.5: Unified status + sync-with-extraction
            commands::get_full_status,
            commands::sync_and_extract,
            // V3: Pipeline + Knowledge + Search
            commands::list_pipeline_runs,
            commands::get_pipeline_detail,
            commands::get_pipeline_stats,
            commands::list_sessions_v3,
            commands::list_knowledge,
            commands::search_knowledge,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Minimize to tray instead of closing (spec §69)
                window.hide().ok();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error running AIKS Desktop");
}
