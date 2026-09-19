// CI lint baseline: pre-existing Clippy debt; remove allowances incrementally.
#![allow(clippy::derivable_impls)]

use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::ai::AiModelConfig;
use crate::pipeline::EmbeddingConfig;

/// Full AIKS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub sync: SyncConfig,
    pub providers: ProvidersConfig,
    pub content: ContentConfig,
    pub security: SecurityConfig,
    pub siyuan: SiYuanConfig,
    pub archive: ArchiveConfig,
    pub extractor: ExtractorConfig,
    pub ai: AiModelConfig,
    pub embedding: EmbeddingConfig,
    pub desktop: DesktopConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sync: SyncConfig::default(),
            providers: ProvidersConfig::default(),
            content: ContentConfig::default(),
            security: SecurityConfig::default(),
            siyuan: SiYuanConfig::default(),
            archive: ArchiveConfig::default(),
            extractor: ExtractorConfig::default(),
            ai: AiModelConfig::default(),
            embedding: EmbeddingConfig::default(),
            desktop: DesktopConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopConfig {
    pub startup: bool,
    pub close_to_tray: bool,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            startup: true,
            close_to_tray: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    pub scan_interval_seconds: u64,
    pub watch_enabled: bool,
    pub debounce_seconds: u64,
    pub max_parallel_sessions: usize,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            scan_interval_seconds: 300,
            watch_enabled: true,
            debounce_seconds: 2,
            max_parallel_sessions: 4,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProvidersConfig {
    pub claude: ProviderConfig,
    pub codex: CodexProviderConfig,
    pub gemini: ProviderConfig,
    pub opencode: ProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub enabled: bool,
    /// Override path (empty string = use default)
    pub path: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CodexProviderConfig {
    pub enabled: bool,
    pub path: String,
    pub include_archived_sessions: bool,
}

impl Default for CodexProviderConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: String::new(),
            include_archived_sessions: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContentConfig {
    pub include_system: bool,
    pub include_thinking: bool,
    pub include_tool_calls: bool,
    pub include_tool_results: bool,
    pub max_tool_result_chars: usize,
    pub minimum_messages: usize,
    pub minimum_session_chars: usize,
}

impl Default for ContentConfig {
    fn default() -> Self {
        Self {
            include_system: false,
            include_thinking: false,
            include_tool_calls: true,
            include_tool_results: true,
            max_tool_result_chars: 10000,
            // 0 means sync all sessions regardless of message count
            // (Codex DB fast-path sets count=0 for all sessions)
            minimum_messages: 0,
            minimum_session_chars: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    pub redact_secrets: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            redact_secrets: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SiYuanConfig {
    pub base_url: String,
    pub token: String,
    /// Knowledge notebook — only distilled knowledge docs live here (clean tree).
    pub notebook_name: String,
    /// Session archive notebook — raw session markdown is archived here.
    pub session_notebook_name: String,
    pub session_root: String,
    pub knowledge_root: String,
}

impl Default for SiYuanConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:6806".to_string(),
            token: String::new(),
            notebook_name: "AI Knowledge".to_string(),
            session_notebook_name: "AI Session Archive".to_string(),
            session_root: "/10 AI Sessions".to_string(),
            knowledge_root: "/20 Knowledge".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ArchiveConfig {
    pub enabled: bool,
    pub compression: String,
}

impl Default for ArchiveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            compression: "gzip".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtractorConfig {
    pub enabled: bool,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub minimum_confidence: f64,
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "openai-compatible".to_string(),
            base_url: "http://127.0.0.1:11434/v1".to_string(),
            model: "qwen3.8:27b".to_string(),
            minimum_confidence: 0.85,
        }
    }
}

impl Config {
    /// Load config from a TOML file.
    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read AIKS config: {}", path.display()))?;
        let mut config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse AIKS config: {}", path.display()))?;
        // Allow SIYUAN_TOKEN env override
        if let Ok(token) = std::env::var("SIYUAN_TOKEN") {
            if !token.is_empty() {
                config.siyuan.token = token;
            }
        }
        Ok(config)
    }

    /// Persist the complete typed config directly to its final path.
    ///
    /// The config is small and written only from explicit settings actions, so
    /// a direct replacement is preferable to remove+rename on Windows: the
    /// latter can temporarily delete a valid config and surface opaque
    /// `os error 2` failures when the second rename cannot find its source.
    pub fn write_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("AIKS config path has no parent: {}", path.display()))?;
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create AIKS config directory: {}", parent.display()))?;
        let content = toml::to_string_pretty(self).context("Failed to serialize AIKS config")?;
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write AIKS config: {}", path.display()))?;
        Ok(())
    }

    /// Return the AIKS state database path.
    pub fn state_db_path(&self) -> PathBuf {
        data_root().join("aiks.db")
    }

    /// Return the archive base directory.
    pub fn archive_dir(&self) -> PathBuf {
        data_root().join("archive")
    }
}

/// Resolve the AIKS data root directory.
///
/// Priority: `AIKS_DATA_DIR` env var → pointer file → default
/// (`%LOCALAPPDATA%\AIKnowledgeSync`). The pointer file
/// `<default>\data-root.txt` holds one line: the new root path. This
/// relocates ALL heavy data (state DB, SiYuan workspace incl. its temp
/// index, archive, logs) to another drive without env-var setup.
pub fn data_root() -> PathBuf {
    let default = default_data_root();
    let env_override = std::env::var("AIKS_DATA_DIR").ok();
    let pointer_file = std::fs::read_to_string(default.join("data-root.txt")).ok();
    resolve_data_root(default, env_override.as_deref(), pointer_file.as_deref())
}

impl Config {
    /// Return the resolved path for the given provider, or None for default.
    pub fn claude_path(&self) -> Option<PathBuf> {
        if self.providers.claude.path.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.providers.claude.path))
        }
    }

    pub fn codex_path(&self) -> Option<PathBuf> {
        if self.providers.codex.path.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.providers.codex.path))
        }
    }

    pub fn gemini_path(&self) -> Option<PathBuf> {
        if self.providers.gemini.path.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.providers.gemini.path))
        }
    }

    pub fn opencode_path(&self) -> Option<PathBuf> {
        if self.providers.opencode.path.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.providers.opencode.path))
        }
    }
}

/// Default data root: `%LOCALAPPDATA%\AIKnowledgeSync` (falls back to
/// `~/.AIKnowledgeSync` when the local-data dir is unavailable).
fn default_data_root() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("AIKnowledgeSync")
}

/// Pure data-root resolution logic — testable without touching the real env.
///
/// Whitespace-only values are ignored; the env var wins over the pointer file
/// (explicit per-process override beats persistent machine state).
pub(crate) fn resolve_data_root(
    default: PathBuf,
    env_override: Option<&str>,
    pointer_file: Option<&str>,
) -> PathBuf {
    if let Some(env) = env_override.map(str::trim).filter(|s| !s.is_empty()) {
        return PathBuf::from(env);
    }
    if let Some(content) = pointer_file.map(str::trim).filter(|s| !s.is_empty()) {
        return PathBuf::from(content);
    }
    default
}

#[cfg(test)]
mod data_root_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn resolve_prefers_env_over_pointer() {
        let p = resolve_data_root(
            PathBuf::from("C:\\default"),
            Some("E:\\env-root"),
            Some("E:\\ptr-root"),
        );
        assert_eq!(p, PathBuf::from("E:\\env-root"));
    }

    #[test]
    fn resolve_prefers_pointer_over_default() {
        let p = resolve_data_root(PathBuf::from("C:\\default"), None, Some(" E:\\ptr-root\n"));
        assert_eq!(p, PathBuf::from("E:\\ptr-root"));
    }

    #[test]
    fn resolve_ignores_blank_overrides() {
        assert_eq!(
            resolve_data_root(PathBuf::from("C:\\default"), Some("  "), Some("")),
            PathBuf::from("C:\\default")
        );
    }

    #[test]
    fn resolve_defaults_without_any_override() {
        assert_eq!(
            resolve_data_root(PathBuf::from("C:\\default"), None, None),
            PathBuf::from("C:\\default")
        );
    }

    #[test]
    fn write_file_can_replace_an_existing_config_and_reload_it() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config").join("aiks.toml");
        let mut config = Config::default();

        config.embedding.enabled = false;
        config.write_file(&path).unwrap();
        config.embedding.enabled = true;
        config.embedding.base_url = "http://example.invalid/v1".to_string();
        config.write_file(&path).unwrap();

        let reloaded = Config::from_file(&path).unwrap();
        assert!(reloaded.embedding.enabled);
        assert_eq!(reloaded.embedding.base_url, "http://example.invalid/v1");
    }
}
