//! A mode is selected before opening any legacy engine, worker or database.
mod legacy;
use crate::service_desktop::ServiceDesktop;
use aiks_core::{config::BackendMode, storage::ownership::BusinessDbLease, Config};
use std::sync::{atomic::Ordering, Arc};
use tauri::{AppHandle, Emitter, Manager};

pub fn selected_config() -> anyhow::Result<Config> {
    let path = crate::app_state::config_file_path();
    let mut config = if path.exists() {
        Config::from_file(&path)?
    } else {
        Config::default()
    };
    if cfg!(debug_assertions) {
        if let Ok(value) = std::env::var("AIKS_BACKEND_MODE") {
            config.backend.mode = BackendMode::parse(&value)?;
        }
    }
    Ok(config)
}

pub async fn startup(app: AppHandle) -> anyhow::Result<()> {
    let config = match selected_config() {
        Ok(config) => config,
        Err(error) => {
            let _ = app.emit(
                "startup-error",
                serde_json::json!({"error":"Invalid backend configuration"}),
            );
            return Err(error);
        }
    };
    match config.backend.mode {
        BackendMode::Legacy => {
            // Legacy entrypoints participate in the same cross-process lease.
            let lease = BusinessDbLease::acquire(&config.state_db_path())?;
            anyhow::ensure!(app.manage(lease), "Legacy writer already initialized");
            legacy::startup(app).await
        }
        BackendMode::ServiceLocal => {
            let state = Arc::new(ServiceDesktop::new(config));
            anyhow::ensure!(
                app.manage(state.clone()),
                "Service desktop already initialized"
            );
            if let Some(window) = app.get_webview_window("control") {
                let _ = window.show();
            }
            state.start(&app).await
        }
    }
}
pub async fn shutdown(app: &AppHandle) {
    if let Some(state) = app.try_state::<Arc<ServiceDesktop>>() {
        state.shutdown().await;
    } else {
        legacy::shutdown(app).await;
    }
}
pub fn close_to_tray(app: &AppHandle) -> bool {
    if let Some(state) = app.try_state::<Arc<ServiceDesktop>>() {
        return state.close_to_tray;
    }
    app.try_state::<crate::app_state::AppState>()
        .map(|s| s.close_to_tray.load(Ordering::Acquire))
        .unwrap_or(true)
}
