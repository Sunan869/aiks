from pathlib import Path


def replace_once(path: str, old: str, new: str, label: str) -> None:
    target = Path(path)
    text = target.read_text()
    if old not in text:
        raise SystemExit(f"missing replacement target: {label}")
    target.write_text(text.replace(old, new, 1))

commands = "apps/aiks-desktop/src-tauri/src/workbench/commands.rs"
replace_once(
    commands,
    "fn dispatch_action(\n",
    "pub(super) fn dispatch_action(\n",
    "expose bridge dispatcher to event handler",
)

events = "apps/aiks-desktop/src-tauri/src/workbench/events.rs"
replace_once(
    events,
    '''use aiks_core::ai::ModelService;
use aiks_core::sink::SiYuanSink;
use aiks_core::{
    AiksEngine, CreateKnowledgeInput, KnowledgeIndexInput, KnowledgeIndexService, KnowledgeService,
};''',
    '''use aiks_core::ai::ModelService;
use aiks_core::knowledge::{AiAssistOperation, AiAssistRequest, AiAssistService, AiAssistSuggestion};
use aiks_core::sink::SiYuanSink;
use aiks_core::{
    AiksEngine, CreateKnowledgeInput, KnowledgeIndexInput, KnowledgeIndexService, KnowledgeService,
};''',
    "AI Assist imports",
)
replace_once(
    events,
    '''use super::protocol::{validate_identifier, WorkspaceMode, BRIDGE_PROTOCOL_VERSION};''',
    '''use super::protocol::{
    validate_identifier, WorkbenchAction, WorkspaceMode, BRIDGE_PROTOCOL_VERSION,
};''',
    "WorkbenchAction import",
)
replace_once(
    events,
    '''struct InboundBridgeEvent {
    source: String,
    version: u16,
    nonce: Option<String>,
    event: String,
    #[serde(default)]
    payload: Value,
}
''',
    '''struct InboundBridgeEvent {
    source: String,
    version: u16,
    nonce: Option<String>,
    event: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Debug, Clone)]
struct AiAssistBridgeRequest {
    request_id: String,
    doc_id: String,
    operation: AiAssistOperation,
}
''',
    "AI Assist bridge request type",
)
replace_once(
    events,
    '''            "documentCreated" => {
''',
    '''            "requestAiAssist" => {
                match parse_ai_assist_request(&envelope.payload) {
                    Ok(request) => spawn_ai_assist(&app_handle, request),
                    Err(error) => tracing::warn!(
                        error = %error,
                        "[WORKBENCH] rejected invalid AI Assist request"
                    ),
                }
            }
            "documentCreated" => {
''',
    "AI Assist event match arm",
)
replace_once(
    events,
    '''fn validated_doc_id(payload: &Value, action: &str) -> Option<String> {
''',
    '''fn parse_ai_assist_request(payload: &Value) -> anyhow::Result<AiAssistBridgeRequest> {
    let request_id = payload
        .get("requestId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("AI Assist request missing requestId"))?;
    let doc_id = payload
        .get("docId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("AI Assist request missing docId"))?;
    let operation = payload
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("AI Assist request missing operation"))?;

    Ok(AiAssistBridgeRequest {
        request_id: validate_identifier("request_id", request_id)?,
        doc_id: validate_identifier("doc_id", doc_id)?,
        operation: serde_json::from_value(Value::String(operation.to_string()))?,
    })
}

fn spawn_ai_assist(app: &AppHandle, request: AiAssistBridgeRequest) {
    let Some(state) = app.try_state::<AppState>() else {
        send_ai_assist_result(
            app,
            &request.request_id,
            Err(anyhow::anyhow!("AIKS application state is unavailable")),
        );
        return;
    };
    let Some(engine) = state.engine() else {
        send_ai_assist_result(
            app,
            &request.request_id,
            Err(anyhow::anyhow!("AIKS engine is unavailable")),
        );
        return;
    };
    let siyuan_url = state.siyuan_url.clone();
    let app_bg = app.clone();

    tauri::async_runtime::spawn(async move {
        let result = assist_siyuan_document(engine, siyuan_url, &request).await;
        send_ai_assist_result(&app_bg, &request.request_id, result);
    });
}

async fn assist_siyuan_document(
    engine: Arc<AiksEngine>,
    siyuan_url: Arc<Mutex<Option<String>>>,
    request: &AiAssistBridgeRequest,
) -> anyhow::Result<AiAssistSuggestion> {
    let sink = build_siyuan_sink(&engine, siyuan_url).await?;
    let escaped_doc_id = request.doc_id.replace('\'', "''");
    let rows = sink
        .query_sql(&format!(
            "SELECT id, content, hpath FROM blocks WHERE id = '{escaped_doc_id}' AND type = 'd' LIMIT 1"
        ))
        .await?;
    let row = rows
        .first()
        .ok_or_else(|| anyhow::anyhow!("SiYuan document not found: {}", request.doc_id))?;
    let hpath = row.get("hpath").and_then(Value::as_str).unwrap_or_default();
    let config = engine.config();
    if !is_under_knowledge_root(hpath, &config.siyuan.knowledge_root) {
        anyhow::bail!("AI Assist is only available for canonical Knowledge documents");
    }

    let markdown = sink.get_document_markdown(&request.doc_id).await?;
    if markdown.trim().is_empty() {
        anyhow::bail!("AI Assist requires canonical document content");
    }

    let db = engine.db();
    let existing = match find_knowledge_id(db.as_ref(), &request.doc_id)? {
        Some(id) => KnowledgeService::new(db.as_ref()).get(&id)?,
        None => None,
    };
    let title = document_title(row, hpath);
    let model_service = Arc::new(ModelService::new(
        config.ai.clone(),
        config.embedding.clone(),
    )?);
    AiAssistService::new(model_service)
        .suggest(AiAssistRequest {
            operation: request.operation.clone(),
            title,
            content: markdown,
            existing_summary: existing.as_ref().map(|item| item.summary.clone()),
            existing_tags: existing
                .as_ref()
                .map(|item| item.tags.clone())
                .unwrap_or_default(),
            existing_category: existing.as_ref().map(|item| item.category.clone()),
        })
        .await
}

fn send_ai_assist_result(
    app: &AppHandle,
    request_id: &str,
    result: anyhow::Result<AiAssistSuggestion>,
) {
    let action = match result {
        Ok(suggestion) => WorkbenchAction::AiAssistResult {
            request_id: request_id.to_string(),
            ok: true,
            suggestion: serde_json::to_value(suggestion).ok(),
            error: None,
        },
        Err(error) => WorkbenchAction::AiAssistResult {
            request_id: request_id.to_string(),
            ok: false,
            suggestion: None,
            error: Some(error.to_string()),
        },
    };
    let controller = app.state::<WorkbenchController>();
    if let Err(error) = super::commands::dispatch_action(app, controller.inner(), action) {
        tracing::warn!(
            error = %error,
            request_id = %request_id,
            "[WORKBENCH] failed to deliver AI Assist result"
        );
    }
}

fn validated_doc_id(payload: &Value, action: &str) -> Option<String> {
''',
    "AI Assist handler functions",
)
replace_once(
    events,
    '''    #[test]
    fn knowledge_root_filter_accepts_only_children() {
''',
    '''    #[test]
    fn parses_ai_assist_identity_and_operation_without_window_body() {
        let payload = serde_json::json!({
            "requestId": "request-1",
            "docId": "doc-1",
            "operation": "summary",
            "content": "untrusted window body must be ignored",
        });
        let request = parse_ai_assist_request(&payload).unwrap();
        assert_eq!(request.request_id, "request-1");
        assert_eq!(request.doc_id, "doc-1");
        assert_eq!(request.operation, AiAssistOperation::Summary);
    }

    #[test]
    fn accepts_ai_assist_as_a_valid_cross_system_event() {
        assert_eq!(
            validate_inbound(&event("requestAiAssist", Some("nonce-1")), "nonce-1").unwrap(),
            "requestAiAssist"
        );
    }

    #[test]
    fn knowledge_root_filter_accepts_only_children() {
''',
    "AI Assist event tests",
)
