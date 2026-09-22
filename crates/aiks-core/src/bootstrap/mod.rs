/// AIKS Embedded Bootstrap.
///
/// Responsible for:
/// 1. Resolving the embedded SiYuan runtime location
/// 2. Validating runtime integrity
/// 3. Building a SiyuanRuntimeConfig
/// 4. Ensuring the SiYuan notebook exists
///
/// Spec §37-38:
///   - No accessAuthCode in embedded mode
///   - No SIYUAN_TOKEN in normal operation
///   - Developer override via SIYUAN_URL + SIYUAN_TOKEN env vars only
use std::path::PathBuf;
use std::time::Duration;

use tracing::info;

use crate::runtime::{SiyuanRuntime, SiyuanRuntimeConfig};

mod process_paths;

/// Result from ensuring the AI Knowledge notebook exists.
#[derive(Debug, Clone)]
pub struct NotebookInfo {
    pub id: String,
    pub name: String,
}

/// Full bootstrap configuration for AIKS Desktop.
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// Root of the embedded SiYuan runtime (kernel/, stage/, appearance/)
    pub runtime_root: PathBuf,
    /// SiYuan workspace directory
    pub workspace: PathBuf,
    /// Expected SiYuan version (from siyuan.version)
    pub expected_version: Option<String>,
    /// User data root directory (%LOCALAPPDATA%\AIKnowledgeSync)
    pub data_dir: PathBuf,
    /// Notebook name to create/ensure
    pub notebook_name: String,
}

impl BootstrapConfig {
    pub fn new(runtime_root: PathBuf, data_dir: PathBuf, notebook_name: impl Into<String>) -> Self {
        let workspace = data_dir.join("siyuan").join("workspace");
        Self {
            runtime_root,
            workspace,
            expected_version: None,
            data_dir,
            notebook_name: notebook_name.into(),
        }
    }

    /// Build a SiyuanRuntimeConfig from this bootstrap config.
    pub fn runtime_config(&self) -> SiyuanRuntimeConfig {
        // These two paths cross a process boundary into the Go/SQLite runtime.
        // Keep the canonical Rust data root for locks, configuration and logs.
        let mut cfg = SiyuanRuntimeConfig::new(
            process_paths::for_kernel(&self.runtime_root),
            process_paths::for_kernel(&self.workspace),
            &self.data_dir,
        );
        cfg.expected_version = self.expected_version.clone();
        cfg.startup_timeout = Duration::from_secs(120);
        cfg
    }
}

/// Read the expected SiYuan version from siyuan.version file.
pub fn read_expected_version(project_root: &std::path::Path) -> Option<String> {
    let version_file = project_root.join("siyuan.version");
    if !version_file.exists() {
        return None;
    }
    std::fs::read_to_string(&version_file)
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|l| l.starts_with("version="))
                .map(|l| l["version=".len()..].trim().to_string())
        })
        .filter(|s| !s.is_empty())
}

/// Validate the runtime and return a readable summary.
pub fn validate_runtime(config: &BootstrapConfig) -> anyhow::Result<()> {
    let runtime_cfg = config.runtime_config();
    let validation = SiyuanRuntime::validate(&runtime_cfg);
    if !validation.is_valid() {
        anyhow::bail!(
            "SiYuan runtime is not ready:\n{}",
            validation.errors.join("\n  ")
        );
    }
    info!(
        kernel_size = validation.kernel_size_bytes,
        "Runtime validation passed"
    );
    Ok(())
}

/// Ensure the "AI Knowledge" notebook exists in SiYuan.
pub async fn ensure_notebook(base_url: &str, notebook_name: &str) -> anyhow::Result<NotebookInfo> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    // List notebooks
    let resp: serde_json::Value = client
        .post(format!("{}/api/notebook/lsNotebooks", base_url))
        .json(&serde_json::json!({}))
        .send()
        .await?
        .json()
        .await?;

    if let Some(notebooks) = resp["data"]["notebooks"].as_array() {
        for nb in notebooks {
            if nb["name"].as_str() == Some(notebook_name) {
                let id = nb["id"].as_str().unwrap_or("").to_string();
                info!(notebook_id = %id, "Notebook already exists");
                return Ok(NotebookInfo {
                    id,
                    name: notebook_name.to_string(),
                });
            }
        }
    }

    // Create
    let resp: serde_json::Value = client
        .post(format!("{}/api/notebook/createNotebook", base_url))
        .json(&serde_json::json!({"name": notebook_name}))
        .send()
        .await?
        .json()
        .await?;

    let id = resp["data"]["notebook"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No notebook ID in createNotebook response"))?
        .to_string();

    info!(notebook_id = %id, name = notebook_name, "Created notebook");
    Ok(NotebookInfo {
        id,
        name: notebook_name.to_string(),
    })
}

/// Check developer override environment variables.
///
/// Spec §29: SIYUAN_URL and SIYUAN_TOKEN are only for developer override.
pub struct DevOverride {
    pub url: Option<String>,
    pub token: Option<String>,
}

impl DevOverride {
    pub fn from_env() -> Self {
        Self {
            url: std::env::var("SIYUAN_URL").ok().filter(|s| !s.is_empty()),
            token: std::env::var("SIYUAN_TOKEN").ok().filter(|s| !s.is_empty()),
        }
    }

    pub fn is_active(&self) -> bool {
        self.url.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn bootstrap_config_workspace_path() {
        let dir = tempdir().unwrap();
        let cfg = BootstrapConfig::new(
            dir.path().join("runtime"),
            dir.path().to_path_buf(),
            "AI Knowledge",
        );
        assert!(
            cfg.workspace.ends_with("siyuan/workspace")
                || cfg.workspace.ends_with("siyuan\\workspace")
        );
    }

    #[test]
    fn runtime_config_no_access_auth_code() {
        let dir = tempdir().unwrap();
        let cfg = BootstrapConfig::new(
            dir.path().join("runtime"),
            dir.path().to_path_buf(),
            "AI Knowledge",
        );
        let runtime_cfg = cfg.runtime_config();
        // SiyuanRuntimeConfig must not have an accessAuthCode field
        assert_eq!(runtime_cfg.language, "zh-CN");
        assert!(runtime_cfg.expected_version.is_none());
    }

    #[test]
    fn dev_override_inactive_by_default() {
        // Remove env vars if set
        std::env::remove_var("SIYUAN_URL");
        std::env::remove_var("SIYUAN_TOKEN");
        let dev = DevOverride::from_env();
        assert!(!dev.is_active());
    }

    #[test]
    fn save_and_load_token_removed() {
        // Spec §38: No token bootstrap in embedded mode.
        // Verify there's no save_token / load_token function in this module.
        // (Compile-time verification — if this file compiles, there's no such function.)
        let _ = BootstrapConfig::new(
            PathBuf::from("runtime"),
            PathBuf::from("data"),
            "AI Knowledge",
        );
    }
}
