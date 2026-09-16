use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Listener, Manager};

use aiks_core::{
    knowledge::refresh_siyuan_document_read_model, sink::SiYuanSink, KnowledgeService,
};

use crate::app_state::AppState;

use super::controller::WorkbenchController;
use super::protocol::{validate_identifier, WorkspaceMode, BRIDGE_PROTOCOL_VERSION};

pub const WORKBENCH_EVENT_CHANNEL: &str = "aiks-workbench-event";

const ALLOWED_EVENTS: &[&str] = &[
    "bridgeReady",
    "documentOpened",
    "documentChanged",
    "documentCreated",
    "documentDeleted",
    "blockFocused",
    "knowledgeModified",
    "requestReExtract",
    "requestOpenSession",
    "requestOpenKnowledge",
    "requestShowPipeline",
    "workspaceModeChanged",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InboundBridgeEvent {
    source: String,
    version: u16,
    nonce: Option<String>,
    event: String,
    #[serde(default)]
    payload: Value,
}

fn validate_inbound<'a>(
    event: &'a InboundBridgeEvent,
    expected_nonce: &str,
) -> anyhow::Result<&'a str> {
    if event.source != "aiks-bridge" {
        anyhow::bail!("unexpected bridge source");
    }
    if event.version != BRIDGE_PROTOCOL_VERSION {
        anyhow::bail!("unsupported inbound bridge protocol version");
    }
    if event.nonce.as_deref() != Some(expected_nonce) {
        anyhow::bail!("invalid bridge nonce");
    }
    if !ALLOWED_EVENTS.contains(&event.event.as_str()) {
        anyhow::bail!("unsupported inbound bridge event");
    }
    Ok(event.event.as_str())
}

pub fn register(app: &AppHandle) {
    let app_handle = app.clone();
    app.listen(WORKBENCH_EVENT_CHANNEL, move |event| {
        let envelope: InboundBridgeEvent = match serde_json::from_str(event.payload()) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(error = %error, "[WORKBENCH] rejected malformed inbound event");
                return;
            }
        };

        let controller = app_handle.state::<WorkbenchController>();
        let event_name = match validate_inbound(&envelope, controller.nonce()) {
            Ok(name) => name,
            Err(error) => {
                tracing::warn!(error = %error, event = %envelope.event, "[WORKBENCH] rejected inbound event");
                return;
            }
        };

        match event_name {
            "bridgeReady" => {
                controller.set_ready(true);
                if let Some(mode) = envelope.payload.get("mode").and_then(Value::as_str) {
                    if let Ok(mode) = WorkspaceMode::parse(mode) {
                        controller.set_mode(mode);
                    }
                }
                let _ = app_handle.emit("workbench-ready", controller.status());
            }
            "workspaceModeChanged" => {
                if let Some(mode) = envelope.payload.get("mode").and_then(Value::as_str) {
                    if let Ok(mode) = WorkspaceMode::parse(mode) {
                        controller.set_mode(mode);
                    }
                }
            }
            "documentChanged" | "knowledgeModified" => {
                let Some(doc_id) = envelope.payload.get("docId").and_then(Value::as_str) else {
                    tracing::warn!("[WORKBENCH] document change missing docId");
                    return;
                };
                let doc_id = match validate_identifier("doc_id", doc_id) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(error = %error, "[WORKBENCH] invalid changed document id");
                        return;
                    }
                };

                if let Some(state) = app_handle.try_state::<AppState>() {
                    if let Some(engine) = state.engine() {
                        let db = engine.db();
                        match KnowledgeService::new(db.as_ref()).invalidate_siyuan_document(&doc_id) {
                            Ok(Some(knowledge_id)) => {
                                let _ = app_handle.emit(
                                    "knowledge-index-invalidated",
                                    serde_json::json!({
                                        "knowledgeId": knowledge_id,
                                        "docId": doc_id,
                                    }),
                                );

                                let app_bg = app_handle.clone();
                                let engine_bg = engine.clone();
                                let siyuan_url = state.siyuan_url.clone();
                                let doc_id_bg = doc_id.clone();
                                tauri::async_runtime::spawn(async move {
                                    let runtime_url = siyuan_url.lock().await.clone();
                                    let mut siyuan_config = engine_bg.config().siyuan.clone();
                                    if let Some(runtime_url) = runtime_url {
                                        siyuan_config.base_url = runtime_url;
                                    }

                                    let sink = match SiYuanSink::new(siyuan_config) {
                                        Ok(sink) => sink,
                                        Err(error) => {
                                            tracing::warn!(
                                                error = %error,
                                                doc_id = %doc_id_bg,
                                                "[WORKBENCH] could not initialize SiYuan read-model refresh"
                                            );
                                            return;
                                        }
                                    };
                                    let markdown = match sink.get_document_markdown(&doc_id_bg).await {
                                        Ok(markdown) => markdown,
                                        Err(error) => {
                                            tracing::warn!(
                                                error = %error,
                                                doc_id = %doc_id_bg,
                                                "[WORKBENCH] could not read changed SiYuan document"
                                            );
                                            return;
                                        }
                                    };

                                    let db = engine_bg.db();
                                    match refresh_siyuan_document_read_model(
                                        db.as_ref(),
                                        &doc_id_bg,
                                        &markdown,
                                    ) {
                                        Ok(Some(refreshed_id)) => {
                                            let _ = app_bg.emit(
                                                "knowledge-index-refreshed",
                                                serde_json::json!({
                                                    "knowledgeId": refreshed_id,
                                                    "docId": doc_id_bg,
                                                }),
                                            );
                                        }
                                        Ok(None) => {}
                                        Err(error) => tracing::warn!(
                                            error = %error,
                                            doc_id = %doc_id_bg,
                                            "[WORKBENCH] failed to rebuild knowledge read model"
                                        ),
                                    }
                                });
                            }
                            Ok(None) => {}
                            Err(error) => tracing::warn!(
                                error = %error,
                                doc_id = %doc_id,
                                "[WORKBENCH] failed to invalidate knowledge indexes"
                            ),
                        }
                    }
                }
            }
            _ => {
                let _ = app_handle.emit("workbench-event", &envelope.payload);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(name: &str, nonce: Option<&str>) -> InboundBridgeEvent {
        InboundBridgeEvent {
            source: "aiks-bridge".into(),
            version: 1,
            nonce: nonce.map(str::to_string),
            event: name.into(),
            payload: serde_json::json!({}),
        }
    }

    #[test]
    fn accepts_only_expected_version_nonce_source_and_event() {
        assert_eq!(
            validate_inbound(&event("bridgeReady", Some("nonce-1")), "nonce-1").unwrap(),
            "bridgeReady"
        );

        let mut wrong_version = event("bridgeReady", Some("nonce-1"));
        wrong_version.version = 2;
        assert!(validate_inbound(&wrong_version, "nonce-1").is_err());

        let mut wrong_source = event("bridgeReady", Some("nonce-1"));
        wrong_source.source = "other".into();
        assert!(validate_inbound(&wrong_source, "nonce-1").is_err());

        assert!(validate_inbound(&event("bridgeReady", None), "nonce-1").is_err());
        assert!(validate_inbound(&event("bridgeReady", Some("wrong")), "nonce-1").is_err());
        assert!(validate_inbound(&event("runShellCommand", Some("nonce-1")), "nonce-1").is_err());
    }
}
