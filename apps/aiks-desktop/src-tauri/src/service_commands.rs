//! Small, typed native actions; no arbitrary endpoint, SQL, filesystem or process proxy.
use crate::service_desktop::ServiceDesktop;
use aiks_core::config::BackendMode;
use aiks_core::service::SnapshotReceipt;
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, Manager, Webview};

fn trusted(view: &Webview) -> Result<(), String> {
    let url = view.url().map_err(|_| "untrusted_shell")?;
    let packaged = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"));
    let dev = cfg!(debug_assertions)
        && url.scheme() == "http"
        && url.host_str() == Some("localhost")
        && url.port() == Some(1420);
    if view.label() != "control" || !(packaged || dev) {
        return Err("untrusted_shell".into());
    }
    Ok(())
}
fn state(app: &AppHandle) -> Result<Arc<ServiceDesktop>, String> {
    app.try_state::<Arc<ServiceDesktop>>()
        .map(|s| s.inner().clone())
        .ok_or("service_not_ready".into())
}
#[tauri::command]
pub async fn service_status(app: AppHandle, webview: Webview) -> Result<Value, String> {
    trusted(&webview)?;
    if let Ok(state) = state(&app) {
        return Ok(state.status().await);
    }
    let mode = crate::lifecycle::selected_config()
        .map_err(|_| "invalid_backend_configuration")?
        .backend
        .mode;
    Ok(
        json!({"mode":if mode==BackendMode::Legacy{"legacy"}else{"service_local"},"phase":"starting"}),
    )
}
#[tauri::command]
pub async fn service_collect_selected(app: AppHandle, webview: Webview, sources: Vec<String>) -> Result<Value, String> {
    trusted(&webview)?;
    state(&app)?.collect(sources).await
}
#[tauri::command]
pub async fn service_uploads(app: AppHandle, webview: Webview) -> Result<Value, String> {
    trusted(&webview)?;
    let (client, outbox) = state(&app)?.connection().await?;
    let identity = client.connection().clone();
    tokio::task::spawn_blocking(move || {
        let rows = outbox.statuses(identity.instance_id(), identity.space_id()).map_err(|e| e.to_string())?;
        serde_json::to_value(rows).map_err(|_| "invalid_upload_state".into())
    }).await.map_err(|_| "collector_storage_unavailable".to_string())?
}
#[tauri::command]
pub async fn service_get_receipt(app: AppHandle, webview: Webview, id: String) -> Result<SnapshotReceipt, String> {
    trusted(&webview)?;
    state(&app)?.connection().await?.0.get_receipt(&id).await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn service_get_job(app: AppHandle, webview: Webview, id: String) -> Result<Value, String> {
    trusted(&webview)?;
    state(&app)?.connection().await?.0.get_job(&id).await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn service_search(app: AppHandle, webview: Webview, query: String, corpus: Option<aiks_core::search::SearchCorpus>) -> Result<Value, String> {
    trusted(&webview)?;
    state(&app)?.connection().await?.0.search_corpus(&query, corpus).await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn service_sessions(app: AppHandle, webview: Webview, offset: Option<usize>) -> Result<Value, String> {
    trusted(&webview)?;
    state(&app)?.connection().await?.0.sessions_page(offset.unwrap_or(0)).await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn service_session(app: AppHandle, webview: Webview, id: String) -> Result<Value, String> {
    trusted(&webview)?;
    state(&app)?.connection().await?.0.session(&id).await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn service_knowledge_list(app: AppHandle, webview: Webview, offset: Option<usize>) -> Result<Value, String> {
    trusted(&webview)?;
    state(&app)?.connection().await?.0.knowledge_page(offset.unwrap_or(0)).await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn service_knowledge(app: AppHandle, webview: Webview, id: String) -> Result<Value, String> {
    trusted(&webview)?;
    state(&app)?.connection().await?.0.knowledge(&id).await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn service_ui_preferences(app: AppHandle, webview: Webview) -> Result<Value, String> {
    trusted(&webview)?;
    serde_json::to_value(state(&app)?.ui_preferences().await?).map_err(|_| "invalid_preferences".into())
}
#[tauri::command]
pub async fn service_finish_onboarding(app: AppHandle, webview: Webview, skipped: bool) -> Result<Value, String> {
    trusted(&webview)?;
    serde_json::to_value(state(&app)?.finish_onboarding(skipped).await?).map_err(|_| "invalid_preferences".into())
}
#[tauri::command]
pub async fn service_save_sources(app: AppHandle, webview: Webview, sources: Vec<String>) -> Result<Value, String> {
    trusted(&webview)?;
    serde_json::to_value(state(&app)?.save_sources(sources).await?).map_err(|_| "invalid_preferences".into())
}
#[tauri::command]
pub async fn service_scan_sessions(app: AppHandle, webview: Webview, source: String) -> Result<Value, String> {
    trusted(&webview)?;
    state(&app)?.scan_sessions(source).await
}
#[tauri::command]
pub async fn service_browse_sessions(app: AppHandle, webview: Webview, source: String, query: String, offset: usize) -> Result<Value, String> {
    trusted(&webview)?;
    state(&app)?.browse_sessions(source, query, offset).await
}
#[tauri::command]
pub async fn service_preview_session(app: AppHandle, webview: Webview, source: String, key: String) -> Result<Value, String> {
    trusted(&webview)?;
    state(&app)?.preview_session(source, key).await
}
#[tauri::command]
pub async fn service_exclude_sessions(app: AppHandle, webview: Webview, source: String, keys: Vec<String>, excluded: bool) -> Result<Value, String> {
    trusted(&webview)?;
    state(&app)?.exclude_sessions(source, keys, excluded).await
}
