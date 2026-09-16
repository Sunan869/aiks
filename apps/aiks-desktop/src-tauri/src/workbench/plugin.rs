use std::fs;
use std::path::{Path, PathBuf};

const REQUIRED_FILES: [&str; 3] = ["plugin.json", "index.js", "index.css"];

pub fn install_bridge_plugin(source: &Path, workspace: &Path) -> anyhow::Result<PathBuf> {
    for name in REQUIRED_FILES {
        let path = source.join(name);
        if !path.is_file() {
            anyhow::bail!("AIKS bridge plugin resource is missing: {}", path.display());
        }
    }

    let target = workspace.join("data").join("plugins").join("aiks-bridge");
    fs::create_dir_all(&target)?;
    copy_tree(source, &target)?;
    Ok(target)
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

    use super::install_bridge_plugin;

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
}
