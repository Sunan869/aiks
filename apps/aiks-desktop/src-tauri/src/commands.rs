/// Tauri commands — the bridge between React frontend and AIKS Core.
use std::sync::Arc;

use aiks_core::runtime::SiyuanRuntime;
use serde::{Deserialize, Serialize};
use tauri::{State};
use tokio::sync::Mutex;

use crate::app_state::AppState;

// ── Response types ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct StatusResponse {
    pub total_sessions: usize,
    pub synced: usize,
    pub pending: usize,
    pub conflict: usize,
    pub failed: usize,
    pub last_sync_at: Option<String>,
    pub runtime: Option<RuntimeStatusDto>,
}

#[derive(Serialize, Clone)]
pub struct RuntimeStatusDto {
    pub state: String,
    pub port: Option<u16>,
    pub version: Option<String>,
    pub mode: String,
}

#[derive(Serialize)]
pub struct ScanResponse {
    pub total: usize,
    pub by_source: std::collections::HashMap<String, usize>,
}

#[derive(Serialize)]
pub struct SyncResponse {
    pub new_count: usize,
    pub updated_count: usize,
    pub unchanged_count: usize,
    pub skipped_count: usize,
    pub conflict_count: usize,
    pub failed_count: usize,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub startup: bool,
    pub sync_enabled: bool,
    pub scan_interval_seconds: u64,
    pub include_thinking: bool,
    pub include_tool_calls: bool,
    pub max_tool_result_chars: usize,
    pub redact_secrets: bool,
    // AI settings
    pub ai_enabled: bool,
    pub ai_auto_extract: bool,
    pub ai_extract_tags: bool,
    pub ai_extract_problems: bool,
    pub ai_extract_decisions: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            startup: true,
            sync_enabled: true,
            scan_interval_seconds: 300,
            include_thinking: false,
            include_tool_calls: true,
            max_tool_result_chars: 10000,
            redact_secrets: true,
            ai_enabled: true,
            ai_auto_extract: true,
            ai_extract_tags: true,
            ai_extract_problems: true,
            ai_extract_decisions: true,
        }
    }
}

#[derive(Serialize)]
pub struct DoctorResponse {
    pub checks: Vec<DoctorCheckDto>,
    pub all_ok: bool,
}

#[derive(Serialize)]
pub struct DoctorCheckDto {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct SessionDto {
    pub id: String,
    pub source: String,
    pub title: Option<String>,
    pub project_path: Option<String>,
    pub message_count: usize,
    pub updated_at: Option<String>,
}

// ── Commands ───────────────────────────────────────────────────────────────────

/// get_status — returns app status. Runtime info comes from AppState.
#[tauri::command]
pub async fn get_status(
    state: State<'_, AppState>,
    runtime: State<'_, Arc<Mutex<Option<SiyuanRuntime>>>>,
) -> Result<StatusResponse, String> {
    let runtime_dto = {
        let lock = runtime.lock().await;
        if let Some(rt) = lock.as_ref() {
            let health = rt.health().await;
            Some(RuntimeStatusDto {
                state: format!("{:?}", health.state),
                port: health.port,
                version: health.version,
                mode: "Embedded".to_string(),
            })
        } else {
            None
        }
    };

    if let Some(engine) = state.engine() {
        engine
            .status()
            .map(|s| StatusResponse {
                total_sessions: s.total_sessions,
                synced: s.synced,
                pending: s.pending,
                conflict: s.conflict,
                failed: s.failed,
                last_sync_at: s.last_sync_at,
                runtime: runtime_dto,
            })
            .map_err(|e| e.to_string())
    } else {
        Ok(StatusResponse {
            total_sessions: 0, synced: 0, pending: 0, conflict: 0, failed: 0,
            last_sync_at: None, runtime: runtime_dto,
        })
    }
}

#[tauri::command]
pub async fn scan_sources(
    source: Option<String>,
    state: State<'_, AppState>,
) -> Result<ScanResponse, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let result = engine.scan(source.as_deref()).await;
    Ok(ScanResponse {
        total: result.total,
        by_source: result.by_source,
    })
}

#[tauri::command]
pub async fn sync_now(
    source: Option<String>,
    overwrite: Option<bool>,
    state: State<'_, AppState>,
) -> Result<SyncResponse, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let opts = aiks_core::SyncOptions {
        source_filter: source,
        dry_run: false,
        overwrite: overwrite.unwrap_or(false),
    };
    engine
        .sync(opts)
        .await
        .map(|s| SyncResponse {
            new_count: s.new_count,
            updated_count: s.updated_count,
            unchanged_count: s.unchanged_count,
            skipped_count: s.skipped_count,
            conflict_count: s.conflict_count,
            failed_count: s.failed_count,
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_doctor(state: State<'_, AppState>) -> Result<DoctorResponse, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let result = engine.doctor().await;
    Ok(DoctorResponse {
        checks: result.checks.into_iter().map(|c| DoctorCheckDto {
            name: c.name,
            ok: c.ok,
            message: c.message,
        }).collect(),
        all_ok: result.all_ok,
    })
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let settings_file = state.data_dir.join("config").join("app.json");
    if settings_file.exists() {
        let content = std::fs::read_to_string(&settings_file).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    } else {
        Ok(AppSettings::default())
    }
}

/// B10 + R07: save_settings must preserve the rest of aiks.toml.
/// It loads the full typed config, mutates ONLY the UI-controlled fields and
/// writes the result back atomically. Previously a fixed template regenerated
/// the whole file, silently dropping AI endpoints/keys, embedding config and
/// provider paths on next restart.
#[tauri::command]
pub async fn save_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Save JSON for UI consumption
    let settings_file = state.data_dir.join("config").join("app.json");
    std::fs::create_dir_all(settings_file.parent().unwrap()).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&settings_file, json).map_err(|e| e.to_string())?;

    // Load the existing full config (or defaults if missing), mutate the
    // controlled subset, and atomically write it back.
    let toml_file = state.data_dir.join("config").join("aiks.toml");
    let mut config = if toml_file.exists() {
        aiks_core::Config::from_file(&toml_file)
            .map_err(|e| format!("Failed to load existing aiks.toml: {}", e))?
    } else {
        aiks_core::Config::default()
    };

    // [ai] — only the UI-controlled fields
    config.ai.enabled = settings.ai_enabled;
    config.ai.auto_extract = settings.ai_auto_extract;

    // [sync]
    config.sync.scan_interval_seconds = settings.scan_interval_seconds;
    config.sync.watch_enabled = settings.sync_enabled;

    // [security]
    config.security.redact_secrets = settings.redact_secrets;

    // [content]
    config.content.include_thinking = settings.include_thinking;
    config.content.include_tool_calls = settings.include_tool_calls;
    config.content.max_tool_result_chars = settings.max_tool_result_chars;

    let toml_content =
        toml::to_string_pretty(&config).map_err(|e| format!("Failed to serialize config: {}", e))?;

    // Atomic write: temp file + rename so a crash never leaves a half-written config.
    let tmp_file = state.data_dir.join("config").join("aiks.toml.tmp");
    std::fs::write(&tmp_file, &toml_content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_file, &toml_file).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn open_data_folder(state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(&state.data_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn restart_siyuan(
    runtime: State<'_, Arc<Mutex<Option<SiyuanRuntime>>>>,
) -> Result<u16, String> {
    let mut lock = runtime.lock().await;
    if let Some(rt) = lock.as_mut() {
        rt.restart().await.map(|info| info.port).map_err(|e| e.to_string())
    } else {
        Err("SiYuan runtime not initialized".to_string())
    }
}

#[tauri::command]
pub async fn get_siyuan_url(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.siyuan_url().await)
}

#[tauri::command]
pub async fn get_sessions(
    source: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<SessionDto>, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let scan = engine.scan(source.as_deref()).await;
    let limit = limit.unwrap_or(100);
    Ok(scan.summaries.into_iter().take(limit).map(|s| SessionDto {
        id: s.external_session_id,
        source: s.source.display_name().to_string(),
        title: s.title,
        project_path: s.project_path,
        message_count: s.message_count,
        updated_at: s.updated_at.map(|t| t.to_rfc3339()),
    }).collect())
}

#[tauri::command]
pub async fn get_sync_history(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let status = engine.status().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "total_sessions": status.total_sessions,
        "synced": status.synced,
        "pending": status.pending,
        "last_sync_at": status.last_sync_at,
        "last_discovered": status.last_sync_discovered,
        "last_changed": status.last_sync_changed,
        "last_synced": status.last_sync_synced,
        "last_failed": status.last_sync_failed
    }))
}

// ===== AI Knowledge Commands =====

#[tauri::command]
pub async fn get_ai_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let ai = engine.ai_status().await;
    Ok(serde_json::json!({
        "enabled": ai.enabled,
        "healthy": ai.healthy,
        "model": ai.model,
        "display_name": ai.display_name,
        "extraction_stats": {
            "total": ai.extraction_stats.total,
            "success": ai.extraction_stats.success,
            "skipped": ai.extraction_stats.skipped,
            "failed": ai.extraction_stats.failed,
            "pending": ai.extraction_stats.pending
        }
    }))
}

#[tauri::command]
pub async fn test_ai_connection(state: State<'_, AppState>) -> Result<bool, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    Ok(engine.ai_health_check().await)
}

#[tauri::command]
pub async fn extract_session_now(
    source: String,
    session_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let source_kind = aiks_core::SourceKind::from_str(&source)
        .ok_or_else(|| format!("Unknown source: {}", source))?;
    engine
        .extract_session_now(source_kind, &session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_knowledge_stats(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let stats = aiks_core::knowledge::service::get_extraction_stats(engine.db().as_ref())
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "total": stats.total,
        "success": stats.success,
        "skipped": stats.skipped,
        "failed": stats.failed,
        "pending": stats.pending
    }))
}

#[tauri::command]
pub async fn get_recent_knowledge(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let conn = db.conn();
    let limit = limit.unwrap_or(10);

    // Check table exists
    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='knowledge_extraction'",
        [],
        |row| row.get::<_, i64>(0),
    ).unwrap_or(0) > 0;

    if !table_exists {
        return Ok(vec![]);
    }

    let mut stmt = conn.prepare(
        "SELECT source, external_session_id, category, knowledge_score, knowledge_document_id, updated_at
         FROM knowledge_extraction WHERE status = 'SUCCESS'
         ORDER BY updated_at DESC LIMIT ?1"
    ).map_err(|e| e.to_string())?;

    let rows = stmt.query_map([limit as i64], |row| {
        Ok(serde_json::json!({
            "source": row.get::<_, String>(0)?,
            "session_id": row.get::<_, String>(1)?,
            "category": row.get::<_, Option<String>>(2)?,
            "score": row.get::<_, Option<f64>>(3)?,
            "doc_id": row.get::<_, Option<String>>(4)?,
            "updated_at": row.get::<_, String>(5)?
        }))
    }).map_err(|e| e.to_string())?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[tauri::command]
pub async fn open_knowledge_window(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri::Manager;
    let url = state.siyuan_url().await;
    if let Some(window) = app.get_webview_window("knowledge") {
        if let Some(url_str) = &url {
            if let Ok(parsed) = url_str.parse() {
                let _ = window.navigate(parsed);
            }
        }
        let _ = window.show();
        let _ = window.set_focus();
    }
    Ok(())
}

/// Unified full status — single source of truth for all UI pages.
/// Eliminates state divergence between provider scan and DB counts.
#[tauri::command]
pub async fn get_full_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let full = engine.full_status().await;
    Ok(serde_json::to_value(full).map_err(|e| e.to_string())?)
}

/// Sync AND enqueue extraction (the main sync button).
/// AI extraction is non-blocking — if AI fails, Raw Sync still succeeds.
#[tauri::command]
pub async fn sync_and_extract(
    source: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let opts = aiks_core::SyncOptions {
        source_filter: source,
        dry_run: false,
        overwrite: false,
    };

    // R09: use the SAME orchestration as startup/watcher/tray — real sync that
    // also submits PipelineJobs to the worker. Previously this only ran a plain
    // sync and logged candidates without ever enqueuing anything.
    let stats = engine.sync_and_enqueue_extraction(opts).await.map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "discovered": stats.discovered,
        "new_count": stats.new_count,
        "updated_count": stats.updated_count,
        "unchanged_count": stats.unchanged_count,
        "skipped_count": stats.skipped_count,
        "failed_count": stats.failed_count,
        "extraction_queued": stats.extraction_candidates.len()
    }))
}

// ===== V3 Pipeline Commands =====

/// List pipeline runs for the Processing Center page
#[tauri::command]
pub async fn list_pipeline_runs(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let orchestrator = aiks_core::PipelineOrchestrator::new(db);
    let runs = orchestrator.list_runs(limit.unwrap_or(200)).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(runs).map_err(|e| e.to_string())?)
}

/// Get pipeline run detail (with stage trace)
#[tauri::command]
pub async fn get_pipeline_detail(
    run_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let orchestrator = aiks_core::PipelineOrchestrator::new(db);
    match orchestrator.get_run_detail(&run_id).map_err(|e| e.to_string())? {
        Some(detail) => Ok(serde_json::to_value(detail).map_err(|e| e.to_string())?),
        None => Err(format!("Pipeline run not found: {}", run_id)),
    }
}

/// Get pipeline processing stats
#[tauri::command]
pub async fn get_pipeline_stats(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let orchestrator = aiks_core::PipelineOrchestrator::new(db);
    let stats = orchestrator.get_stats().map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(stats).map_err(|e| e.to_string())?)
}

/// List sessions (V3 version — from DB with pipeline status)
#[tauri::command]
pub async fn list_sessions_v3(
    source: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let conn = db.conn();
    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);

    let source_filter = source.as_deref().unwrap_or("");
    let rows: Vec<serde_json::Value> = if source_filter.is_empty() {
        let mut stmt = conn.prepare(
            "SELECT ss.id, ss.source, ss.external_session_id, ss.title, ss.project_name, ss.project_path,
                    ss.source_updated_at, ss.content_hash,
                    pr.id as run_id, pr.status as pipeline_status, pr.current_stage
             FROM source_session ss
             LEFT JOIN pipeline_run pr ON pr.session_id = ss.id AND pr.pipeline_version = 'v3'
             ORDER BY ss.source_updated_at DESC
             LIMIT ?1 OFFSET ?2"
        ).map_err(|e| e.to_string())?;
        let mapped = stmt.query_map(rusqlite::params![limit as i64, offset as i64], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "source": row.get::<_, String>(1)?,
                "session_id": row.get::<_, String>(2)?,
                "title": row.get::<_, Option<String>>(3)?,
                "project_name": row.get::<_, Option<String>>(4)?,
                "project_path": row.get::<_, Option<String>>(5)?,
                "updated_at": row.get::<_, Option<String>>(6)?,
                "content_hash": row.get::<_, Option<String>>(7)?,
                "run_id": row.get::<_, Option<String>>(8)?,
                "pipeline_status": row.get::<_, Option<String>>(9)?,
                "current_stage": row.get::<_, Option<String>>(10)?
            }))
        }).map_err(|e| e.to_string())?;
        let result: Vec<serde_json::Value> = mapped.filter_map(|r| r.ok()).collect();
        result
    } else {
        let mut stmt = conn.prepare(
            "SELECT ss.id, ss.source, ss.external_session_id, ss.title, ss.project_name, ss.project_path,
                    ss.source_updated_at, ss.content_hash,
                    pr.id as run_id, pr.status as pipeline_status, pr.current_stage
             FROM source_session ss
             LEFT JOIN pipeline_run pr ON pr.session_id = ss.id AND pr.pipeline_version = 'v3'
             WHERE ss.source = ?1
             ORDER BY ss.source_updated_at DESC
             LIMIT ?2 OFFSET ?3"
        ).map_err(|e| e.to_string())?;
        let mapped = stmt.query_map(rusqlite::params![source_filter, limit as i64, offset as i64], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "source": row.get::<_, String>(1)?,
                "session_id": row.get::<_, String>(2)?,
                "title": row.get::<_, Option<String>>(3)?,
                "project_name": row.get::<_, Option<String>>(4)?,
                "project_path": row.get::<_, Option<String>>(5)?,
                "updated_at": row.get::<_, Option<String>>(6)?,
                "content_hash": row.get::<_, Option<String>>(7)?,
                "run_id": row.get::<_, Option<String>>(8)?,
                "pipeline_status": row.get::<_, Option<String>>(9)?,
                "current_stage": row.get::<_, Option<String>>(10)?
            }))
        }).map_err(|e| e.to_string())?;
        let result: Vec<serde_json::Value> = mapped.filter_map(|r| r.ok()).collect();
        result
    };

    let total: i64 = if source_filter.is_empty() {
        conn.query_row("SELECT COUNT(*) FROM source_session", [], |r| r.get(0)).unwrap_or(0)
    } else {
        conn.query_row("SELECT COUNT(*) FROM source_session WHERE source = ?1",
            rusqlite::params![source_filter], |r| r.get(0)).unwrap_or(0)
    };

    Ok(serde_json::json!({
        "items": rows,
        "total": total,
        "limit": limit,
        "offset": offset
    }))
}

/// List knowledge items with proper project/category filtering (B21 fix)
#[tauri::command]
pub async fn list_knowledge(
    project: Option<String>,
    category: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let conn = db.conn();
    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);

    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='knowledge_item'",
        [], |row| row.get::<_, i64>(0),
    ).unwrap_or(0) > 0;

    if !table_exists {
        return Ok(serde_json::json!({"items": [], "total": 0}));
    }

    // R02: Build WHERE clause and parameters together so the bind count always
    // matches the placeholder count for every filter combination.
    let mut where_parts: Vec<&str> = Vec::new();
    let mut filter_params: Vec<rusqlite::types::Value> = Vec::new();
    let has_project = project.as_deref().map(|p| !p.is_empty()).unwrap_or(false);
    let has_category = category.as_deref().map(|c| !c.is_empty()).unwrap_or(false);

    if has_project {
        where_parts.push("project_name = ?");
        filter_params.push(rusqlite::types::Value::Text(project.clone().unwrap()));
    }
    if has_category {
        where_parts.push("category = ?");
        filter_params.push(rusqlite::types::Value::Text(category.clone().unwrap()));
    }

    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_parts.join(" AND "))
    };

    let rows: Vec<serde_json::Value> = {
        let sql = format!(
            "SELECT id, source_session_id, project_name, title, category, summary, tags, confidence, created_at, updated_at
             FROM knowledge_item
             {}
             ORDER BY updated_at DESC
             LIMIT ? OFFSET ?",
            where_clause
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let mut all_params = filter_params.clone();
        all_params.push(rusqlite::types::Value::Integer(limit as i64));
        all_params.push(rusqlite::types::Value::Integer(offset as i64));
        let mapped = stmt.query_map(
            rusqlite::params_from_iter(all_params),
            |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "session_id": row.get::<_, i64>(1)?,
                    "project_name": row.get::<_, Option<String>>(2)?,
                    "title": row.get::<_, String>(3)?,
                    "category": row.get::<_, String>(4)?,
                    "summary": row.get::<_, String>(5)?,
                    "tags": row.get::<_, String>(6)?,
                    "confidence": row.get::<_, f64>(7)?,
                    "created_at": row.get::<_, String>(8)?,
                    "updated_at": row.get::<_, String>(9)?
                }))
            }
        ).map_err(|e| e.to_string())?;
        let r: Vec<serde_json::Value> = mapped.filter_map(|r| r.ok()).collect();
        r
    };

    // B21/R02: Use same WHERE predicate and same filter params for total count.
    // Surface DB errors to the caller instead of masking them with 0.
    let total: i64 = {
        let count_sql = format!("SELECT COUNT(*) FROM knowledge_item {}", where_clause);
        conn.query_row(&count_sql, rusqlite::params_from_iter(filter_params.clone()), |r| r.get(0))
            .map_err(|e| e.to_string())?
    };

    Ok(serde_json::json!({
        "items": rows,
        "total": total,
        "limit": limit,
        "offset": offset
    }))
}

/// Search knowledge through the Core search pipeline.
///
/// The response keeps the existing desktop result shape while also surfacing
/// degraded search state (for example, vector backend unavailable).
#[tauri::command]
pub async fn search_knowledge(
    query: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let outcome = engine
        .search_knowledge(&query, limit.unwrap_or(20))
        .await
        .map_err(|e| e.to_string())?;

    let degraded = outcome.degraded();
    let warnings: Vec<String> = outcome
        .degradations
        .iter()
        .map(|d| d.message.clone())
        .collect();

    let db = engine.db();
    let repo = aiks_core::KnowledgeRepo::new(&db);
    let mut results = Vec::new();
    for hit in outcome.hits {
        if let Some(detail) = repo
            .get_by_id(&hit.knowledge_id)
            .map_err(|e| e.to_string())?
        {
            results.push(serde_json::json!({
                "id": detail.id,
                "title": detail.title,
                "category": detail.category,
                "summary": detail.summary,
                "project_name": detail.project_name,
                "tags": detail.tags,
                "confidence": detail.confidence,
                "match_type": hit.match_type
            }));
        }
    }

    Ok(serde_json::json!({
        "results": results,
        "query": query,
        "total": results.len(),
        "degraded": degraded,
        "warnings": warnings
    }))
}

// ===== V3 Detail Commands =====

/// Get session detail with raw messages
#[tauri::command]
pub async fn get_session_detail(
    session_id: i64,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let conn = db.conn();

    // Session info
    let session = conn.query_row(
        "SELECT id, source, external_session_id, title, project_name, project_path,
                source_updated_at, content_hash
         FROM source_session WHERE id = ?1",
        rusqlite::params![session_id],
        |row| Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "source": row.get::<_, String>(1)?,
            "session_id": row.get::<_, String>(2)?,
            "title": row.get::<_, Option<String>>(3)?,
            "project_name": row.get::<_, Option<String>>(4)?,
            "project_path": row.get::<_, Option<String>>(5)?,
            "updated_at": row.get::<_, Option<String>>(6)?,
            "content_hash": row.get::<_, Option<String>>(7)?
        })),
    ).map_err(|e| e.to_string())?;

    // Pipeline run
    let pipeline_run: Option<serde_json::Value> = conn.query_row(
        "SELECT id, status, current_stage, pipeline_version, started_at, finished_at, error_stage, error_message
         FROM pipeline_run WHERE session_id = ?1 ORDER BY updated_at DESC LIMIT 1",
        rusqlite::params![session_id],
        |row| Ok(serde_json::json!({
            "run_id": row.get::<_, String>(0)?,
            "status": row.get::<_, String>(1)?,
            "current_stage": row.get::<_, Option<String>>(2)?,
            "pipeline_version": row.get::<_, String>(3)?,
            "started_at": row.get::<_, Option<String>>(4)?,
            "finished_at": row.get::<_, Option<String>>(5)?,
            "error_stage": row.get::<_, Option<String>>(6)?,
            "error_message": row.get::<_, Option<String>>(7)?
        })),
    ).ok();

    // Session chunks
    let mut chunk_stmt = conn.prepare(
        "SELECT chunk_index, message_start, message_end, token_count FROM session_chunk WHERE session_id = ?1 ORDER BY chunk_index"
    ).map_err(|e| e.to_string())?;
    let chunks: Vec<serde_json::Value> = {
        let mapped = chunk_stmt.query_map(rusqlite::params![session_id], |row| {
            Ok(serde_json::json!({
                "index": row.get::<_, i32>(0)?,
                "message_start": row.get::<_, i32>(1)?,
                "message_end": row.get::<_, i32>(2)?,
                "token_count": row.get::<_, i32>(3)?
            }))
        }).map_err(|e| e.to_string())?;
        let r: Vec<serde_json::Value> = mapped.filter_map(|r| r.ok()).collect();
        r
    };

    // Knowledge items
    let mut ki_stmt = conn.prepare(
        "SELECT id, title, category, summary, confidence, created_at FROM knowledge_item WHERE source_session_id = ?1"
    ).map_err(|e| e.to_string())?;
    let knowledge: Vec<serde_json::Value> = {
        let mapped = ki_stmt.query_map(rusqlite::params![session_id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "category": row.get::<_, String>(2)?,
                "summary": row.get::<_, String>(3)?,
                "confidence": row.get::<_, f64>(4)?,
                "created_at": row.get::<_, String>(5)?
            }))
        }).map_err(|e| e.to_string())?;
        let r: Vec<serde_json::Value> = mapped.filter_map(|r| r.ok()).collect();
        r
    };

    Ok(serde_json::json!({
        "session": session,
        "pipeline_run": pipeline_run,
        "chunks": chunks,
        "knowledge": knowledge
    }))
}

/// Get knowledge item detail
#[tauri::command]
pub async fn get_knowledge_detail(
    knowledge_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let knowledge_repo = aiks_core::KnowledgeRepo::new(&db);

    match knowledge_repo.get_by_id(&knowledge_id).map_err(|e| e.to_string())? {
        Some(detail) => {
            let chunks: Vec<serde_json::Value> = detail.chunks.iter().map(|c| serde_json::json!({
                "id": c.id,
                "heading": c.heading,
                "chunk_index": c.chunk_index,
                "token_count": c.token_count,
                "text": &c.text[..c.text.len().min(500)],
                "has_embedding": c.has_embedding
            })).collect();

            Ok(serde_json::json!({
                "id": detail.id,
                "session_id": detail.session_id,
                "project_name": detail.project_name,
                "title": detail.title,
                "category": detail.category,
                "summary": detail.summary,
                "content": detail.content,
                "tags": detail.tags,
                "confidence": detail.confidence,
                "created_at": detail.created_at,
                "updated_at": detail.updated_at,
                "source": detail.source,
                "session_external_id": detail.session_external_id,
                "session_title": detail.session_title,
                "chunks": chunks
            }))
        }
        None => Err(format!("Knowledge item not found: {}", knowledge_id)),
    }
}

/// R09: manually trigger extraction backfill for historical sessions.
/// Submits real PipelineJobs (not just DB rows) to the running worker.
#[tauri::command]
pub async fn backfill_extractions(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let submitted = engine.backfill_pending_extractions().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "submitted": submitted }))
}

/// Push distilled knowledge items into the SiYuan knowledge notebook
/// (knowledge-first tree with source-session deep links).
#[tauri::command]
pub async fn sync_knowledge_to_siyuan(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let stats = engine
        .sync_knowledge_to_siyuan(false)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "created": stats.created,
        "updated": stats.updated,
        "unchanged": stats.unchanged,
        "conflict": stats.conflict,
        "failed": stats.failed,
    }))
}

/// Manually trigger pipeline for a specific session
#[tauri::command]
pub async fn run_pipeline_for_session(
    session_id: i64,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let conn = db.conn();

    let result = conn.query_row(
        "SELECT source, external_session_id, title, project_name FROM source_session WHERE id = ?1",
        rusqlite::params![session_id],
        |row| Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
        )),
    ).map_err(|e| format!("Session not found: {}", e))?;

    let (source, ext_id, title, project) = result;

    engine.enqueue_pipeline_for_session(session_id, ext_id, source, title, project)
        .map_err(|e| e.to_string())
}

/// Get hybrid search results (FTS5 + vector) through Core.
#[tauri::command]
pub async fn hybrid_search(
    query: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let outcome = engine
        .search_knowledge(&query, limit.unwrap_or(20))
        .await
        .map_err(|e| e.to_string())?;

    let degraded = outcome.degraded();
    let warnings: Vec<String> = outcome
        .degradations
        .iter()
        .map(|d| d.message.clone())
        .collect();
    let items: Vec<serde_json::Value> = outcome
        .hits
        .into_iter()
        .map(|hit| serde_json::json!({
            "knowledge_id": hit.knowledge_id,
            "chunk_id": hit.chunk_id,
            "chunk_text": hit.chunk_text,
            "score": hit.score,
            "match_type": hit.match_type
        }))
        .collect();

    Ok(serde_json::json!({
        "results": items,
        "query": query,
        "total": items.len(),
        "degraded": degraded,
        "warnings": warnings
    }))
}
