mod ai_assist_commands;
mod app_state;
mod bootstrap;
mod commands;
mod diagnostics;
mod embedding_commands;
mod knowledge_commands;
mod lifecycle;
mod provider_commands;
mod search_commands;
pub mod session_workbench;
mod share_import_commands;
mod storage_commands;
mod tray;
mod workbench;

use std::fs::{self, OpenOptions};
use std::sync::{atomic::Ordering, Arc};
use tokio::sync::Mutex;

use aiks_core::runtime::SiyuanRuntime;
use tauri::{Manager, WindowEvent};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn run() {
    let log_dir = app_state::data_dir().join("logs");
    let log_path = log_dir.join("aiks.log");
    let file_layer = fs::create_dir_all(&log_dir)
        .ok()
        .and_then(|_| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .ok()
        })
        .map(|file| {
            tracing_subscriber::fmt::layer()
                .compact()
                .with_ansi(false)
                .with_target(false)
                .with_writer(std::sync::Mutex::new(file))
        });

    let console_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_target(false)
        .with_writer(std::io::stdout);

    tracing_subscriber::registry()
        .with(EnvFilter::new("aiks=debug,info"))
        .with(console_layer)
        .with(file_layer)
        .init();

    tracing::info!(log_path = %log_path.display(), "AIKS file logging initialized");

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
                match storage_commands::prepare_storage_before_startup(&app_handle).await {
                    Ok(true) => match lifecycle::startup(app_handle.clone()).await {
                        Ok(()) => tracing::info!("AIKS startup complete"),
                        Err(e) => {
                            tracing::error!("Startup failed: {}", e);
                            if let Some(window) = app_handle.get_webview_window("control") {
                                let _ = window.show();
                            }
                        }
                    },
                    Ok(false) => {
                        tracing::info!("Waiting for the user to select an AIKS data directory");
                    }
                    Err(e) => {
                        tracing::error!("Storage preparation failed: {}", e);
                        match lifecycle::startup(app_handle.clone()).await {
                            Ok(()) => {
                                tracing::info!("AIKS startup complete after storage fallback")
                            }
                            Err(startup_error) => {
                                tracing::error!(
                                    "Startup failed after storage fallback: {}",
                                    startup_error
                                );
                                if let Some(window) = app_handle.get_webview_window("control") {
                                    let _ = window.show();
                                }
                            }
                        }
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            provider_commands::get_source_descriptors,
            provider_commands::save_provider_settings,
            commands::get_status,
            commands::scan_sources,
            commands::sync_now,
            commands::get_settings,
            commands::save_settings,
            commands::get_doctor,
            commands::open_data_folder,
            commands::restart_app,
            commands::restart_siyuan,
            commands::get_siyuan_url,
            commands::get_sessions,
            commands::get_sync_history,
            commands::get_ai_status,
            commands::test_ai_connection,
            commands::test_ai_connection_with_settings,
            embedding_commands::get_embedding_settings,
            embedding_commands::save_embedding_settings,
            embedding_commands::test_embedding_connection_with_settings,
            embedding_commands::rebuild_semantic_index,
            storage_commands::get_data_storage_settings,
            storage_commands::pick_data_directory,
            storage_commands::set_data_storage_root,
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
            // V4.2 Unified Search + AI Assist
            search_commands::search_all_v42,
            ai_assist_commands::assist_knowledge_v42,
            // Public AI web conversation import
            share_import_commands::fetch_chatgpt_share_html,
            share_import_commands::persist_share_conversation,
            share_import_commands::import_share_url_browser,
            // V4.1 SiYuan Embedded Workbench
            diagnostics::get_v41_diagnostics,
            session_workbench::get_session_workbench_doc_id,
            workbench::commands::get_workbench_status,
            workbench::commands::mount_workbench,
            workbench::commands::show_workbench,
            workbench::commands::reload_workbench,
            workbench::commands::hide_workbench,
            workbench::commands::set_workbench_mode,
            workbench::commands::show_workbench_root,
            workbench::commands::open_siyuan_document,
            workbench::commands::open_siyuan_block,
            workbench::commands::refresh_siyuan_document,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let close_to_tray = window
                    .app_handle()
                    .try_state::<app_state::AppState>()
                    .map(|state| state.close_to_tray.load(Ordering::Acquire))
                    .unwrap_or(true);

                if close_to_tray || window.label() != "control" {
                    window.hide().ok();
                    api.prevent_close();
                } else {
                    api.prevent_close();
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        lifecycle::shutdown(&app).await;
                        app.exit(0);
                    });
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error running AIKS Desktop");
}
