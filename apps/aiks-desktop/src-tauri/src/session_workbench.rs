use aiks_core::storage::StateDb;
use tauri::State;

use crate::app_state::AppState;

pub fn lookup_session_doc_id(db: &StateDb, session_id: i64) -> anyhow::Result<Option<String>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT siyuan_doc_id FROM source_session WHERE id = ?1",
    )?;
    let mut rows = stmt.query([session_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    Ok(row.get(0)?)
}

#[tauri::command]
pub fn get_session_workbench_doc_id(
    state: State<'_, AppState>,
    session_id: i64,
) -> Result<Option<String>, String> {
    let engine = state
        .engine()
        .ok_or_else(|| "AIKS engine is unavailable".to_string())?;
    lookup_session_doc_id(engine.db().as_ref(), session_id).map_err(|error| error.to_string())
}
