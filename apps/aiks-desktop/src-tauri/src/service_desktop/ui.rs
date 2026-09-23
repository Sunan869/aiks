//! Local setup and session selection remain available without model/service readiness.
use super::*;
use crate::service_client::{browse::LocalSessionBrowser, preferences::{Onboarding, SourceScope, UiPreferences}};

impl ServiceDesktop {
    pub(super) async fn local_store(&self) -> Result<Arc<CollectorOutbox>, String> {
        let mut store = self.outbox.write().await;
        if let Some(store) = &*store { return Ok(store.clone()); }
        if *self.phase.read().await == "stopped" { return Err("service_stopping".into()); }
        let root = crate::app_state::data_dir().join("service-local");
        let outbox = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            validate_private_path(&root, false)?;
            std::fs::create_dir_all(&root)?;
            let path = root.join("collector.db");
            validate_private_path(&path, true)?;
            Ok(Arc::new(CollectorOutbox::open(&path)?))
        }).await.map_err(|_| "collector_storage_unavailable")?
            .map_err(|_| "collector_storage_unavailable")?;
        *store = Some(outbox.clone());
        Ok(outbox)
    }
    pub async fn ui_preferences(&self) -> Result<UiPreferences, String> {
        let store = self.local_store().await?;
        tokio::task::spawn_blocking(move || store.ui_preferences()).await
            .map_err(|_| "collector_storage_unavailable")?.map_err(|e| e.to_string())
    }
    pub async fn finish_onboarding(&self, skipped: bool) -> Result<UiPreferences, String> {
        let store = self.local_store().await?;
        tokio::task::spawn_blocking(move || store.finish_onboarding(if skipped { Onboarding::Skipped } else { Onboarding::Completed })).await
            .map_err(|_| "collector_storage_unavailable")?.map_err(|e| e.to_string())
    }
    pub async fn save_sources(&self, sources: Vec<String>) -> Result<UiPreferences, String> {
        for key in &sources { self.source(key)?; }
        let store = self.local_store().await?;
        tokio::task::spawn_blocking(move || store.save_selected_sources(sources)).await
            .map_err(|_| "collector_storage_unavailable")?.map_err(|e| e.to_string())
    }
    fn source(&self, key: &str) -> Result<aiks_core::SourceKind, String> {
        let source = aiks_core::SourceKind::from_str(key).ok_or("invalid_source")?;
        if source.as_str() != key || self.providers.get(source).is_none() {
            return Err("source_disabled_or_unavailable".into());
        }
        Ok(source)
    }
    pub(super) fn source_key(&self, source: aiks_core::SourceKind) -> Result<String, String> {
        let paths = descriptors(&self.provider_config, &[]).into_iter()
            .find(|d| d.key == source.as_str()).ok_or("invalid_source")?.paths;
        let bytes = serde_json::to_vec(&(source.as_str(), paths)).map_err(|_| "invalid_source")?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
    async fn source_scope(&self, source: aiks_core::SourceKind) -> Result<Option<SourceScope>, String> {
        let Some(client) = self.client.read().await.clone() else { return Ok(None); };
        let identity = client.connection();
        SourceScope::new(identity.instance_id(), identity.space_id(), source, &self.source_key(source)?)
            .map(Some).map_err(|e|e.to_string())
    }
    pub async fn scan_sessions(&self, key: String) -> Result<Value, String> {
        let _permit = self.gate.try_acquire().map_err(|_| "collector_busy")?;
        let source = self.source(&key)?;
        let provider = self.providers.get(source).ok_or("source_disabled_or_unavailable")?;
        let browser = tokio::time::timeout(Duration::from_secs(30), LocalSessionBrowser::scan(provider))
            .await.map_err(|_| "scan_timeout")?.map_err(|e|e.to_string())?;
        self.browsers.lock().await.insert(source, Arc::new(browser));
        self.browse_sessions(key, String::new(), 0).await
    }
    pub async fn browse_sessions(&self, key: String, query: String, offset: usize) -> Result<Value, String> {
        let source = self.source(&key)?;
        let browser = self.browsers.lock().await.get(&source).cloned().ok_or("scan_required")?;
        let scope = self.source_scope(source).await?;
        let can_save = scope.is_some();
        let excluded = if let Some(scope) = scope {
            let store = self.local_store().await?;
            tokio::task::spawn_blocking(move || store.excluded(&scope)).await
                .map_err(|_| "collector_storage_unavailable")?.map_err(|e|e.to_string())?
        } else { vec![] };
        let page = browser.page(&query, offset, 30, &excluded).map_err(|e|e.to_string())?;
        let mut value = serde_json::to_value(page).map_err(|_| "invalid_session_list")?;
        value["can_save"] = Value::Bool(can_save);
        Ok(value)
    }
    pub async fn preview_session(&self, key: String, row: String) -> Result<Value, String> {
        let _permit = self.gate.try_acquire().map_err(|_| "collector_busy")?;
        let source = self.source(&key)?;
        let browser = self.browsers.lock().await.get(&source).cloned().ok_or("scan_required")?;
        let provider = self.providers.get(source).ok_or("source_disabled_or_unavailable")?;
        let preview = tokio::time::timeout(Duration::from_secs(15), browser.preview(provider, &row))
            .await.map_err(|_| "scan_timeout")?.map_err(|e|e.to_string())?;
        serde_json::to_value(preview).map_err(|_| "invalid_session_preview".into())
    }
    pub async fn exclude_sessions(&self, key: String, rows: Vec<String>, excluded: bool) -> Result<Value, String> {
        if rows.is_empty() || rows.len() > 1000 { return Err("invalid_selection".into()); }
        let _permit = self.gate.try_acquire().map_err(|_| "collector_busy")?;
        let source = self.source(&key)?;
        let browser = self.browsers.lock().await.get(&source).cloned().ok_or("scan_required")?;
        let ids = rows.iter().map(|key| browser.resolve(key).map(|s|s.external_session_id.clone()))
            .collect::<Result<Vec<_>,_>>().map_err(|_| "scan_required")?;
        let scope = self.source_scope(source).await?.ok_or("service_not_ready")?;
        let store = self.local_store().await?;
        let change = tokio::task::spawn_blocking(move || store.set_excluded(&scope, &ids, excluded)).await
            .map_err(|_| "collector_storage_unavailable")?.map_err(|e|e.to_string())?;
        serde_json::to_value(change).map_err(|_| "invalid_selection_result".into())
    }
}
