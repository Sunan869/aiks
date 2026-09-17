use std::sync::Arc;

use aiks_core::{
    ai::ModelService,
    knowledge::{AiAssistOperation, AiAssistRequest, AiAssistService},
};
use tauri::State;

use crate::app_state::AppState;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAssistCommandRequest {
    siyuan_doc_id: String,
    operation: AiAssistOperation,
    title: String,
    content: String,
    existing_summary: Option<String>,
    existing_tags: Option<Vec<String>>,
    existing_category: Option<String>,
}

/// Return an AI suggestion for canonical SiYuan content without writing either
/// SiYuan or AIKS state. The caller explicitly decides whether/how to apply it.
#[tauri::command]
pub async fn assist_knowledge_v42(
    request: AiAssistCommandRequest,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let AiAssistCommandRequest {
        siyuan_doc_id,
        operation,
        title,
        content,
        existing_summary,
        existing_tags,
        existing_category,
    } = request;

    if siyuan_doc_id.trim().is_empty() {
        return Err("siyuan_doc_id must not be empty".to_string());
    }

    let engine = state.engine().ok_or("Engine not initialized")?;
    let models = Arc::new(
        ModelService::new(
            engine.ai_config().clone(),
            engine.embedding_config().clone(),
        )
        .map_err(|error| error.to_string())?,
    );
    let suggestion = AiAssistService::new(models)
        .suggest(AiAssistRequest {
            operation,
            title,
            content,
            existing_summary,
            existing_tags: existing_tags.unwrap_or_default(),
            existing_category,
        })
        .await
        .map_err(|error| error.to_string())?;

    serde_json::to_value(suggestion).map_err(|error| error.to_string())
}
