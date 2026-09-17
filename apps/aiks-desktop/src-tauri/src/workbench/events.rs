use std::sync::Arc;

use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Listener, Manager};
use tokio::sync::Mutex;

use aiks_core::ai::ModelService;
use aiks_core::sink::SiYuanSink;
use aiks_core::{
    AiksEngine, CreateKnowledgeInput, KnowledgeIndexInput, KnowledgeIndexService,
    KnowledgeService,
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
            "documentCreated" => {
                let Some(doc_id) = validated_doc_id(&envelope.payload, "created") else {
                    return;
                };
                spawn_document_index(&app_handle, doc_id);
            }
            "documentChanged" | "knowledgeModified" => {
                let Some(doc_id) = validated_doc_id(&envelope.payload, "changed") else {
                    return;
                };

                if let Some(state) = app_handle.try_state::<AppState>() {
                    if let Some(engine) = state.engine() {
                        let db = engine.db();
                        match KnowledgeService::new(db.as_ref())
                            .invalidate_siyuan_document(&doc_id)
                        {
                            Ok(Some(knowledge_id)) => emit_index_status(
                                &app_handle,
                                &doc_id,
                                Some(&knowledge_id),
                                "stale",
                                None,
                            ),
                            Ok(None) => {}
                            Err(error) => tracing::warn!(
                                error = %error,
                                doc_id = %doc_id,
                                "[WORKBENCH] failed to invalidate knowledge indexes"
                            ),
                        }
                    }
                }

                spawn_document_index(&app_handle, doc_id);
            }
            "documentDeleted" => {
                let Some(doc_id) = validated_doc_id(&envelope.payload, "deleted") else {
                    return;
                };
                handle_document_deleted(&app_handle, &doc_id);
            }
            _ => {
                let _ = app_handle.emit("workbench-event", &envelope.payload);
            }
        }
    });
}

fn validated_doc_id(payload: &Value, action: &str) -> Option<String> {
    let Some(doc_id) = payload.get("docId").and_then(Value::as_str) else {
        tracing::warn!(action = %action, "[WORKBENCH] document lifecycle event missing docId");
        return None;
    };
    match validate_identifier("doc_id", doc_id) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::warn!(
                error = %error,
                action = %action,
                "[WORKBENCH] invalid lifecycle document id"
            );
            None
        }
    }
}

fn spawn_document_index(app: &AppHandle, doc_id: String) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(engine) = state.engine() else {
        return;
    };
    let siyuan_url = state.siyuan_url.clone();
    let app_bg = app.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(error) = index_siyuan_document(
            app_bg.clone(),
            engine,
            siyuan_url,
            doc_id.clone(),
        )
        .await
        {
            tracing::warn!(
                error = %error,
                doc_id = %doc_id,
                "[WORKBENCH] canonical knowledge indexing failed"
            );
            let error_message = error.to_string();
            emit_index_status(
                &app_bg,
                &doc_id,
                None,
                "failed",
                Some(error_message.as_str()),
            );
        }
    });
}

async fn index_siyuan_document(
    app: AppHandle,
    engine: Arc<AiksEngine>,
    siyuan_url: Arc<Mutex<Option<String>>>,
    doc_id: String,
) -> anyhow::Result<Option<String>> {
    let sink = build_siyuan_sink(&engine, siyuan_url).await?;
    let escaped_doc_id = doc_id.replace('\'', "''");
    let rows = sink
        .query_sql(&format!(
            "SELECT id, content, hpath FROM blocks WHERE id = '{escaped_doc_id}' AND type = 'd' LIMIT 1"
        ))
        .await?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };

    let hpath = row.get("hpath").and_then(Value::as_str).unwrap_or_default();
    let config = engine.config();
    if !is_under_knowledge_root(hpath, &config.siyuan.knowledge_root) {
        return Ok(None);
    }

    let markdown = sink.get_document_markdown(&doc_id).await?;
    let db = engine.db();
    let existing_id = find_knowledge_id(db.as_ref(), &doc_id)?;

    let knowledge_id = match existing_id {
        Some(id) => id,
        None => {
            if markdown.trim().is_empty() {
                tracing::debug!(
                    doc_id = %doc_id,
                    "[WORKBENCH] deferring empty native SiYuan document until it has content"
                );
                return Ok(None);
            }
            let title = document_title(row, hpath);
            KnowledgeService::new(db.as_ref())
                .create_native_siyuan(
                    CreateKnowledgeInput {
                        title,
                        category: Some("general".to_string()),
                        project_name: None,
                        summary: Some(String::new()),
                        content: markdown.clone(),
                        tags: Vec::new(),
                    },
                    &doc_id,
                    None,
                )?
                .id
        }
    };

    emit_index_status(
        &app,
        &doc_id,
        Some(&knowledge_id),
        "indexing",
        None,
    );

    let model_service = Arc::new(ModelService::new(config.ai.clone(), config.embedding.clone())?);
    let index_service = KnowledgeIndexService::new(db, model_service);
    let result = index_service
        .index_document(KnowledgeIndexInput {
            knowledge_id: knowledge_id.clone(),
            siyuan_doc_id: doc_id.clone(),
            markdown,
        })
        .await?;

    let _ = app.emit(
        "knowledge-index-status",
        serde_json::json!({
            "knowledgeId": &knowledge_id,
            "docId": &doc_id,
            "status": "ready",
            "skipped": result.skipped,
            "chunkCount": result.chunk_count,
            "embeddedCount": result.embedded_count,
        }),
    );

    Ok(Some(knowledge_id))
}

async fn build_siyuan_sink(
    engine: &AiksEngine,
    siyuan_url: Arc<Mutex<Option<String>>>,
) -> anyhow::Result<SiYuanSink> {
    let runtime_url = siyuan_url.lock().await.clone();
    let mut siyuan_config = engine.config().siyuan.clone();
    if let Some(runtime_url) = runtime_url {
        siyuan_config.base_url = runtime_url;
    }
    SiYuanSink::new(siyuan_config)
}

fn find_knowledge_id(
    db: &aiks_core::storage::StateDb,
    siyuan_doc_id: &str,
) -> anyhow::Result<Option<String>> {
    let conn = db.conn();
    Ok(conn
        .query_row(
            "SELECT id FROM knowledge_item WHERE siyuan_doc_id = ?1 LIMIT 1",
            params![siyuan_doc_id],
            |row| row.get(0),
        )
        .optional()?)
}

fn is_under_knowledge_root(hpath: &str, knowledge_root: &str) -> bool {
    let root = knowledge_root.trim().trim_end_matches('/');
    if root.is_empty() {
        return false;
    }
    hpath.starts_with(&format!("{root}/"))
}

fn document_title(row: &Value, hpath: &str) -> String {
    let title = row
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if !title.is_empty() {
        return title.to_string();
    }

    hpath
        .trim_end_matches('/')
        .rsplit('/')
        .find(|segment| !segment.trim().is_empty())
        .unwrap_or("Untitled")
        .trim()
        .to_string()
}

fn handle_document_deleted(app: &AppHandle, doc_id: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(engine) = state.engine() else {
        return;
    };

    let config = engine.config();
    let model_service = match ModelService::new(config.ai.clone(), config.embedding.clone()) {
        Ok(service) => Arc::new(service),
        Err(error) => {
            tracing::warn!(
                error = %error,
                doc_id = %doc_id,
                "[WORKBENCH] failed to initialize index service for delete"
            );
            let error_message = error.to_string();
            emit_index_status(
                app,
                doc_id,
                None,
                "failed",
                Some(error_message.as_str()),
            );
            return;
        }
    };
    let index_service = KnowledgeIndexService::new(engine.db(), model_service);
    match index_service.mark_deleted(doc_id) {
        Ok(Some(knowledge_id)) => emit_index_status(
            app,
            doc_id,
            Some(&knowledge_id),
            "deleted",
            None,
        ),
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                error = %error,
                doc_id = %doc_id,
                "[WORKBENCH] failed to remove deleted knowledge index"
            );
            let error_message = error.to_string();
            emit_index_status(
                app,
                doc_id,
                None,
                "failed",
                Some(error_message.as_str()),
            );
        }
    }
}

fn emit_index_status(
    app: &AppHandle,
    doc_id: &str,
    knowledge_id: Option<&str>,
    status: &str,
    error: Option<&str>,
) {
    let _ = app.emit(
        "knowledge-index-status",
        serde_json::json!({
            "knowledgeId": knowledge_id,
            "docId": doc_id,
            "status": status,
            "error": error,
        }),
    );
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

    #[test]
    fn knowledge_root_filter_accepts_only_children() {
        assert!(is_under_knowledge_root(
            "/20 Knowledge/Project/Note",
            "/20 Knowledge"
        ));
        assert!(!is_under_knowledge_root(
            "/10 AI Sessions/Codex/Session",
            "/20 Knowledge"
        ));
        assert!(!is_under_knowledge_root("/20 Knowledge", "/20 Knowledge"));
    }
}
