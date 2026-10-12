use crate::app_state::AppState;
use aiks_core::knowledge::{
    render_agent_context_scoped, render_curated_agent_context_scoped, render_project_review_scoped,
    ProjectEvolutionService, ProjectEvolutionSnapshot, ProjectMemoryService, ProjectMemorySnapshot,
    ProjectOverview, ProjectReportDraft, ProjectReportDraftInput, ProjectReportDraftService,
    ProjectReportVersion, ProjectSessionBinding, ProjectSessionBindingService,
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
pub async fn list_project_session_bindings(
    state: State<'_, AppState>,
) -> Result<Vec<ProjectSessionBinding>, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    ProjectSessionBindingService::new(&db)
        .list()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn bind_project_session(
    session_id: i64,
    target_project_id: String,
    state: State<'_, AppState>,
    gate: State<'_, std::sync::Arc<aiks_core::storage::backup_quiescence::BackupQuiescence>>,
) -> Result<ProjectSessionBinding, String> {
    let _writer = gate
        .try_begin_write()
        .ok_or("正在执行备份，已暂停写入操作")?;
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    ProjectSessionBindingService::new(&db)
        .bind(session_id, &target_project_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn clear_project_session_binding(
    session_id: i64,
    state: State<'_, AppState>,
    gate: State<'_, std::sync::Arc<aiks_core::storage::backup_quiescence::BackupQuiescence>>,
) -> Result<bool, String> {
    let _writer = gate
        .try_begin_write()
        .ok_or("正在执行备份，已暂停写入操作")?;
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    ProjectSessionBindingService::new(&db)
        .clear(session_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_project_review(
    project_id: String,
    from: String,
    through: String,
    source: Option<String>,
    state: State<'_, AppState>,
    gate: State<'_, std::sync::Arc<aiks_core::storage::backup_quiescence::BackupQuiescence>>,
) -> Result<String, String> {
    let _writer = gate
        .try_begin_write()
        .ok_or("正在执行备份，已暂停写入操作")?;
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let snapshot = ProjectMemoryService::new(&db)
        .get_review_scoped(&project_id, 200, source.as_deref(), &from, &through)
        .map_err(|error| error.to_string())?;
    render_project_review_scoped(&snapshot, &from, &through, source.as_deref())
        .map_err(|error| error.to_string())
}

/// S3.2: read-only draft list; unlike export this never contacts any service.
#[tauri::command]
pub async fn list_project_report_drafts(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ProjectReportDraft>, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    ProjectReportDraftService::new(&db)
        .list(&project_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_project_report_draft(
    draft_id: String,
    revision: Option<i64>,
    state: State<'_, AppState>,
) -> Result<ProjectReportVersion, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    ProjectReportDraftService::new(&db)
        .get(&draft_id, revision)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Reviewed report draft was not found".to_string())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProjectReportDraftPayload {
    pub draft_id: Option<String>,
    pub project_id: String,
    pub from: String,
    pub through: String,
    pub source: Option<String>,
    pub expected_revision: i64,
    pub body: String,
}

/// User-reviewed local save; never publishes or writes Provider source files.
#[tauri::command]
pub async fn save_project_report_draft(
    payload: SaveProjectReportDraftPayload,
    state: State<'_, AppState>,
    gate: State<'_, std::sync::Arc<aiks_core::storage::backup_quiescence::BackupQuiescence>>,
) -> Result<ProjectReportVersion, String> {
    let _writer = gate
        .try_begin_write()
        .ok_or("正在执行备份，已暂停写入操作")?;
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    ProjectReportDraftService::new(&db)
        .save(ProjectReportDraftInput {
            draft_id: payload.draft_id.as_deref(),
            project_id: &payload.project_id,
            from: &payload.from,
            through: &payload.through,
            source: payload.source.as_deref(),
            expected_revision: payload.expected_revision,
            body: &payload.body,
        })
        .map_err(|error| error.to_string())
}

/// Opt-in selected Agent context preview. Missing or stale IDs fail closed
/// against the current canonical project read model; no Provider write.
#[tauri::command]
pub async fn create_curated_agent_context_pack(
    project_id: String,
    max_tokens: usize,
    source: Option<String>,
    knowledge_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    // An explicit user selection may only come from the bounded current
    // snapshot. The Core renderer requires every chosen ID to be present.
    let snapshot = ProjectMemoryService::new(&db)
        .get_scoped(&project_id, 200, source.as_deref())
        .map_err(|error| error.to_string())?;
    render_curated_agent_context_scoped(&snapshot, max_tokens, source.as_deref(), &knowledge_ids)
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
