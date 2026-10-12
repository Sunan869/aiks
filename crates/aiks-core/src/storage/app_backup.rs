//! Offline full-backup assembly foundation: one SQLite-consistent state snapshot
//! plus explicitly selected, already quiesced SiYuan workspace files.
//! Never discovers or mutates Provider sources; no live restore API.
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use anyhow::{bail, Context, Result};
use uuid::Uuid;

use super::backup::{create_sqlite_backup, load_and_verify_backup, save_backup_manifest};
use super::backup_bundle::{
    create_file_bundle, restore_file_bundle_to_new_directory, verify_file_bundle, BundleManifest,
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
    if destination_parent.starts_with(&root) || root.starts_with(&destination_parent.join(
        destination.file_name().context("backup destination name missing")?,
    )) {
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
        assert!(create_offline_backup_from_quiesced_workspace(&db, &workspace, &enclosing).is_err());
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
