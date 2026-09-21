mod aider;
mod antigravity;
pub mod catalog;
pub mod claude;
mod cline_family;
pub mod codex;
mod continue_dev;
mod copilot;
mod cursor;
mod cursor_agent;
pub mod discovery;
pub mod gemini;
mod kimi;
pub mod local_io;
mod local_paths;
mod message_parts;
pub mod native;
pub mod opencode;
mod qwen;
pub mod settings;
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
    Unsupported { message: String },
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
            ProviderHealth::Unsupported { message } => message.as_str(),
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

    /// Compatibility default for existing providers; new sources report scope and completeness.
    async fn discover_report(&self) -> anyhow::Result<discovery::DiscoveryReport> {
        Ok(discovery::DiscoveryReport {
            sessions: self.discover_sessions().await?,
            ..Default::default()
        })
    }

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
    pub async fn discover_selected(
        &self,
        selected: Option<SourceKind>,
    ) -> Vec<(SourceKind, anyhow::Result<discovery::DiscoveryReport>)> {
        use std::future::{poll_fn, Future};
        use std::pin::Pin;
        use std::task::Poll;
        type Output = (SourceKind, anyhow::Result<discovery::DiscoveryReport>);
        let limit = tokio::sync::Semaphore::new(4);
        let mut pending: Vec<Pin<Box<dyn Future<Output = Output> + Send + '_>>> = self
            .providers
            .iter()
            .filter(|p| selected.is_none_or(|s| p.source() == s))
            .map(|p| {
                let limit = &limit;
                Box::pin(async move {
                    let _permit = limit
                        .acquire()
                        .await
                        .expect("local provider semaphore stays open");
                    (p.source(), p.discover_report().await)
                }) as Pin<Box<dyn Future<Output = Output> + Send + '_>>
            })
            .collect();
        let mut reports = Vec::new();
        while !pending.is_empty() {
            let (index, result) = poll_fn(|cx| {
                for (index, future) in pending.iter_mut().enumerate() {
                    if let Poll::Ready(output) = future.as_mut().poll(cx) {
                        return Poll::Ready((index, output));
                    }
                }
                Poll::Pending
            })
            .await;
            drop(pending.swap_remove(index));
            reports.push(result);
        }
        reports
    }

    pub async fn discover_all(&self) -> Vec<SessionSummary> {
        self.discover_selected(None)
            .await
            .into_iter()
            .filter_map(|(source, result)| match result {
                Ok(report) => {
                    if !report.complete {
                        tracing::warn!(
                            source = source.as_str(),
                            diagnostics = report.diagnostics.len(),
                            "Provider discovery incomplete; valid sessions retained"
                        );
                    }
                    Some(report.sessions)
                }
                Err(_) => {
                    tracing::warn!(source = source.as_str(), "Provider discovery failed");
                    None
                }
            })
            .flatten()
            .collect()
    }

    pub async fn discover_all_detailed(
        &self,
    ) -> Vec<(SourceKind, anyhow::Result<Vec<SessionSummary>>)> {
        self.discover_selected(None)
            .await
            .into_iter()
            .map(|(source, result)| {
                let result = result.and_then(|report| {
                    anyhow::ensure!(
                        report.complete,
                        "Provider scan incomplete; missing detection prohibited"
                    );
                    Ok(report.sessions)
                });
                (source, result)
            })
            .collect()
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

    for source in catalog::EXTERNAL_SOURCES {
        if let Some(settings) = catalog::external_config(config, source).filter(|p| p.enabled) {
            match native::NativeProvider::new(source, settings) {
                Ok(provider) => providers.push(Box::new(provider)),
                Err(_) => tracing::warn!(
                    source = source.as_str(),
                    "External provider configuration invalid"
                ),
            }
        }
    }
    ProviderRegistry::new(providers)
}
