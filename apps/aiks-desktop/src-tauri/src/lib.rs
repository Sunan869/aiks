mod app_state;
mod bootstrap;
mod commands;
mod knowledge_commands;
mod lifecycle;
mod tray;
mod workbench;

use std::sync::Arc;
use tokio::sync::Mutex;

use aiks_core::runtime::SiyuanRuntime;
use tauri::{Manager, WindowEvent};
use tracing_subscriber::EnvFilter;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("aiks=debug,info"))
        .with_target(false)
        .compact()
        .init();

    let runtime_container: Arc<Mutex<Option<SiyuanRuntime>>> = Arc::new(Mutex::new(None));

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(runtime_container)
        .manage(workbench::controller::WorkbenchController::new())
        .setup(|app| {
            let app_handle = app.handle().clone();
            tray::setup_tray(app)?;
            workbench::events::register(&app_handle);
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
            commands::get_ai_status,
            commands::test_ai_connection,
            commands::extract_session_now,
            commands::get_knowledge_stats,
            commands::get_recent_knowledge,
            commands::open_knowledge_window,
            commands::get_full_status,
            commands::sync_and_extract,
            commands::list_pipeline_runs,
            commands::get_pipeline_detail,
            commands::get_pipeline_stats,
            commands::list_sessions_v3,
            commands::list_knowledge,
            commands::search_knowledge,
            commands::get_session_detail,
            commands::get_knowledge_detail,
            commands::run_pipeline_for_session,
            commands::backfill_extractions,
            commands::sync_knowledge_to_siyuan,
            commands::hybrid_search,
            // V4 Native Knowledge Workbench
            knowledge_commands::list_knowledge_v4,
            knowledge_commands::get_knowledge_detail_v4,
            knowledge_commands::create_knowledge,
            knowledge_commands::update_knowledge,
            knowledge_commands::set_knowledge_favorite,
            knowledge_commands::archive_knowledge,
            knowledge_commands::restore_knowledge,
            knowledge_commands::search_knowledge_v4,
            knowledge_commands::publish_knowledge,
            // V4.1 SiYuan Embedded Workbench
            workbench::commands::get_workbench_status,
            workbench::commands::show_workbench,
            workbench::commands::hide_workbench,
            workbench::commands::set_workbench_mode,
            workbench::commands::show_workbench_root,
            workbench::commands::open_siyuan_document,
            workbench::commands::open_siyuan_block,
            workbench::commands::show_workbench_backlinks,
            workbench::commands::show_workbench_outline,
            workbench::commands::show_workbench_database,
            workbench::commands::show_workbench_graph,
            workbench::commands::show_workbench_search,
            workbench::commands::refresh_siyuan_document,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                window.hide().ok();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error running AIKS Desktop");
}
