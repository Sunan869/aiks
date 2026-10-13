use std::sync::Arc;

use aiks_core::{ai::ModelService, RagAnswerService, RagAskRequest};
use tauri::{ipc::JavaScriptChannelId, State, Webview};

use crate::app_state::AppState;

#[tauri::command]
pub async fn ask_aiks_rag(
    request: RagAskRequest,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;

    if !engine.ai_config().enabled {
        return Err("AI model is disabled".to_string());
    }

    let models = Arc::new(
        ModelService::new(
            engine.ai_config().clone(),
            engine.embedding_config().clone(),
        )
        .map_err(|error| error.to_string())?,
    );

    let answer = RagAnswerService::new(engine.db(), models)
        .ask(request)
        .await
        .map_err(|error| error.to_string())?;

    serde_json::to_value(answer).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn ask_aiks_rag_stream(
    request: RagAskRequest,
    on_delta: JavaScriptChannelId,
    webview: Webview,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;

    if !engine.ai_config().enabled {
        return Err("AI model is disabled".to_string());
    }

    let models = Arc::new(
        ModelService::new(
            engine.ai_config().clone(),
            engine.embedding_config().clone(),
        )
        .map_err(|error| error.to_string())?,
    );

    let channel = on_delta.channel_on(webview);
    let answer = RagAnswerService::new(engine.db(), models)
        .ask_stream(request, move |delta| {
            channel
                .send(serde_json::json!({ "type": "delta", "text": delta }))
                .map_err(|error| anyhow::anyhow!(error.to_string()))
        })
        .await
        .map_err(|error| error.to_string())?;

    serde_json::to_value(answer).map_err(|error| error.to_string())
}
