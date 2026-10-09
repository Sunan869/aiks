use crate::app_state::AppState;
use crate::app_state::AppState;
use aiks_core::knowledge::{
    render_agent_context, render_project_review, ProjectMemoryService, ProjectMemorySnapshot,
    ProjectOverview,
};
use tauri::State;

#[tauri::command]
pub async fn list_project_memory(
    state: State<'_, AppState>,
) -> Result<Vec<ProjectOverview>, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    ProjectMemoryService::new(&db)
        .list()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_project_memory(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<ProjectMemorySnapshot, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    ProjectMemoryService::new(&db)
        .get(&project_id, 200)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_project_review(
    project_id: String,
    from: String,
    through: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let snapshot = ProjectMemoryService::new(&db)
        .get(&project_id, 200)
        .map_err(|error| error.to_string())?;
    render_project_review(&snapshot, &from, &through).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_agent_context_pack(
    project_id: String,
    max_tokens: usize,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let snapshot = ProjectMemoryService::new(&db)
        .get(&project_id, 200)
        .map_err(|error| error.to_string())?;
    Ok(render_agent_context(&snapshot, max_tokens))
}
