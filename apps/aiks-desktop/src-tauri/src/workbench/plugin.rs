use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Map, Value};

const REQUIRED_FILES: [&str; 3] = ["plugin.json", "index.js", "index.css"];
const BRIDGE_PLUGIN_NAME: &str = "aiks-bridge";

pub fn install_bridge_plugin(source: &Path, workspace: &Path) -> anyhow::Result<PathBuf> {
    for name in REQUIRED_FILES {
        let path = source.join(name);
        if !path.is_file() {
            anyhow::bail!("AIKS bridge plugin resource is missing: {}", path.display());
        }
    }

    let target = workspace.join("data").join("plugins").join(BRIDGE_PLUGIN_NAME);
    fs::create_dir_all(&target)?;
    copy_tree(source, &target)?;
    Ok(target)
}

/// Ensure the bundled AIKS bridge is actually loadable by a fresh SiYuan workspace.
///
/// Copying a plugin into `data/plugins` only installs it. SiYuan persists plugin
/// activation separately and, on desktop, will not load any plugin until Bazaar
/// trust is accepted. The embedded AIKS workspace therefore opts into plugin
/// loading and explicitly enables only the bundled `aiks-bridge` before the
/// workbench webview is mounted.
pub async fn ensure_bridge_plugin_enabled(base_url: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let base_url = base_url.trim_end_matches('/');

    let conf_response: Value = client
        .post(format!("{base_url}/api/system/getConf"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure_api_success(&conf_response, "read SiYuan configuration")?;

    let bazaar = conf_response
        .pointer("/data/conf/bazaar")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("SiYuan configuration is missing data.conf.bazaar"))?;
    let bazaar = bridge_bazaar_config(bazaar);

    let bazaar_response: Value = client
        .post(format!("{base_url}/api/setting/setBazaar"))
        .json(&Value::Object(bazaar))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure_api_success(&bazaar_response, "enable SiYuan plugin loading")?;

    let enable_response: Value = client
        .post(format!("{base_url}/api/petal/setPetalEnabled"))
        .json(&json!({
            "packageName": "aiks-bridge",
            "enabled": true,
            "app": "aiks-embedded"
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure_api_success(&enable_response, "enable AIKS bridge plugin")?;

    Ok(())
}

fn bridge_bazaar_config(current: &Map<String, Value>) -> Map<String, Value> {
    let mut config = current.clone();
    config.insert("trust".to_string(), json!(true));
    config.insert("petalDisabled".to_string(), json!(false));
    config
}

fn ensure_api_success(response: &Value, operation: &str) -> anyhow::Result<()> {
    let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code == 0 {
        return Ok(());
    }

    let message = response
        .get("msg")
        .and_then(Value::as_str)
        .unwrap_or("unknown SiYuan API error");
    anyhow::bail!("failed to {operation}: {message} (code {code})")
}

fn copy_tree(source: &Path, target: &Path) -> anyhow::Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            fs::create_dir_all(&target_path)?;
            copy_tree(&source_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::{json, Map};

    use super::{bridge_bazaar_config, ensure_api_success, install_bridge_plugin};

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("aiks-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn installs_bridge_into_workspace_and_updates_existing_files() {
        let source = scratch("bridge-source");
        let workspace = scratch("bridge-workspace");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("plugin.json"), r#"{"name":"aiks-bridge"}"#).unwrap();
        fs::write(source.join("index.js"), "v1").unwrap();
        fs::write(source.join("index.css"), "css-v1").unwrap();

        install_bridge_plugin(&source, &workspace).unwrap();
        let target = workspace.join("data/plugins/aiks-bridge");
        assert_eq!(fs::read_to_string(target.join("index.js")).unwrap(), "v1");
        assert_eq!(
            fs::read_to_string(target.join("plugin.json")).unwrap(),
            r#"{"name":"aiks-bridge"}"#
        );

        fs::write(source.join("index.js"), "v2").unwrap();
        install_bridge_plugin(&source, &workspace).unwrap();
        assert_eq!(fs::read_to_string(target.join("index.js")).unwrap(), "v2");

        fs::remove_dir_all(source).ok();
        fs::remove_dir_all(workspace).ok();
    }

    #[test]
    fn refuses_missing_or_incomplete_bridge_sources() {
        let source = scratch("bridge-incomplete");
        let workspace = scratch("bridge-workspace");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("plugin.json"), "{}").unwrap();

        assert!(install_bridge_plugin(&source, &workspace).is_err());
        assert!(!workspace.join("data/plugins/aiks-bridge").exists());

        fs::remove_dir_all(source).ok();
        fs::remove_dir_all(workspace).ok();
    }

    #[test]
    fn embedded_bridge_bazaar_config_preserves_existing_settings() {
        let mut current = Map::new();
        current.insert("bazaarURL".to_string(), json!("https://example.invalid"));
        current.insert("trust".to_string(), json!(false));
        current.insert("petalDisabled".to_string(), json!(true));

        let configured = bridge_bazaar_config(&current);

        assert_eq!(configured.get("trust"), Some(&json!(true)));
        assert_eq!(configured.get("petalDisabled"), Some(&json!(false)));
        assert_eq!(
            configured.get("bazaarURL"),
            Some(&json!("https://example.invalid"))
        );
    }

    #[test]
    fn siyuan_api_error_is_not_silently_accepted() {
        let error = ensure_api_success(&json!({"code": -1, "msg": "rejected"}), "test")
            .expect_err("non-zero SiYuan API response must fail");
        assert!(error.to_string().contains("rejected"));
        assert!(ensure_api_success(&json!({"code": 0}), "test").is_ok());
    }
}
