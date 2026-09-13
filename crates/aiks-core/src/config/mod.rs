use std::path::PathBuf;

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
        Self { redact_secrets: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SiYuanConfig {
    pub base_url: String,
    pub token: String,
    pub notebook_name: String,
    pub session_root: String,
    pub knowledge_root: String,
}

impl Default for SiYuanConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:6806".to_string(),
            token: String::new(),
            notebook_name: "AI Knowledge".to_string(),
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
        let content = std::fs::read_to_string(path)?;
        let mut config: Config = toml::from_str(&content)?;
        // Allow SIYUAN_TOKEN env override
        if let Ok(token) = std::env::var("SIYUAN_TOKEN") {
            if !token.is_empty() {
                config.siyuan.token = token;
            }
        }
        Ok(config)
    }

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

    /// Return the AIKS state database path.
    pub fn state_db_path(&self) -> PathBuf {
        if let Some(local_data) = dirs::data_local_dir() {
            local_data.join("AIKnowledgeSync").join("aiks.db")
        } else if let Some(home) = dirs::home_dir() {
            home.join(".aiks").join("aiks.db")
        } else {
            PathBuf::from("aiks.db")
        }
    }

    /// Return the archive base directory.
    pub fn archive_dir(&self) -> PathBuf {
        if let Some(local_data) = dirs::data_local_dir() {
            local_data.join("AIKnowledgeSync").join("archive")
        } else if let Some(home) = dirs::home_dir() {
            home.join(".aiks").join("archive")
        } else {
            PathBuf::from("archive")
        }
    }
}
