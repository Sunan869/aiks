use std::sync::Arc;

use aiks_core::{ai::ModelService, SearchCorpus, UnifiedSearchFilter, UnifiedSearchService};
use tauri::State;

use crate::app_state::AppState;

fn parse_corpora(values: Vec<String>) -> Result<Vec<SearchCorpus>, String> {
    values
        .into_iter()
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "knowledge" => Ok(SearchCorpus::Knowledge),
            "session" | "sessions" => Ok(SearchCorpus::Session),
            other => Err(format!("Unsupported search corpus: {other}")),
        })
        .collect()
}

#[tauri::command]
pub async fn search_all_v42(
    query: String,
    limit: Option<usize>,
    corpora: Option<Vec<String>>,
    project: Option<String>,
    source: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    let semantic_enabled = engine.embedding_config().enabled;
    let model_service = ModelService::new(
        engine.ai_config().clone(),
        engine.embedding_config().clone(),
    )
    .map_err(|error| error.to_string())?;
    let service = UnifiedSearchService::new(engine.db(), Arc::new(model_service));
    let mut outcome = service
        .search(
            &query,
            limit.unwrap_or(30).clamp(1, 100),
            UnifiedSearchFilter {
                corpora: parse_corpora(corpora.unwrap_or_default())?,
                project,
                source,
            },
        )
        .await
        .map_err(|error| error.to_string())?;

    if !semantic_enabled {
        outcome
            .warnings
            .retain(|warning| !warning.starts_with("Semantic search is disabled"));
        outcome.degraded = !outcome.warnings.is_empty();
    }

    Ok(serde_json::json!({
        "hits": outcome.hits,
        "degraded": outcome.degraded,
        "warnings": outcome.warnings,
        "semantic_enabled": semantic_enabled,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_transport_values_are_explicit() {
        assert_eq!(
            parse_corpora(vec!["knowledge".into(), "session".into()]).unwrap(),
            vec![SearchCorpus::Knowledge, SearchCorpus::Session]
        );
        assert!(parse_corpora(vec!["siyuan".into()]).is_err());
    }
}
