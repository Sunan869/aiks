use serde::Serialize;
use crate::config::{Config, ExternalProviderConfig};
use crate::model::SourceKind;
use super::ProviderHealth;

pub const EXTERNAL_SOURCES: [SourceKind; 11] = [
    SourceKind::Antigravity, SourceKind::Cursor, SourceKind::CursorAgent,
    SourceKind::Cline, SourceKind::RooCode, SourceKind::KiloCode, SourceKind::GithubCopilot,
    SourceKind::KimiCode, SourceKind::QwenCode, SourceKind::Continue, SourceKind::Aider,
];
pub const ALL_SOURCES: [SourceKind; 16] = [
    SourceKind::ClaudeCode, SourceKind::Codex, SourceKind::GeminiCli, SourceKind::OpenCode, SourceKind::WorkBuddy,
    SourceKind::Antigravity, SourceKind::Cursor, SourceKind::CursorAgent, SourceKind::Cline, SourceKind::RooCode, SourceKind::KiloCode,
    SourceKind::GithubCopilot, SourceKind::KimiCode, SourceKind::QwenCode, SourceKind::Continue, SourceKind::Aider,
];
pub fn external_config(config: &Config, source: SourceKind) -> Option<&ExternalProviderConfig> {
    let p = &config.providers;
    Some(match source {
        SourceKind::Antigravity => &p.antigravity, SourceKind::Cursor => &p.cursor,
        SourceKind::CursorAgent => &p.cursor_agent, SourceKind::Cline => &p.cline,
        SourceKind::RooCode => &p.roo_code, SourceKind::KiloCode => &p.kilo_code,
        SourceKind::GithubCopilot => &p.github_copilot, SourceKind::KimiCode => &p.kimi_code,
        SourceKind::QwenCode => &p.qwen_code, SourceKind::Continue => &p.continue_dev,
        SourceKind::Aider => &p.aider, _ => return None,
    })
}
#[derive(Debug, Clone, Serialize)]
pub struct ProviderDescriptor {
    pub key: &'static str,
    pub display_name: &'static str,
    pub config_key: &'static str,
    pub enabled: bool,
    pub paths: Vec<String>,
    pub status: String,
    pub message: String,
}
pub fn descriptors(config: &Config, health: &[(SourceKind, ProviderHealth)]) -> Vec<ProviderDescriptor> {
    ALL_SOURCES.iter().map(|&source| {
        let (config_key, enabled, paths) = if let Some(p) = external_config(config, source) {
            let paths = if !p.path.trim().is_empty() { vec![p.path.clone()] } else { p.paths.clone() };
            (source.as_str(), p.enabled, paths)
        } else {
            let p = &config.providers;
            let (key, enabled, path) = match source {
                SourceKind::ClaudeCode => ("claude", p.claude.enabled, &p.claude.path),
                SourceKind::Codex => ("codex", p.codex.enabled, &p.codex.path),
                SourceKind::GeminiCli => ("gemini", p.gemini.enabled, &p.gemini.path),
                SourceKind::OpenCode => ("opencode", p.opencode.enabled, &p.opencode.path),
                SourceKind::WorkBuddy => ("workbuddy", p.workbuddy.enabled, &p.workbuddy.path),
                _ => unreachable!("external source is missing from catalog"),
            };
            (key, enabled, if path.trim().is_empty() { Vec::new() } else { vec![path.clone()] })
        };
        let h = health.iter().find(|(kind, _)| *kind == source).map(|(_, h)| h);
        let status = if !enabled { "disabled" } else { match h {
            Some(ProviderHealth::Ok) => "ok", Some(ProviderHealth::Unsupported { .. }) => "unsupported",
            Some(ProviderHealth::NotConfigured) => "not_configured", Some(ProviderHealth::Error { .. }) => "error",
            Some(ProviderHealth::NotFound { .. }) => "not_found", None => "unknown",
        }};
        ProviderDescriptor { key: source.as_str(), display_name: source.display_name(), config_key,
            enabled, paths, status: status.into(), message: h.map_or("", ProviderHealth::message).into() }
    }).collect()
}
