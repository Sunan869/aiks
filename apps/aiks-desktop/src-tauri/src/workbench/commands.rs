use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, State, Url};
use uuid::Uuid;

use crate::app_state::AppState;

use super::controller::{WorkbenchController, WorkbenchStatus};
use super::protocol::{
    validate_identifier, validate_protocol_version, WorkbenchAction, WorkspaceMode,
    BRIDGE_PROTOCOL_VERSION,
};

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
                (
                    "openBlock",
                    json!({ "docId": doc_id, "blockId": block_id }),
                )
            }
            WorkbenchAction::SetWorkspaceMode { mode } => {
                ("setWorkspaceMode", json!({ "mode": mode }))
            }
            WorkbenchAction::ShowBacklinks { block_id } => {
                let block_id = validate_identifier("block_id", &block_id)?;
                ("showBacklinks", json!({ "blockId": block_id }))
            }
            WorkbenchAction::ShowOutline => ("showOutline", json!({})),
            WorkbenchAction::ShowDatabase => ("showDatabase", json!({})),
            WorkbenchAction::ShowGraph => ("showGraph", json!({})),
            WorkbenchAction::ShowSearch => ("showSearch", json!({})),
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

fn dispatch_action(
    app: &AppHandle,
    controller: &WorkbenchController,
    action: WorkbenchAction,
) -> Result<(), String> {
    let expected_origin = controller
        .origin()
        .ok_or_else(|| "SiYuan workbench is unavailable".to_string())?;
    let window = app
        .get_webview_window("knowledge")
        .ok_or_else(|| "knowledge webview is unavailable".to_string())?;
    let current_url = window.url().map_err(|e| e.to_string())?;
    if !same_origin(&current_url, &expected_origin) {
        return Err("SiYuan workbench is not loaded yet".to_string());
    }

    let envelope = BridgeEnvelope::for_action(controller, action).map_err(|e| e.to_string())?;
    let message = serde_json::to_string(&envelope).map_err(|e| e.to_string())?;
    let script = format!("window.postMessage({message}, window.location.origin);");
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
pub async fn show_workbench(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
) -> Result<(), String> {
    let origin = sync_origin(app_state.inner(), controller.inner())
        .await?
        .ok_or_else(|| "SiYuan workbench is unavailable".to_string())?;
    let window = app
        .get_webview_window("knowledge")
        .ok_or_else(|| "knowledge webview is unavailable".to_string())?;
    let current_url = window.url().map_err(|e| e.to_string())?;
    if !same_origin(&current_url, &origin) {
        window.navigate(origin).map_err(|e| e.to_string())?;
    }
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn hide_workbench(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("knowledge")
        .ok_or_else(|| "knowledge webview is unavailable".to_string())?;
    window.hide().map_err(|e| e.to_string())
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
pub async fn show_workbench_backlinks(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
    block_id: String,
) -> Result<(), String> {
    sync_origin(app_state.inner(), controller.inner()).await?;
    dispatch_action(
        &app,
        controller.inner(),
        WorkbenchAction::ShowBacklinks { block_id },
    )
}

#[tauri::command]
pub async fn show_workbench_outline(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
) -> Result<(), String> {
    sync_origin(app_state.inner(), controller.inner()).await?;
    dispatch_action(&app, controller.inner(), WorkbenchAction::ShowOutline)
}

#[tauri::command]
pub async fn show_workbench_database(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
) -> Result<(), String> {
    sync_origin(app_state.inner(), controller.inner()).await?;
    dispatch_action(&app, controller.inner(), WorkbenchAction::ShowDatabase)
}

#[tauri::command]
pub async fn show_workbench_graph(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
) -> Result<(), String> {
    sync_origin(app_state.inner(), controller.inner()).await?;
    dispatch_action(&app, controller.inner(), WorkbenchAction::ShowGraph)
}

#[tauri::command]
pub async fn show_workbench_search(
    app: AppHandle,
    app_state: State<'_, AppState>,
    controller: State<'_, WorkbenchController>,
) -> Result<(), String> {
    sync_origin(app_state.inner(), controller.inner()).await?;
    dispatch_action(&app, controller.inner(), WorkbenchAction::ShowSearch)
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
}
