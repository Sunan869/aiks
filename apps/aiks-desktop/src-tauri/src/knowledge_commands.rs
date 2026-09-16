use aiks_core::{
    CreateKnowledgeInput, KnowledgeListFilter, KnowledgeRecord, KnowledgeService,
    UpdateKnowledgeInput,
};
use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;

#[derive(Debug, Deserialize)]
pub struct KnowledgeWritePayload {
    pub title: String,
    pub category: Option<String>,
    pub project_name: Option<String>,
    pub summary: Option<String>,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct KnowledgeUpdatePayload {
    pub title: String,
    pub category: String,
    pub project_name: Option<String>,
    pub summary: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn to_json(item: KnowledgeRecord) -> serde_json::Value {
    let tags = serde_json::to_string(&item.tags).unwrap_or_else(|_| "[]".to_string());
    serde_json::json!({
        "id": item.id,
        "session_id": item.source_session_id,
        "project_name": item.project_name,
        "title": item.title,
        "category": item.category,
        "summary": item.summary,
        "content": item.content,
        "tags": tags,
        "confidence": item.confidence,
        "source_type": item.source_type,
        "managed_by": item.managed_by,
        "status": item.status,
        "is_favorite": item.is_favorite,
        "created_at": item.created_at,
        "updated_at": item.updated_at,
        "source": item.source,
        "session_external_id": item.session_external_id,
        "session_title": item.session_title,
        "chunks": []
    })
}

#[tauri::command]
pub async fn list_knowledge_v4(
    project: Option<String>,
    category: Option<String>,
    source_type: Option<String>,
    status: Option<String>,
    favorite: Option<bool>,
    limit: Option<usize>,
    offset: Option<usize>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let result = KnowledgeService::new(&db)
        .list(KnowledgeListFilter {
            project,
            category,
            source_type,
            status,
            favorite,
            limit,
            offset,
        })
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "items": result.items.into_iter().map(to_json).collect::<Vec<_>>(),
        "total": result.total,
        "limit": result.limit,
        "offset": result.offset
    }))
}

#[tauri::command]
pub async fn get_knowledge_detail_v4(
    knowledge_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let item = KnowledgeService::new(&db)
        .get(&knowledge_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Knowledge item not found: {knowledge_id}"))?;
    Ok(to_json(item))
}

#[tauri::command]
pub async fn create_knowledge(
    input: KnowledgeWritePayload,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let item = KnowledgeService::new(&db)
        .create_manual(CreateKnowledgeInput {
            title: input.title,
            category: input.category,
            project_name: input.project_name,
            summary: input.summary,
            content: input.content,
            tags: input.tags,
        })
        .map_err(|e| e.to_string())?;
    Ok(to_json(item))
}

#[tauri::command]
pub async fn update_knowledge(
    knowledge_id: String,
    input: KnowledgeUpdatePayload,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let item = KnowledgeService::new(&db)
        .update(
            &knowledge_id,
            UpdateKnowledgeInput {
                title: input.title,
                category: input.category,
                project_name: input.project_name,
                summary: input.summary,
                content: input.content,
                tags: input.tags,
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(to_json(item))
}

#[tauri::command]
pub async fn set_knowledge_favorite(
    knowledge_id: String,
    favorite: bool,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let item = KnowledgeService::new(&db)
        .set_favorite(&knowledge_id, favorite)
        .map_err(|e| e.to_string())?;
    Ok(to_json(item))
}

#[tauri::command]
pub async fn archive_knowledge(
    knowledge_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let item = KnowledgeService::new(&db)
        .archive(&knowledge_id)
        .map_err(|e| e.to_string())?;
    Ok(to_json(item))
}

#[tauri::command]
pub async fn restore_knowledge(
    knowledge_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let item = KnowledgeService::new(&db)
        .restore(&knowledge_id)
        .map_err(|e| e.to_string())?;
    Ok(to_json(item))
}

#[tauri::command]
pub async fn search_knowledge_v4(
    query: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let outcome = engine
        .search_knowledge(&query, limit.unwrap_or(20))
        .await
        .map_err(|e| e.to_string())?;
    let db = engine.db();
    let service = KnowledgeService::new(&db);
    let warnings: Vec<String> = outcome
        .degradations
        .iter()
        .map(|d| d.message.clone())
        .collect();
    let degraded = outcome.degraded();
    let mut results = Vec::new();
    for hit in outcome.hits {
        if let Some(item) = service.get(&hit.knowledge_id).map_err(|e| e.to_string())? {
            if item.status != "active" {
                continue;
            }
            results.push(serde_json::json!({
                "id": item.id,
                "title": item.title,
                "category": item.category,
                "summary": item.summary,
                "project_name": item.project_name,
                "tags": serde_json::to_string(&item.tags).unwrap_or_else(|_| "[]".into()),
                "confidence": item.confidence,
                "source_type": item.source_type,
                "is_favorite": item.is_favorite,
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

#[tauri::command]
pub async fn publish_knowledge(
    knowledge_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let runtime_url = state.siyuan_url().await;
    let config = engine.config();
    let sink = if let Some(url) = runtime_url {
        aiks_core::sink::SiYuanSink::embedded(url, config.siyuan.notebook_name.clone())
            .map_err(|e| e.to_string())?
    } else {
        aiks_core::sink::SiYuanSink::new(config.siyuan.clone()).map_err(|e| e.to_string())?
    };
    let result =
        aiks_core::knowledge::publish_knowledge_to_siyuan(&db, &sink, &knowledge_id, false)
            .await
            .map_err(|e| e.to_string())?;
    serde_json::to_value(result).map_err(|e| e.to_string())
}
