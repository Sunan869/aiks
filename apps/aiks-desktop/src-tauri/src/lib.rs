mod ai_assist_commands;
mod app_state;
mod bootstrap;
mod commands;
mod diagnostics;
mod embedding_commands;
mod knowledge_commands;
mod lifecycle;
mod search_commands;
pub mod session_workbench;
mod storage_commands;
mod tray;
mod workbench;

use tauri::{Emitter, Manager};
use tauri_plugin_log::{Target, TargetKind};

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                ])
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            if let Err(e) = bootstrap::ensure_siyuan_runtime(app.handle()) {
                tracing::warn!("SiYuan runtime bootstrap failed: {}", e);
            }

            if let Err(e) = tray::setup_tray(app.handle()) {
                tracing::warn!("Tray setup failed: {}", e);
            }

            let app_handle = app.handle().clone();
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
                        tracing::info!("AIKS startup paused for initial storage selection")
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
            commands::list_sessions,
            commands::get_session,
            commands::get_processing,
            commands::get_processing_detail,
            commands::get_overview,
            commands::get_sources,
            commands::get_sync_runs,
            commands::get_sync_run,
            commands::get_knowledge,
            commands::get_knowledge_detail,
            commands::list_deleted_knowledge,
            commands::restore_knowledge,
            commands::purge_deleted_knowledge,
            commands::search_knowledge,
            commands::semantic_search_knowledge,
            commands::reindex_knowledge,
            commands::run_sync,
            commands::retry_job,
            commands::dismiss_job,
            commands::get_settings,
            commands::save_settings,
            commands::test_ai_connection,
            commands::test_embedding_connection,
            commands::list_ai_models,
            commands::open_file_or_dir,
            commands::get_logs,
            commands::resolve_chat_target,
            commands::get_runtime_health,
            commands::get_deleted_knowledge_stats,
            commands::get_session_index_status,
            commands::get_session_index_queue,
            commands::rebuild_session_index,
            commands::unified_search,
            commands::get_knowledge_index_status,
            commands::rebuild_knowledge_index,
            commands::publish_knowledge,
            commands::update_knowledge,
            commands::delete_knowledge,
            commands::restore_knowledge_version,
            commands::list_knowledge_versions,
            commands::list_knowledge_sources,
            commands::open_knowledge_source,
            knowledge_commands::list_knowledge,
            knowledge_commands::get_knowledge,
            knowledge_commands::update_knowledge,
            knowledge_commands::publish_knowledge,
            knowledge_commands::delete_knowledge,
            knowledge_commands::list_knowledge_sources,
            knowledge_commands::open_knowledge_source,
            knowledge_commands::list_knowledge_versions,
            knowledge_commands::restore_knowledge_version,
            search_commands::unified_search,
            ai_assist_commands::ai_assist_improve,
            ai_assist_commands::ai_assist_summarize,
            ai_assist_commands::ai_assist_rewrite,
            ai_assist_commands::ai_assist_continue,
            diagnostics::get_diagnostics,
            diagnostics::repair_index,
            diagnostics::retry_failed_jobs,
            embedding_commands::embedding_config_status,
            embedding_commands::set_embedding_enabled,
            embedding_commands::get_embedding_service_status,
            embedding_commands::rebuild_all_semantic_indexes,
            storage_commands::get_data_storage_settings,
            storage_commands::pick_data_directory,
            storage_commands::set_data_storage_root,
            storage_commands::open_data_storage_root,
            workbench::commands::get_workbench_status,
            workbench::commands::start_workbench,
            workbench::commands::stop_workbench,
            workbench::commands::restart_workbench,
            workbench::commands::show_workbench,
            workbench::commands::hide_workbench,
            workbench::commands::open_workbench_notebook,
            workbench::commands::open_workbench_doc,
            workbench::commands::reload_workbench,
            workbench::commands::get_workbench_readiness,
            workbench::commands::get_workbench_route,
            workbench::commands::set_workbench_route,
            workbench::commands::get_workbench_url,
            workbench::commands::get_workbench_log_tail,
            workbench::commands::get_workbench_config,
            workbench::commands::save_workbench_config,
        ])
        .on_window_event(|window, event| {
            use tauri::WindowEvent;
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    if window.label() == "control" {
                        let should_hide = window
                            .try_state::<app_state::AppState>()
                            .map(|state| {
                                state
                                    .close_to_tray
                                    .load(std::sync::atomic::Ordering::Relaxed)
                            })
                            .unwrap_or(true);
                        if should_hide {
                            api.prevent_close();
                            let _ = window.hide();
                        }
                    }
                }
                WindowEvent::Destroyed => {
                    if window.label() == "control" {
                        let app_handle = window.app_handle().clone();
                        tauri::async_runtime::spawn(async move {
                            lifecycle::shutdown(&app_handle).await;
                        });
                    }
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error running AIKS Desktop");
}
