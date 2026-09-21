//! Provider configuration edits preserve unknown/unexposed TOML values.
use super::catalog::{descriptors, EXTERNAL_SOURCES};
use crate::{model::SourceKind, Config};
use anyhow::{ensure, Context, Result};
use std::io::Write;
use std::path::Path;
use toml::Value;

pub fn patch_provider_toml(
    existing: &str,
    key: &str,
    enabled: bool,
    paths: &[String],
) -> Result<String> {
    let source = SourceKind::from_str(key).context("Unknown provider key")?;
    ensure!(key == source.as_str(), "Use the stable provider key");
    ensure!(
        !matches!(
            source,
            SourceKind::ChatgptShare
                | SourceKind::ClaudeShare
                | SourceKind::GeminiShare
                | SourceKind::DeepseekShare
                | SourceKind::DoubaoShare
                | SourceKind::KimiShare
                | SourceKind::YuanbaoShare
                | SourceKind::QwenShare
        ),
        "Managed share sources have no local provider configuration"
    );
    ensure!(paths.len() <= 32, "At most 32 provider roots are allowed");
    ensure!(
        paths
            .iter()
            .all(|p| p.len() <= 32768 && !p.contains(['\0', '\n', '\r'])),
        "Invalid provider path"
    );
    let paths: Vec<String> = paths
        .iter()
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty())
        .collect();
    let external = EXTERNAL_SOURCES.contains(&source);
    ensure!(
        external || paths.len() <= 1,
        "This provider accepts one data root"
    );
    let mut value: Value = toml::from_str(existing)
        .map_err(|_| anyhow::anyhow!("Invalid existing TOML; configuration left unchanged"))?;
    let defaults = descriptors(&Config::default(), &[]);
    let descriptor = defaults
        .iter()
        .find(|d| d.key == key)
        .context("Provider descriptor missing")?;
    let providers = value
        .as_table_mut()
        .context("Configuration must be a TOML table")?
        .entry("providers")
        .or_insert_with(|| Value::Table(Default::default()));
    let provider = providers
        .as_table_mut()
        .context("providers must be a table")?
        .entry(descriptor.config_key)
        .or_insert_with(|| Value::Table(Default::default()));
    let table = provider
        .as_table_mut()
        .context("Provider configuration must be a table")?;
    table.insert("enabled".into(), Value::Boolean(enabled));
    table.insert(
        "path".into(),
        Value::String(if paths.len() == 1 {
            paths[0].clone()
        } else {
            String::new()
        }),
    );
    if external {
        table.insert(
            "paths".into(),
            Value::Array(if paths.len() > 1 {
                paths.into_iter().map(Value::String).collect()
            } else {
                Vec::new()
            }),
        );
    }
    let encoded = toml::to_string_pretty(&value)?;
    let _: Config = toml::from_str(&encoded).map_err(|_| {
        anyhow::anyhow!("Configuration validation failed; original file left unchanged")
    })?;
    Ok(encoded)
}

/// Callers serialize concurrent in-process settings writes with their common lock.
/// A changed source file aborts the optimistic update. Rename failure never deletes
/// the original destination (including Windows sharing/permission failures).
pub fn save_provider_settings(
    path: &Path,
    key: &str,
    enabled: bool,
    roots: &[String],
) -> Result<()> {
    let before = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let text = std::str::from_utf8(before.as_deref().unwrap_or_default())
        .context("Configuration is not UTF-8")?;
    let after = patch_provider_toml(text, key, enabled, roots)?;
    let parent = path.parent().context("Configuration parent is missing")?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".aiks-provider-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        if let Ok(metadata) = std::fs::metadata(path) {
            file.set_permissions(metadata.permissions())?;
        }
        file.write_all(after.as_bytes())?;
        file.sync_all()?;
        drop(file);
        let current = match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        ensure!(
            current == before,
            "Configuration changed during save; reload and retry"
        );
        std::fs::rename(&temporary, path)
            .context("Could not replace configuration; original file was retained")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
