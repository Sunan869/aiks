use crate::app_state::AppState;
use aiks_core::knowledge::{
    render_agent_context_scoped, render_project_review_scoped, ProjectEvolutionService,
    ProjectEvolutionSnapshot, ProjectMemoryService, ProjectMemorySnapshot, ProjectOverview,
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

/// Read-only evidence links; no inferred contradictions or automatic edits.
#[tauri::command]
pub async fn list_project_evolution(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<ProjectEvolutionSnapshot, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    ProjectEvolutionService::new(&db)
        .list(&project_id, 100)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_project_review(
    project_id: String,
    from: String,
    through: String,
    source: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let snapshot = ProjectMemoryService::new(&db)
        .get_review_scoped(&project_id, 200, source.as_deref(), &from, &through)
        .map_err(|error| error.to_string())?;
    render_project_review_scoped(&snapshot, &from, &through, source.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_agent_context_pack(
    project_id: String,
    max_tokens: usize,
    source: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let snapshot = ProjectMemoryService::new(&db)
        .get_scoped(&project_id, 200, source.as_deref())
        .map_err(|error| error.to_string())?;
    render_agent_context_scoped(&snapshot, max_tokens, source.as_deref())
        .map_err(|error| error.to_string())
}
