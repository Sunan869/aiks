use crate::app_state::{config_file_path, AppState};
use aiks_core::{
    providers::{
        catalog::{descriptors, ProviderDescriptor},
        settings,
    },
    Config,
};
use serde::Serialize;
use tauri::{State, Webview};

pub static CONFIG_SAVE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Serialize)]
pub struct SourceDescriptor {
    #[serde(flatten)]
    pub descriptor: ProviderDescriptor,
    pub restart_required: bool,
}
fn trusted(webview: &Webview) -> Result<(), String> {
    if webview.label() == "control" {
        Ok(())
    } else {
        Err("Provider settings are available only in the trusted AIKS shell".into())
    }
}

#[tauri::command]
pub async fn get_source_descriptors(
    webview: Webview,
    state: State<'_, AppState>,
) -> Result<Vec<SourceDescriptor>, String> {
    trusted(&webview)?;
    let engine = state.engine().ok_or("Engine not initialized")?;
    let runtime = engine.provider_descriptors().await;
    let path = config_file_path();
    let saved = if path.exists() {
        Config::from_file(&path).map_err(|_| "Cannot read saved provider configuration")?
    } else {
        Config::default()
    };
    let configured = descriptors(&saved, &[]);
    Ok(runtime
        .into_iter()
        .map(|mut descriptor| {
            let mut restart_required = false;
            if let Some(saved) = configured.iter().find(|d| d.key == descriptor.key) {
                restart_required =
                    saved.enabled != descriptor.enabled || saved.paths != descriptor.paths;
                descriptor.enabled = saved.enabled;
                descriptor.paths = saved.paths.clone();
            }
            SourceDescriptor {
                descriptor,
                restart_required,
            }
        })
        .collect())
}

#[tauri::command]
pub async fn save_provider_settings(
    webview: Webview,
    source: String,
    enabled: bool,
    paths: Vec<String>,
) -> Result<(), String> {
    trusted(&webview)?;
    let _guard = CONFIG_SAVE_LOCK.lock().await;
    let path = config_file_path();
    // The command writes only AIKS configuration, never a third-party store.
    tokio::task::spawn_blocking(move || {
        settings::save_provider_settings(&path, &source, enabled, &paths)
    })
    .await
    .map_err(|_| "Provider settings worker failed".to_string())?
    .map_err(|e| e.to_string())
}
