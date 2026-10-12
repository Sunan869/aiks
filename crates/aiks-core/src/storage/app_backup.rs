//! Offline full-backup assembly foundation: one SQLite-consistent state snapshot
//! plus explicitly selected, already quiesced SiYuan workspace files.
//! Never discovers or mutates Provider sources; no live restore API.
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use uuid::Uuid;

use super::backup::{create_sqlite_backup, load_and_verify_backup, save_backup_manifest};
use super::backup_bundle::{
    create_file_bundle, restore_file_bundle_to_new_directory, validate_restore_destination,
    verify_file_bundle, BundleManifest,
};
use super::StateDb;

/// Enumerate only a caller-provided, quiesced SiYuan workspace root.
/// Reject links, unreadable entries and files escaping that root; nothing is
/// read from Provider sources and no directory is modified.
pub fn enumerate_quiesced_siyuan_workspace(root: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
    let root_metadata = fs::symlink_metadata(root)?;
    if !root_metadata.file_type().is_dir() {
        bail!("SiYuan workspace root must be a real directory");
    }
    let canonical_root = fs::canonicalize(root)?;
    let mut files = Vec::new();
    let mut total_bytes: u64 = 0;
    for item in WalkDir::new(root).follow_links(false) {
        let item = item.context("scan quiesced SiYuan workspace")?;
        let file_type = item.file_type();
        if file_type.is_symlink() {
            bail!("SiYuan workspace contains a symbolic link");
        }
        if file_type.is_dir() {
            continue;
        }
        if !file_type.is_file() {
            bail!("SiYuan workspace contains an unsupported file type");
        }
        let relative = item
            .path()
            .strip_prefix(root)
            .context("SiYuan workspace path outside selected root")?;
        let actual = fs::canonicalize(item.path())?;
        if !actual.starts_with(&canonical_root) {
            bail!("SiYuan workspace file escapes selected root");
        }
        let size = item.metadata()?.len();
        total_bytes = total_bytes
            .checked_add(size)
            .context("SiYuan workspace size overflow")?;
        if files.len() >= 100_000 || total_bytes > 100 * 1024 * 1024 * 1024 {
            bail!("SiYuan workspace exceeds backup resource budget");
        }
        files.push((item.path().to_path_buf(), relative.to_path_buf()));
    }
    files.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(files)
}

/// Back up one explicitly selected SiYuan workspace that the caller has
/// already stopped. The destination must be outside the workspace, otherwise
/// a backup could recursively include its own staging or output.
pub fn create_offline_backup_from_quiesced_workspace(
    db: &StateDb,
    workspace_root: &Path,
    destination: &Path,
) -> Result<BundleManifest> {
    let root = fs::canonicalize(workspace_root)?;
    let root_meta = fs::symlink_metadata(workspace_root)?;
    if !root_meta.file_type().is_dir() {
        bail!("SiYuan workspace root must be a real directory");
    }
    let parent = destination
        .parent()
        .context("backup destination has no parent")?;
    let destination_parent = fs::canonicalize(parent)?;
    if destination_parent.starts_with(&root)
        || root.starts_with(
            destination_parent.join(
                destination
                    .file_name()
                    .context("backup destination name missing")?,
            ),
        )
    {
        bail!("backup output must be independent of the SiYuan workspace");
    }
    let selected = enumerate_quiesced_siyuan_workspace(workspace_root)?;
    let borrowed: Vec<(&Path, &Path)> = selected
        .iter()
        .map(|(source, relative)| (source.as_path(), relative.as_path()))
        .collect();
    create_offline_backup(db, &borrowed, destination)
}

/// The caller is responsible for stopping SiYuan writes while enumerating and
/// backing up these files. Configuration is intentionally excluded until an
/// explicit secret-filtering contract is available.
pub fn create_offline_backup(
    db: &StateDb,
    quiesced_workspace_files: &[(&Path, &Path)],
    destination: &Path,
) -> Result<BundleManifest> {
    create_offline_backup_with_settings(db, quiesced_workspace_files, None, destination)
}

/// Optional non-sensitive settings are reconstructed from a strict allowlist.
/// The full original configuration is never placed in the archive.
pub fn create_offline_backup_with_settings(
    db: &StateDb,
    quiesced_workspace_files: &[(&Path, &Path)],
    settings: Option<&toml::Value>,
    destination: &Path,
) -> Result<BundleManifest> {
    let cleaned = settings.map(sanitized_backup_settings).transpose()?;
    if fs::symlink_metadata(destination).is_ok() {
        bail!("backup destination exists");
    }
    let parent = destination
        .parent()
        .context("backup destination has no parent")?;
    if !parent.is_dir() {
        bail!("backup parent does not exist");
    }
    let staging = parent.join(format!(".aiks-backup-{}", Uuid::new_v4()));
    fs::create_dir(&staging)?;
    let outcome = (|| -> Result<BundleManifest> {
        let snapshot = staging.join("state.sqlite");
        let sqlite_manifest = staging.join("state-manifest.json");
        let manifest = create_sqlite_backup(db, &snapshot)?;
        save_backup_manifest(&sqlite_manifest, &manifest)?;
        load_and_verify_backup(&snapshot, &sqlite_manifest)?;
        let mut sources: Vec<(&Path, PathBuf)> = vec![
            (snapshot.as_path(), PathBuf::from("sqlite/state.sqlite")),
            (
                sqlite_manifest.as_path(),
                PathBuf::from("sqlite/state-manifest.json"),
            ),
        ];
        for (source, relative) in quiesced_workspace_files {
            sources.push((*source, PathBuf::from("siyuan").join(relative)));
        }
        let settings_file = staging.join("settings.toml");
        if let Some(settings) = cleaned.as_ref() {
            fs::write(&settings_file, toml::to_string(settings)?)?;
            sources.push((
                settings_file.as_path(),
                PathBuf::from("config/settings.toml"),
            ));
        }
        let items: Vec<(&Path, &Path)> = sources
            .iter()
            .map(|(source, relative)| (*source, relative.as_path()))
            .collect();
        create_file_bundle(&items, destination)
    })();
    let _ = fs::remove_dir_all(&staging);
    outcome
}

/// Verified counts for display before explicit user confirmation. No data
/// changes, free-space check or automatic SiYuan shutdown are performed here.
#[derive(Debug, Clone, Serialize)]
pub struct OfflineRestorePreview {
    pub file_count: usize,
    pub total_bytes: u64,
    pub siyuan_files: usize,
    pub requires_siyuan_shutdown: bool,
}

pub fn preflight_offline_restore(
    bundle: &Path,
    destination: &Path,
) -> Result<OfflineRestorePreview> {
    let manifest = verify_offline_backup(bundle)?;
    validate_restore_destination(bundle, destination)?;
    let total_bytes = manifest.entries.iter().try_fold(0u64, |sum, entry| {
        sum.checked_add(entry.bytes)
            .context("offline backup byte count overflow")
    })?;
    Ok(OfflineRestorePreview {
        file_count: manifest.entries.len(),
        total_bytes,
        siyuan_files: manifest
            .entries
            .iter()
            .filter(|entry| entry.relative_path.starts_with("siyuan/"))
            .count(),
        requires_siyuan_shutdown: true,
    })
}

/// Enforce a conservative restore-space budget using a free-byte figure
/// measured by the calling platform immediately before confirmation.
/// This does not itself query disk capacity or authorize a live restore.
pub fn preflight_offline_restore_with_capacity(
    bundle: &Path,
    destination: &Path,
    available_bytes: u64,
) -> Result<OfflineRestorePreview> {
    let preview = preflight_offline_restore(bundle, destination)?;
    let reserve = (preview.total_bytes / 5).max(64 * 1024 * 1024);
    let required = preview
        .total_bytes
        .checked_add(reserve)
        .context("restore space budget overflow")?;
    if available_bytes < required {
        bail!("insufficient available space for isolated restore");
    }
    Ok(preview)
}

/// A capacity-checked, explicitly confirmed entry point for callers that
/// have already measured available space and quiesced the destination.
pub fn restore_offline_backup_with_capacity_and_confirmation(
    bundle: &Path,
    destination: &Path,
    confirmed_destination: &Path,
    available_bytes: u64,
) -> Result<BundleManifest> {
    if destination != confirmed_destination {
        bail!("restore destination differs from the confirmed destination");
    }
    preflight_offline_restore_with_capacity(bundle, destination, available_bytes)?;
    restore_offline_backup_to_new_directory(bundle, destination)
}

/// Restore into a new, isolated directory only, after a read-only preflight.
/// Never apply the snapshot automatically to the running AIKS workspace.
/// A restore command must require an explicit confirmation matching the
/// destination chosen during the validated preview. This does not authorize
/// overwriting a running AIKS database or SiYuan workspace.
pub fn restore_offline_backup_after_confirmation(
    bundle: &Path,
    destination: &Path,
    confirmed_destination: &Path,
) -> Result<BundleManifest> {
    if destination != confirmed_destination {
        bail!("restore destination differs from the confirmed destination");
    }
    preflight_offline_restore(bundle, destination)?;
    restore_offline_backup_to_new_directory(bundle, destination)
}

pub fn restore_offline_backup_to_new_directory(
    bundle: &Path,
    destination: &Path,
) -> Result<BundleManifest> {
    preflight_offline_restore(bundle, destination)?;
    let manifest = restore_file_bundle_to_new_directory(bundle, destination)?;
    let result = load_and_verify_backup(
        &destination.join("sqlite/state.sqlite"),
        &destination.join("sqlite/state-manifest.json"),
    );
    if let Err(error) = result {
        let _ = fs::remove_dir_all(destination);
        return Err(error).context("restored SQLite snapshot invalid");
    }
    Ok(manifest)
}

fn validate_offline_manifest(manifest: &BundleManifest) -> Result<()> {
    for required in ["sqlite/state.sqlite", "sqlite/state-manifest.json"] {
        if !manifest
            .entries
            .iter()
            .any(|entry| entry.relative_path == required)
        {
            bail!("offline backup missing required manifest entry: {required}");
        }
    }
    for entry in &manifest.entries {
        if entry.relative_path != "sqlite/state.sqlite"
            && entry.relative_path != "sqlite/state-manifest.json"
            && entry.relative_path != "config/settings.toml"
            && !entry.relative_path.starts_with("siyuan/")
        {
            bail!("offline backup contains an unapproved file category");
        }
    }
    Ok(())
}

/// Validate optional settings even when a forged manifest correctly hashes
/// the file. Never import credential-bearing or unapproved keys.
fn verify_sanitized_settings(bundle: &Path, manifest: &BundleManifest) -> Result<()> {
    if manifest
        .entries
        .iter()
        .any(|entry| entry.relative_path == "config/settings.toml")
    {
        let content = fs::read_to_string(bundle.join("config/settings.toml"))?;
        let settings: toml::Value = toml::from_str(&content)?;
        let cleaned = sanitized_backup_settings(&settings)?;
        if settings != cleaned {
            bail!("offline backup contains unapproved settings");
        }
    }
    Ok(())
}

pub fn verify_offline_backup(bundle: &Path) -> Result<BundleManifest> {
    let manifest = verify_file_bundle(bundle)?;
    validate_offline_manifest(&manifest)?;
    verify_sanitized_settings(bundle, &manifest)?;
    load_and_verify_backup(
        &bundle.join("sqlite/state.sqlite"),
        &bundle.join("sqlite/state-manifest.json"),
    )?;
    Ok(manifest)
}

/// Build a secret-free, portable configuration summary for an offline
/// backup. This is a strict allowlist, never a copy of user configuration.
pub fn sanitized_backup_settings(config: &toml::Value) -> Result<toml::Value> {
    let table = config
        .as_table()
        .context("AIKS configuration must be a TOML table")?;
    let mut filtered = toml::map::Map::new();
    for name in ["language", "theme"] {
        if let Some(value) = table.get(name) {
            let Some(text) = value.as_str() else {
                bail!("backup setting {name} must be a string");
            };
            if text.chars().count() > 64 || text.chars().any(|c| c.is_control()) {
                bail!("invalid backup setting {name}");
            }
            filtered.insert(name.to_owned(), toml::Value::String(text.to_owned()));
        }
    }
    Ok(toml::Value::Table(filtered))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn sanitized_settings_exclude_model_credentials_and_endpoints() {
        let config: toml::Value = toml::from_str(
            r#"
language = "zh-CN"
theme = "dark"
api_key = "secret"
base_url = "http://private-endpoint"
[ai]
token = "should-not-leak"
"#,
        )
        .unwrap();
        let result = sanitized_backup_settings(&config).unwrap();
        let encoded = toml::to_string(&result).unwrap();
        assert!(encoded.contains("zh-CN"));
        assert!(encoded.contains("dark"));
        assert!(!encoded.contains("secret"));
        assert!(!encoded.contains("private-endpoint"));
        assert!(!encoded.contains("token"));
        assert!(!encoded.contains("base_url"));
    }

    #[test]
    fn backup_with_settings_round_trip_excludes_credentials() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let raw: toml::Value = toml::from_str(
            "language = \"zh-CN\"\ntheme = \"dark\"\napi_key = \"sensitive\"\n[ai]\ntoken = \"never-export\"\n",
        )
        .unwrap();
        let bundle = dir.path().join("backup");
        create_offline_backup_with_settings(&db, &[], Some(&raw), &bundle).unwrap();
        verify_offline_backup(&bundle).unwrap();
        let exported = fs::read_to_string(bundle.join("config/settings.toml")).unwrap();
        assert!(exported.contains("zh-CN"));
        assert!(!exported.contains("sensitive"));
        assert!(!exported.contains("never-export"));
        let restored = dir.path().join("restored");
        restore_offline_backup_to_new_directory(&bundle, &restored).unwrap();
        assert_eq!(
            fs::read_to_string(restored.join("config/settings.toml")).unwrap(),
            exported
        );
    }

    #[test]
    fn forged_backup_with_hashed_credentials_fails_preflight() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let config: toml::Value = toml::from_str("language = \"zh-CN\"").unwrap();
        let bundle = dir.path().join("backup");
        create_offline_backup_with_settings(&db, &[], Some(&config), &bundle).unwrap();
        let config_path = bundle.join("config/settings.toml");
        fs::write(
            &config_path,
            "language = \"zh-CN\"\napi_key = \"injected\"\n",
        )
        .unwrap();
        let manifest_path = bundle.join("bundle-manifest.json");
        let mut manifest: BundleManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        let tampered = fs::read(&config_path).unwrap();
        let entry = manifest
            .entries
            .iter_mut()
            .find(|entry| entry.relative_path == "config/settings.toml")
            .unwrap();
        entry.bytes = tampered.len() as u64;
        use sha2::{Digest, Sha256};
        entry.sha256 = hex::encode(Sha256::digest(&tampered));
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        assert!(verify_offline_backup(&bundle).is_err());
        let destination = dir.path().join("restored");
        assert!(preflight_offline_restore(&bundle, &destination).is_err());
        assert!(!destination.exists());
    }

    #[test]
    fn settings_backup_rejects_invalid_allowlisted_values_without_output() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let invalid: toml::Value = toml::from_str("language = 123").unwrap();
        let dest = dir.path().join("backup");
        assert!(create_offline_backup_with_settings(&db, &[], Some(&invalid), &dest).is_err());
        assert!(!dest.exists());
        let too_long = "a".repeat(65);
        let invalid: toml::Value = toml::from_str(&format!("theme = \"{too_long}\"")).unwrap();
        assert!(create_offline_backup_with_settings(&db, &[], Some(&invalid), &dest).is_err());
        assert!(!dest.exists());
    }

    #[test]
    fn confirmation_never_restores_to_a_different_destination() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let bundle = dir.path().join("backup");
        create_offline_backup(&db, &[], &bundle).unwrap();
        let destination = dir.path().join("restore");
        let other = dir.path().join("other");
        assert!(restore_offline_backup_after_confirmation(&bundle, &destination, &other).is_err());
        assert!(!destination.exists());
        assert!(!other.exists());
        restore_offline_backup_after_confirmation(&bundle, &destination, &destination).unwrap();
        assert!(destination.join("sqlite/state.sqlite").is_file());
        assert!(
            restore_offline_backup_after_confirmation(&bundle, &destination, &destination).is_err()
        );
    }

    #[test]
    fn restore_capacity_gate_rejects_insufficient_space_without_side_effects() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let bundle = dir.path().join("bundle");
        create_offline_backup(&db, &[], &bundle).unwrap();
        let target = dir.path().join("restored");
        let preview = preflight_offline_restore(&bundle, &target).unwrap();
        assert!(preview.total_bytes > 0);
        assert!(preflight_offline_restore_with_capacity(&bundle, &target, 0).is_err());
        assert!(!target.exists());
        assert!(
            restore_offline_backup_with_capacity_and_confirmation(
                &bundle,
                &target,
                &target,
                0,
            )
            .is_err()
        );
        assert!(!target.exists());
        restore_offline_backup_with_capacity_and_confirmation(
            &bundle,
            &target,
            &target,
            u64::MAX,
        )
        .unwrap();
        assert!(target.join("sqlite/state.sqlite").is_file());
    }

    #[test]
    fn coordinated_snapshot_and_quiesced_workspace_restore() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let workspace = dir.path().join("note.md");
        fs::write(&workspace, b"quiesced SiYuan document").unwrap();
        let target = dir.path().join("backup");
        let items = [(workspace.as_path(), Path::new("notes/note.md"))];
        let manifest = create_offline_backup(&db, &items, &target).unwrap();
        assert_eq!(manifest.entries.len(), 3);
        verify_offline_backup(&target).unwrap();
        let restore = dir.path().join("restore");
        restore_offline_backup_to_new_directory(&target, &restore).unwrap();
        assert_eq!(
            fs::read(restore.join("siyuan/notes/note.md")).unwrap(),
            b"quiesced SiYuan document"
        );
        assert!(restore_offline_backup_to_new_directory(&target, &restore).is_err());
    }

    #[test]
    fn workspace_enumeration_includes_nested_documents_and_assets() {
        let dir = tempdir().unwrap();
        let workspace = dir.path().join("quiesced-workspace");
        fs::create_dir_all(workspace.join("data/assets")).unwrap();
        fs::write(workspace.join("data/note.sy"), b"document").unwrap();
        fs::write(workspace.join("data/assets/photo.png"), b"asset").unwrap();
        let selected = enumerate_quiesced_siyuan_workspace(&workspace).unwrap();
        assert_eq!(selected.len(), 2);
        assert!(selected
            .iter()
            .any(|(_, relative)| relative == Path::new("data/note.sy")));
        assert!(selected
            .iter()
            .any(|(_, relative)| relative == Path::new("data/assets/photo.png")));
    }

    #[test]
    fn whole_workspace_backup_captures_documents_and_assets_and_restores() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let workspace = dir.path().join("siyuan-workspace");
        fs::create_dir_all(workspace.join("data/assets")).unwrap();
        fs::write(workspace.join("data/note.sy"), b"quiesced note").unwrap();
        fs::write(workspace.join("data/assets/image.png"), b"asset bytes").unwrap();
        let bundle = dir.path().join("bundle");
        let manifest =
            create_offline_backup_from_quiesced_workspace(&db, &workspace, &bundle).unwrap();
        assert_eq!(manifest.entries.len(), 4);
        let restored = dir.path().join("restored");
        restore_offline_backup_to_new_directory(&bundle, &restored).unwrap();
        assert_eq!(
            fs::read(restored.join("siyuan/data/note.sy")).unwrap(),
            b"quiesced note"
        );
        assert_eq!(
            fs::read(restored.join("siyuan/data/assets/image.png")).unwrap(),
            b"asset bytes"
        );
        assert_eq!(
            fs::read(workspace.join("data/note.sy")).unwrap(),
            b"quiesced note"
        );
    }

    #[test]
    fn whole_workspace_backup_refuses_nested_output_before_creating_anything() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let workspace = dir.path().join("siyuan");
        fs::create_dir(&workspace).unwrap();
        fs::write(workspace.join("note.sy"), b"unchanged").unwrap();
        let nested = workspace.join("backup");
        assert!(create_offline_backup_from_quiesced_workspace(&db, &workspace, &nested).is_err());
        assert!(!nested.exists());
        let enclosing = dir.path().join("siyuan");
        assert!(
            create_offline_backup_from_quiesced_workspace(&db, &workspace, &enclosing).is_err()
        );
        assert_eq!(fs::read(workspace.join("note.sy")).unwrap(), b"unchanged");
    }

    #[cfg(unix)]
    #[test]
    fn workspace_backup_rejects_symlinked_output_parent_inside_workspace() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let workspace = dir.path().join("siyuan");
        fs::create_dir(&workspace).unwrap();
        fs::write(workspace.join("note.sy"), b"unchanged").unwrap();
        let alias = dir.path().join("alias");
        symlink(&workspace, &alias).unwrap();
        let destination = alias.join("nested-backup");
        assert!(
            create_offline_backup_from_quiesced_workspace(&db, &workspace, &destination).is_err()
        );
        assert!(!workspace.join("nested-backup").exists());
        assert_eq!(fs::read(workspace.join("note.sy")).unwrap(), b"unchanged");
    }

    #[cfg(unix)]
    #[test]
    fn workspace_enumeration_rejects_symbolic_links() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let external = dir.path().join("external");
        fs::write(&external, b"outside").unwrap();
        symlink(&external, workspace.join("linked.sy")).unwrap();
        assert!(enumerate_quiesced_siyuan_workspace(&workspace).is_err());
        assert_eq!(fs::read(&external).unwrap(), b"outside");
    }

    #[test]
    fn unsafe_workspace_path_never_leaves_backup_destination() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let workspace = dir.path().join("note.md");
        fs::write(&workspace, b"immutable").unwrap();
        let target = dir.path().join("backup");
        let items = [(workspace.as_path(), Path::new("../escape.md"))];
        assert!(create_offline_backup(&db, &items, &target).is_err());
        assert!(!target.exists());
        assert_eq!(fs::read(&workspace).unwrap(), b"immutable");
    }
}
