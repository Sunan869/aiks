use std::sync::Arc;

use aiks_core::{ai::ModelService, RagAnswerService, RagAskRequest};
use tauri::State;

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
