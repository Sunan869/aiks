use serde::Serialize;
use serde_json::{json, Value};
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, State, Url, WebviewUrl};
use uuid::Uuid;

use crate::app_state::AppState;

use super::controller::{WorkbenchController, WorkbenchStatus};
use super::protocol::{
    validate_identifier, validate_protocol_version, WorkbenchAction, WorkspaceMode,
    BRIDGE_PROTOCOL_VERSION,
};

const WORKBENCH_WEBVIEW_LABEL: &str = "siyuan-workbench";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeEnvelope {
    version: u16,
    request_id: String,
    nonce: String,
    action: String,
    payload: Value,
}

impl BridgeEnvelope {
    fn for_action(
        controller: &WorkbenchController,
        action: WorkbenchAction,
    ) -> anyhow::Result<Self> {
        validate_protocol_version(BRIDGE_PROTOCOL_VERSION)?;
        let (action, payload) = match action {
            WorkbenchAction::ShowKnowledgeRoot => ("showKnowledgeRoot", json!({})),
            WorkbenchAction::ShowSessionRoot => ("showSessionRoot", json!({})),
            WorkbenchAction::OpenDocument { doc_id } => {
                let doc_id = validate_identifier("doc_id", &doc_id)?;
                ("openDocument", json!({ "docId": doc_id }))
            }
            WorkbenchAction::OpenBlock { doc_id, block_id } => {
                let doc_id = validate_identifier("doc_id", &doc_id)?;
                let block_id = validate_identifier("block_id", &block_id)?;
                ("openBlock", json!({ "docId": doc_id, "blockId": block_id }))
            }
            WorkbenchAction::SetWorkspaceMode { mode } => {
                ("setWorkspaceMode", json!({ "mode": mode }))
            }
            WorkbenchAction::AiAssistResult {
                request_id,
                ok,
                suggestion,
                error,
            } => {
                let request_id = validate_identifier("request_id", &request_id)?;
                (
                    "aiAssistResult",
                    json!({
                        "requestId": request_id,
                        "ok": ok,
                        "suggestion": suggestion,
                        "error": error,
                    }),
                )
            }
            WorkbenchAction::RefreshDocument { doc_id } => {
                let doc_id = validate_identifier("doc_id", &doc_id)?;
                ("refreshDocument", json!({ "docId": doc_id }))
            }
        };

        Ok(Self {
            version: BRIDGE_PROTOCOL_VERSION,
            request_id: Uuid::new_v4().to_string(),
            nonce: controller.nonce().to_string(),
            action: action.to_string(),
            payload,
        })
    }
}

async fn sync_origin(
    app_state: &AppState,
    controller: &WorkbenchController,
) -> Result<Option<Url>, String> {
    let origin = app_state.siyuan_url.lock().await.clone();
    match origin {
        Some(origin) => {
            controller.set_origin(&origin).map_err(|e| e.to_string())?;
            Ok(controller.origin())
        }
        None => {
            controller.clear_origin();
            Ok(None)
        }
    }
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn validate_bounds(x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
    if !x.is_finite() || !y.is_finite() || !width.is_finite() || !height.is_finite() {
        return Err("workbench bounds must be finite".to_string());
    }
    if x < 0.0 || y < 0.0 || width < 1.0 || height < 1.0 {
        return Err("workbench bounds are outside the control window".to_string());
    }
    Ok(())
}

pub(super) fn dispatch_action(
    app: &AppHandle,
    controller: &WorkbenchController,
    action: WorkbenchAction,
) -> Result<(), String> {
    if !controller.status().ready {
        return Err("SiYuan bridge is still initializing".to_string());
    }

    let expected_origin = controller
        .origin()
        .ok_or_else(|| "SiYuan workbench is unavailable".to_string())?;
    let envelope = BridgeEnvelope::for_action(controller, action).map_err(|e| e.to_string())?;
    let message = serde_json::to_string(&envelope).map_err(|e| e.to_string())?;
    let script = format!("window.postMessage({message}, window.location.origin);");

    if let Some(webview) = app.get_webview(WORKBENCH_WEBVIEW_LABEL) {
        let current_url = webview.url().map_err(|e| e.to_string())?;
        if !same_origin(&current_url, &expected_origin) {
            return Err("SiYuan child workbench is not loaded yet".to_string());
        }
        return webview.eval(&script).map_err(|e| e.to_string());
    }

    let window = app
        .get_webview_window("knowledge")
        .ok_or_else(|| "knowledge workbench is unavailable".to_string())?;
    let current_url = window.url().map_err(|e| e.to_string())?;
    if !same_origin(&current_url, &expected_origin) {
        return Err("SiYuan fallback workbench is not loaded yet".to_string());
    }
    window.eval(&script).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_workbench_status(
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
) -> Result<WorkbenchStatus, String> {
    sync_origin(app_state.inner(), controller.inner()).await?;
    Ok(controller.status())
}

#[tauri::command]
pub async fn mount_workbench(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    validate_bounds(x, y, width, height)?;
    let origin = sync_origin(app_state.inner(), controller.inner())
        .await?
        .ok_or_else(|| "SiYuan workbench is unavailable".to_string())?;

    if let Some(webview) = app.get_webview(WORKBENCH_WEBVIEW_LABEL) {
        let current_url = webview.url().map_err(|e| e.to_string())?;
        if !same_origin(&current_url, &origin) {
            controller.set_ready(false);
            webview.navigate(origin).map_err(|e| e.to_string())?;
        }
        webview
            .set_position(LogicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
        webview
            .set_size(LogicalSize::new(width, height))
            .map_err(|e| e.to_string())?;
        webview.show().map_err(|e| e.to_string())?;
        controller.set_mounted(true);
        return Ok(());
    }

    let parent = app
        .get_window("control")
        .ok_or_else(|| "control window is unavailable".to_string())?;
    let expected_origin = origin.clone();
    let nonce = serde_json::to_string(controller.nonce()).map_err(|e| e.to_string())?;
    let initialization_script = format!("window.__AIKS_WORKBENCH_NONCE__ = {nonce};");
    let builder = WebviewBuilder::new(WORKBENCH_WEBVIEW_LABEL, WebviewUrl::External(origin))
        .initialization_script(initialization_script)
        .on_navigation(move |url| same_origin(url, &expected_origin));

    controller.set_ready(false);
    let webview = parent
        .add_child(
            builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(width, height),
        )
        .map_err(|e| e.to_string())?;
    webview.show().map_err(|e| e.to_string())?;
    controller.set_mounted(true);
    Ok(())
}

#[tauri::command]
pub async fn show_workbench(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
) -> Result<(), String> {
    let origin = sync_origin(app_state.inner(), controller.inner())
        .await?
        .ok_or_else(|| "SiYuan workbench is unavailable".to_string())?;

    if let Some(webview) = app.get_webview(WORKBENCH_WEBVIEW_LABEL) {
        let current_url = webview.url().map_err(|e| e.to_string())?;
        if !same_origin(&current_url, &origin) {
            controller.set_ready(false);
            webview.navigate(origin).map_err(|e| e.to_string())?;
        }
        webview.show().map_err(|e| e.to_string())?;
        controller.set_mounted(true);
        return webview.set_focus().map_err(|e| e.to_string());
    }

    let window = app
        .get_webview_window("knowledge")
        .ok_or_else(|| "knowledge fallback workbench is unavailable".to_string())?;
    let current_url = window.url().map_err(|e| e.to_string())?;
    if !same_origin(&current_url, &origin) {
        controller.set_ready(false);
        window.navigate(origin).map_err(|e| e.to_string())?;
    }
    window.show().map_err(|e| e.to_string())?;
    controller.set_mounted(true);
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn hide_workbench(app: AppHandle) -> Result<(), String> {
    if let Some(webview) = app.get_webview(WORKBENCH_WEBVIEW_LABEL) {
        webview.hide().map_err(|e| e.to_string())?;
    }
    if let Some(window) = app.get_webview_window("knowledge") {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_workbench_mode(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
    mode: String,
) -> Result<(), String> {
    sync_origin(app_state.inner(), controller.inner()).await?;
    let mode = WorkspaceMode::parse(&mode).map_err(|e| e.to_string())?;
    controller.set_mode(mode);
    dispatch_action(
        &app,
        controller.inner(),
        WorkbenchAction::SetWorkspaceMode { mode },
    )
}

#[tauri::command]
pub async fn show_workbench_root(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
    mode: String,
) -> Result<(), String> {
    sync_origin(app_state.inner(), controller.inner()).await?;
    let mode = WorkspaceMode::parse(&mode).map_err(|e| e.to_string())?;
    controller.set_mode(mode);
    let action = match mode {
        WorkspaceMode::Knowledge => WorkbenchAction::ShowKnowledgeRoot,
        WorkspaceMode::Session => WorkbenchAction::ShowSessionRoot,
    };
    dispatch_action(&app, controller.inner(), action)
}

#[tauri::command]
pub async fn open_siyuan_document(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
    doc_id: String,
) -> Result<(), String> {
    sync_origin(app_state.inner(), controller.inner()).await?;
    dispatch_action(
        &app,
        controller.inner(),
        WorkbenchAction::OpenDocument { doc_id },
    )
}

#[tauri::command]
pub async fn open_siyuan_block(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
    doc_id: String,
    block_id: String,
) -> Result<(), String> {
    sync_origin(app_state.inner(), controller.inner()).await?;
    dispatch_action(
        &app,
        controller.inner(),
        WorkbenchAction::OpenBlock { doc_id, block_id },
    )
}

#[tauri::command]
pub async fn refresh_siyuan_document(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
    doc_id: String,
) -> Result<(), String> {
    sync_origin(app_state.inner(), controller.inner()).await?;
    dispatch_action(
        &app,
        controller.inner(),
        WorkbenchAction::RefreshDocument { doc_id },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::protocol::{WorkbenchAction, WorkspaceMode};

    #[test]
    fn bridge_envelope_contains_protocol_request_nonce_action_and_payload() {
        let controller = WorkbenchController::new();
        let envelope = BridgeEnvelope::for_action(
            &controller,
            WorkbenchAction::OpenDocument {
                doc_id: "20260916000100-abcdefg".to_string(),
            },
        )
        .unwrap();
        let json = serde_json::to_value(envelope).unwrap();

        assert_eq!(json["version"], 1);
        assert!(json["requestId"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
        assert_eq!(json["nonce"], controller.nonce());
        assert_eq!(json["action"], "openDocument");
        assert_eq!(json["payload"]["docId"], "20260916000100-abcdefg");
    }

    #[test]
    fn bridge_envelope_rejects_invalid_document_and_block_ids() {
        let controller = WorkbenchController::new();
        assert!(BridgeEnvelope::for_action(
            &controller,
            WorkbenchAction::OpenDocument {
                doc_id: " ".to_string(),
            },
        )
        .is_err());
        assert!(BridgeEnvelope::for_action(
            &controller,
            WorkbenchAction::OpenBlock {
                doc_id: "doc".to_string(),
                block_id: "".to_string(),
            },
        )
        .is_err());
    }

    #[test]
    fn workspace_mode_is_serialized_inside_payload() {
        let controller = WorkbenchController::new();
        let envelope = BridgeEnvelope::for_action(
            &controller,
            WorkbenchAction::SetWorkspaceMode {
                mode: WorkspaceMode::Session,
            },
        )
        .unwrap();
        let json = serde_json::to_value(envelope).unwrap();

        assert_eq!(json["action"], "setWorkspaceMode");
        assert_eq!(json["payload"]["mode"], "session");
    }

    #[test]
    fn rejects_invalid_child_workbench_bounds() {
        assert!(validate_bounds(10.0, 10.0, 500.0, 300.0).is_ok());
        assert!(validate_bounds(-1.0, 10.0, 500.0, 300.0).is_err());
        assert!(validate_bounds(10.0, 10.0, 0.0, 300.0).is_err());
        assert!(validate_bounds(10.0, 10.0, f64::NAN, 300.0).is_err());
    }
}
