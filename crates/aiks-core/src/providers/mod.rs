pub mod claude;
pub mod codex;
pub mod gemini;
pub mod opencode;
pub mod share;
pub mod workbuddy;

use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{NormalizedSession, SourceKind};

/// Summary of a discovered session (cheap to compute, no full message parsing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub source: SourceKind,
    pub external_session_id: String,
    pub title: Option<String>,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
    pub source_path: Option<PathBuf>,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub message_count: usize,
}

/// Health status of a provider
#[derive(Debug, Clone)]
pub enum ProviderHealth {
    Ok,
    NotConfigured,
    NotFound { message: String },
    Error { message: String },
}

impl ProviderHealth {
    pub fn is_ok(&self) -> bool {
        matches!(self, ProviderHealth::Ok)
    }

    pub fn message(&self) -> &str {
        match self {
            ProviderHealth::Ok => "OK",
            ProviderHealth::NotConfigured => "Not configured",
            ProviderHealth::NotFound { message } => message.as_str(),
            ProviderHealth::Error { message } => message.as_str(),
        }
    }
}

/// The provider interface for session discovery and loading.
///
/// Providers are read-only with respect to the source data.
#[async_trait]
pub trait SessionProvider: Send + Sync {
    fn source(&self) -> SourceKind;
    fn parser_version(&self) -> &'static str;

    /// Discover all available session summaries (cheap: no full message parsing)
    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>>;

    /// Load a full normalized session from a summary
    async fn load_session(&self, summary: &SessionSummary) -> anyhow::Result<NormalizedSession>;

    /// Check if this provider is healthy and available
    async fn health_check(&self) -> ProviderHealth;
}

/// Registry of all enabled session providers.
pub struct ProviderRegistry {
    providers: Vec<Box<dyn SessionProvider>>,
}

impl ProviderRegistry {
    pub fn new(providers: Vec<Box<dyn SessionProvider>>) -> Self {
        Self { providers }
    }

    /// Returns all enabled providers.
    pub fn providers(&self) -> &[Box<dyn SessionProvider>] {
        &self.providers
    }

    /// Find a provider by source kind.
    pub fn get(&self, source: SourceKind) -> Option<&dyn SessionProvider> {
        self.providers
            .iter()
            .find(|p| p.source() == source)
            .map(|p| p.as_ref())
    }

    /// Discover all sessions from all providers.
    /// Single provider failures are isolated and logged.
    pub async fn discover_all(&self) -> Vec<SessionSummary> {
        self.discover_all_detailed()
            .await
            .into_iter()
            .filter_map(|(_, result)| match result {
                Ok(sessions) => Some(sessions),
                Err(e) => {
                    tracing::warn!(error = %e, "Provider discover_sessions failed");
                    None
                }
            })
            .flatten()
            .collect()
    }

    /// R14: discover per source, keeping each provider's scan result (success
    /// or failure) so callers can distinguish "no sessions" from "scan failed".
    pub async fn discover_all_detailed(
        &self,
    ) -> Vec<(SourceKind, anyhow::Result<Vec<SessionSummary>>)> {
        let mut results = Vec::new();
        for provider in &self.providers {
            let result = provider.discover_sessions().await;
            if let Err(e) = &result {
                tracing::warn!(
                    source = ?provider.source(),
                    error = %e,
                    "Provider discover_sessions failed"
                );
            }
            results.push((provider.source(), result));
        }
        results
    }

    /// Perform health check on all providers.
    pub async fn health_check_all(&self) -> Vec<(SourceKind, ProviderHealth)> {
        let mut results = Vec::new();
        for provider in &self.providers {
            let health = provider.health_check().await;
            results.push((provider.source(), health));
        }
        results
    }
}

/// Build a ProviderRegistry from configuration.
pub fn build_registry(config: &crate::config::Config) -> ProviderRegistry {
    let mut providers: Vec<Box<dyn SessionProvider>> = Vec::new();

    if config.providers.claude.enabled {
        let path = config.claude_path();
        match claude::ClaudeProvider::new(path) {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => tracing::warn!("Claude provider not available: {}", e),
        }
    }

    if config.providers.codex.enabled {
        let path = config.codex_path();
        match codex::CodexProvider::new(path) {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => tracing::warn!("Codex provider not available: {}", e),
        }
    }

    if config.providers.gemini.enabled {
        let path = config.gemini_path();
        match gemini::GeminiProvider::new(path) {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => tracing::warn!("Gemini provider not available: {}", e),
        }
    }

    if config.providers.opencode.enabled {
        let path = config.opencode_path();
        match opencode::OpenCodeProvider::new(path) {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => tracing::warn!("OpenCode provider not available: {}", e),
        }
    }

    if config.providers.workbuddy.enabled {
        let path = config.workbuddy_path();
        match workbuddy::WorkBuddyProvider::new(path) {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => tracing::warn!("WorkBuddy provider not available: {}", e),
        }
    }

    let share_root = crate::share_import::share_cache_root();
    for source in [
        SourceKind::ChatgptShare,
        SourceKind::ClaudeShare,
        SourceKind::GeminiShare,
    ] {
        providers.push(Box::new(share::CachedShareProvider::new(
            source,
            share_root.clone(),
        )));
    }

    ProviderRegistry::new(providers)
}
