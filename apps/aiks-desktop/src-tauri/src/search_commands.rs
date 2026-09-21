use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use aiks_core::search::query_cache::QueryEmbeddingCache;
use aiks_core::{
    ai::ModelService, SearchCorpus, UnifiedSearchFilter, UnifiedSearchOutcome, UnifiedSearchService,
};
use tauri::{ipc::Channel, State, Window};
use tokio::sync::oneshot;

use crate::app_state::AppState;

#[derive(Default)]
struct RequestSlot {
    latest: u64,
    cancel: Option<oneshot::Sender<()>>,
}

#[derive(Default)]
struct SearchRuntime {
    requests: Mutex<HashMap<String, RequestSlot>>,
    provider: Mutex<Option<(String, Arc<QueryEmbeddingCache>)>>,
}

impl SearchRuntime {
    fn begin(&self, window: &str, id: u64) -> Result<oneshot::Receiver<()>, String> {
        let mut requests = self.requests.lock().map_err(|_| "Search state poisoned")?;
        let slot = requests.entry(window.to_string()).or_default();
        if id <= slot.latest {
            return Err("Search cancelled".to_string());
        }
        if let Some(sender) = slot.cancel.take() {
            let _ = sender.send(());
        }
        let (sender, receiver) = oneshot::channel();
        slot.latest = id;
        slot.cancel = Some(sender);
        Ok(receiver)
    }

    fn cancel(&self, window: &str, id: u64) -> Result<(), String> {
        let mut requests = self.requests.lock().map_err(|_| "Search state poisoned")?;
        let slot = requests.entry(window.to_string()).or_default();
        if id >= slot.latest {
            if let Some(sender) = slot.cancel.take() {
                let _ = sender.send(());
            }
            // A cancellation can reach Rust before its corresponding search.
            // Keep the watermark, not one tombstone for every keystroke.
            slot.latest = id;
        }
        Ok(())
    }

    fn finish(&self, window: &str, id: u64) {
        if let Ok(mut requests) = self.requests.lock() {
            if let Some(slot) = requests.get_mut(window) {
                if slot.latest == id {
                    slot.cancel = None;
                }
            }
        }
    }

    fn provider(&self, engine: &aiks_core::AiksEngine) -> Result<Arc<QueryEmbeddingCache>, String> {
        // Identity includes endpoint/model/dimensions/auth, but is never logged
        // or persisted. Changing a provider discards its process-local cache.
        let identity =
            serde_json::to_string(engine.embedding_config()).map_err(|e| e.to_string())?;
        let mut cached = self
            .provider
            .lock()
            .map_err(|_| "Search provider state poisoned")?;
        if let Some((key, provider)) = cached.as_ref() {
            if key == &identity {
                return Ok(provider.clone());
            }
        }
        let models = ModelService::new(
            engine.ai_config().clone(),
            engine.embedding_config().clone(),
        )
        .map_err(|e| e.to_string())?;
        let provider = Arc::new(QueryEmbeddingCache::new(Arc::new(models)));
        *cached = Some((identity, provider.clone()));
        Ok(provider)
    }
}

fn runtime() -> &'static SearchRuntime {
    static RUNTIME: OnceLock<SearchRuntime> = OnceLock::new();
    RUNTIME.get_or_init(SearchRuntime::default)
}

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

fn transport(mut outcome: UnifiedSearchOutcome, semantic_enabled: bool) -> serde_json::Value {
    if !semantic_enabled {
        outcome
            .warnings
            .retain(|warning| !warning.starts_with("Semantic search is disabled"));
        outcome.degraded = !outcome.warnings.is_empty();
    }
    serde_json::json!({
        "hits": outcome.hits,
        "degraded": outcome.degraded,
        "warnings": outcome.warnings,
        "semantic_enabled": semantic_enabled,
    })
}

/// Optional progress/cancellation arguments preserve the existing command API.
/// A cancellation uses this same command with cancel_only=true and no query.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn search_all_v42(
    query: String,
    limit: Option<usize>,
    corpora: Option<Vec<String>>,
    project: Option<String>,
    source: Option<String>,
    request_id: Option<u64>,
    cancel_only: Option<bool>,
    on_progress: Option<Channel<serde_json::Value>>,
    window: Window,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let runtime = runtime();
    let label = window.label().to_string();
    if cancel_only.unwrap_or(false) {
        runtime.cancel(&label, request_id.ok_or("Missing search request ID")?)?;
        return Ok(serde_json::Value::Null);
    }
    let receiver = request_id.map(|id| runtime.begin(&label, id)).transpose()?;
    let search = async {
        let engine = state.engine().ok_or("Engine not initialized")?;
        let semantic_enabled = engine.embedding_config().enabled;
        let provider = runtime.provider(engine.as_ref())?;
        let service = UnifiedSearchService::new(engine.db(), provider);
        let outcome = service
            .search_with_progress(
                &query,
                limit.unwrap_or(30).clamp(1, 100),
                UnifiedSearchFilter {
                    corpora: parse_corpora(corpora.unwrap_or_default())?,
                    project,
                    source,
                },
                move |partial| {
                    if let Some(channel) = &on_progress {
                        let _ = channel.send(transport(partial.clone(), semantic_enabled));
                    }
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(transport(outcome, semantic_enabled))
    };
    let result = match receiver {
        Some(receiver) => tokio::select! {
            biased;
            _ = receiver => Err("Search cancelled".to_string()),
            result = search => result,
        },
        None => search.await,
    };
    if let Some(id) = request_id {
        runtime.finish(&label, id);
    }
    result
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

    #[test]
    fn cancel_before_start_and_late_completion_do_not_reactivate_old_search() {
        let runtime = SearchRuntime::default();
        runtime.cancel("control", 10).unwrap();
        assert!(runtime.begin("control", 10).is_err());
        let mut old = runtime.begin("control", 11).unwrap();
        let mut current = runtime.begin("control", 12).unwrap();
        assert_eq!(old.try_recv(), Ok(()));
        runtime.finish("control", 11);
        runtime.cancel("control", 11).unwrap();
        assert!(matches!(
            current.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        runtime.cancel("control", 12).unwrap();
        assert_eq!(current.try_recv(), Ok(()));
    }

    #[test]
    fn cancellation_is_scoped_to_the_calling_window() {
        let runtime = SearchRuntime::default();
        let mut first = runtime.begin("first", 1).unwrap();
        let mut second = runtime.begin("second", 1).unwrap();
        runtime.cancel("first", 1).unwrap();
        assert_eq!(first.try_recv(), Ok(()));
        assert!(matches!(
            second.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
    }
}
