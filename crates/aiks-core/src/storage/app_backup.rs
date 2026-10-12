//! Offline full-backup assembly foundation: one SQLite-consistent state snapshot
//! plus explicitly selected, already quiesced SiYuan workspace files.
//! Never discovers or mutates Provider sources; no live restore API.
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use uuid::Uuid;

use super::backup::{create_sqlite_backup, load_and_verify_backup, save_backup_manifest};
use super::backup_bundle::{
    create_file_bundle, restore_file_bundle_to_new_directory, verify_file_bundle, BundleManifest,
};
use super::StateDb;

/// The caller is responsible for stopping SiYuan writes while enumerating and
/// backing up these files. Configuration is intentionally excluded until an
/// explicit secret-filtering contract is available.
pub fn create_offline_backup(
    db: &StateDb,
    quiesced_workspace_files: &[(&Path, &Path)],
    destination: &Path,
) -> Result<BundleManifest> {
    if fs::symlink_metadata(destination).is_ok() {
        bail!("backup destination exists");
    }
    let parent = destination.parent().context("backup destination has no parent")?;
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
            (sqlite_manifest.as_path(), PathBuf::from("sqlite/state-manifest.json")),
        ];
        for (source, relative) in quiesced_workspace_files {
            sources.push((*source, PathBuf::from("siyuan").join(relative)));
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

/// Restore into a new, isolated directory only, then verify the embedded
/// SQLite snapshot against its own format/schema/checksum manifest.
pub fn restore_offline_backup_to_new_directory(
    bundle: &Path,
    destination: &Path,
) -> Result<BundleManifest> {
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

pub fn verify_offline_backup(bundle: &Path) -> Result<BundleManifest> {
    let manifest = verify_file_bundle(bundle)?;
    load_and_verify_backup(
        &bundle.join("sqlite/state.sqlite"),
        &bundle.join("sqlite/state-manifest.json"),
    )?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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
