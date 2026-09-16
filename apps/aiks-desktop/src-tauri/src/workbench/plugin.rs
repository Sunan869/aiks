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
