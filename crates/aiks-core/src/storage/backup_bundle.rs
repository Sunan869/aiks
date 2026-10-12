//! Detached, explicit allow-list directory bundle for S1.3.
//! Does not read provider directories, follow links, or touch a running destination.
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleEntry {
    pub relative_path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    pub format_version: u32,
    pub entries: Vec<BundleEntry>,
}

const MAX_BUNDLE_FILES: usize = 100_000;
const MAX_BUNDLE_BYTES: u64 = 100 * 1024 * 1024 * 1024;

fn safe_relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("bundle path must contain only normal relative components");
    }
    Ok(())
}

fn regular_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        bail!("bundle source must be a regular file, not a link or directory");
    }
    Ok(())
}

fn digest(path: &Path) -> Result<(u64, String)> {
    regular_file(path)?;
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes = bytes
            .checked_add(n as u64)
            .context("bundle size overflow")?;
        hash.update(&buffer[..n]);
    }
    Ok((bytes, hex::encode(hash.finalize())))
}

/// Copy an explicitly supplied list of detached files into a *new* bundle
/// directory. The caller must quiesce mutable workspace files first.
pub fn create_file_bundle(
    sources: &[(&Path, &Path)],
    destination: &Path,
) -> Result<BundleManifest> {
    if sources.len() > MAX_BUNDLE_FILES {
        bail!("too many bundle files");
    }
    if fs::symlink_metadata(destination).is_ok() {
        bail!("bundle destination exists");
    }
    fs::create_dir(destination).context("create exclusive bundle directory")?;
    let result = (|| -> Result<BundleManifest> {
        let mut entries = Vec::with_capacity(sources.len());
        let mut total = 0u64;
        let mut seen = std::collections::HashSet::new();
        for (source, relative) in sources {
            safe_relative(relative)?;
            if *relative == Path::new("bundle-manifest.json") {
                bail!("bundle destination is reserved for the manifest");
            }
            if !seen.insert((*relative).to_path_buf()) {
                bail!("duplicate bundle destination");
            }
            regular_file(source)?;
            // Reject symlinked parent directories, including intermediate
            // workspace paths. Caller must supply trusted rooted inputs.
            for ancestor in source.ancestors().skip(1) {
                if ancestor.as_os_str().is_empty() {
                    break;
                }
                if fs::symlink_metadata(ancestor).is_ok_and(|meta| meta.file_type().is_symlink()) {
                    bail!("bundle source traverses a symbolic link");
                }
            }
            let (size, before) = digest(source)?;
            total = total.checked_add(size).context("bundle size overflow")?;
            if total > MAX_BUNDLE_BYTES {
                bail!("bundle size limit exceeded");
            }
            let target = destination.join(relative);
            let parent = target.parent().context("invalid bundle destination")?;
            fs::create_dir_all(parent)?;
            let mut reader = fs::File::open(source)?;
            let mut writer = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            std::io::copy(&mut reader, &mut writer)?;
            writer.sync_all()?;
            let (actual_size, after) = digest(&target)?;
            if size != actual_size || before != after {
                bail!("bundle input changed while copying");
            }
            entries.push(BundleEntry {
                relative_path: relative.to_string_lossy().into_owned(),
                bytes: size,
                sha256: after,
            });
        }
        let manifest = BundleManifest {
            format_version: 1,
            entries,
        };
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination.join("bundle-manifest.json"))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(manifest)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

/// Verify a detached bundle without writing anything to the destination.
pub fn verify_file_bundle(destination: &Path) -> Result<BundleManifest> {
    let manifest_path = destination.join("bundle-manifest.json");
    regular_file(&manifest_path)?;
    let manifest: BundleManifest = serde_json::from_reader(fs::File::open(&manifest_path)?)?;
    if manifest.format_version != 1 || manifest.entries.len() > MAX_BUNDLE_FILES {
        bail!("unsupported bundle manifest");
    }
    let mut seen = std::collections::HashSet::new();
    let mut total = 0u64;
    for entry in &manifest.entries {
        let relative = Path::new(&entry.relative_path);
        safe_relative(relative)?;
        if relative == Path::new("bundle-manifest.json")
            || !seen.insert(entry.relative_path.as_str())
        {
            bail!("duplicate or reserved bundle path");
        }
        let target = destination.join(relative);
        for parent in target.ancestors().skip(1) {
            if parent == destination {
                break;
            }
            if fs::symlink_metadata(parent).is_ok_and(|meta| meta.file_type().is_symlink()) {
                bail!("bundle traversal through symlink");
            }
        }
        let (size, hash) = digest(&target)?;
        total = total.checked_add(size).context("bundle size overflow")?;
        if total > MAX_BUNDLE_BYTES || size != entry.bytes || hash != entry.sha256 {
            bail!("bundle integrity mismatch");
        }
    }
    Ok(manifest)
}

/// Restore a validated, detached archive into a brand-new directory only.
/// Never replace live AIKS or SiYuan files. A failed copy removes only the
/// directory exclusively created by this call.
pub fn restore_file_bundle_to_new_directory(
    bundle: &Path,
    destination: &Path,
) -> Result<BundleManifest> {
    let manifest = verify_file_bundle(bundle)?;
    // A destination nested in the source archive could be mistaken for
    // mutable source data. Canonicalize the archive before permitting writes.
    let bundle_root = fs::canonicalize(bundle)?;
    let parent = destination.parent().context("restore destination has no parent")?;
    let parent_root = fs::canonicalize(parent)?;
    if parent_root.starts_with(&bundle_root) {
        bail!("restore destination must be outside the source bundle");
    }
    if fs::symlink_metadata(destination).is_ok() {
        bail!("restore destination already exists");
    }
    fs::create_dir(destination).context("create exclusive restore directory")?;
    let result = (|| -> Result<()> {
        for entry in &manifest.entries {
            let relative = Path::new(&entry.relative_path);
            safe_relative(relative)?;
            let source = bundle.join(relative);
            regular_file(&source)?;
            let target = destination.join(relative);
            let target_parent = target.parent().context("invalid restored file path")?;
            fs::create_dir_all(target_parent)?;
            let mut input = fs::File::open(&source)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
            let (size, hash) = digest(&target)?;
            if size != entry.bytes || hash != entry.sha256 {
                bail!("restored file integrity mismatch");
            }
            // Revalidate source to catch a concurrent modification while copying.
            let (source_size, source_hash) = digest(&source)?;
            if source_size != entry.bytes || source_hash != entry.sha256 {
                bail!("bundle source changed during restore");
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(destination);
        return Err(error);
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn bundle_round_trip_and_tamper_rejection() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("state.sqlite");
        fs::write(&source, b"detached-snapshot").unwrap();
        let target = dir.path().join("bundle");
        let items = [(source.as_path(), Path::new("sqlite/state.sqlite"))];
        create_file_bundle(&items, &target).unwrap();
        assert_eq!(verify_file_bundle(&target).unwrap().entries.len(), 1);
        fs::write(target.join("sqlite/state.sqlite"), b"modified").unwrap();
        assert!(verify_file_bundle(&target).is_err());
    }

    #[test]
    fn duplicate_and_traversal_destinations_leave_no_bundle() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("sample");
        fs::write(&source, b"ok").unwrap();
        let duplicate = dir.path().join("duplicate");
        let items = [
            (source.as_path(), Path::new("state/file")),
            (source.as_path(), Path::new("state/file")),
        ];
        assert!(create_file_bundle(&items, &duplicate).is_err());
        assert!(!duplicate.exists());
        let traversal = dir.path().join("traversal");
        let items = [(source.as_path(), Path::new("../escape"))];
        assert!(create_file_bundle(&items, &traversal).is_err());
        assert!(!traversal.exists());
    }

    #[test]
    fn reserved_manifest_filename_does_not_destroy_preexisting_files() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("data");
        fs::write(&source, b"preserve original").unwrap();
        let bundle = dir.path().join("bundle");
        assert!(create_file_bundle(
            &[(source.as_path(), Path::new("bundle-manifest.json"))],
            &bundle
        )
        .is_err());
        assert!(!bundle.exists());
        assert_eq!(fs::read(&source).unwrap(), b"preserve original");
    }

    #[test]
    fn detached_restore_round_trip_and_existing_directory_are_safe() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("workspace-source");
        fs::write(&source, b"offline knowledge snapshot").unwrap();
        let bundle = dir.path().join("bundle");
        create_file_bundle(
            &[(source.as_path(), Path::new("workspace/data.md"))],
            &bundle,
        )
        .unwrap();
        let restored = dir.path().join("restored");
        let manifest = restore_file_bundle_to_new_directory(&bundle, &restored).unwrap();
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(
            fs::read(restored.join("workspace/data.md")).unwrap(),
            b"offline knowledge snapshot"
        );
        assert!(restore_file_bundle_to_new_directory(&bundle, &restored).is_err());
        assert_eq!(
            fs::read(restored.join("workspace/data.md")).unwrap(),
            b"offline knowledge snapshot"
        );
    }

    #[test]
    fn detached_restore_rejects_tampering_without_creating_destination() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        fs::write(&source, b"untampered").unwrap();
        let bundle = dir.path().join("bundle");
        create_file_bundle(&[(source.as_path(), Path::new("safe.txt"))], &bundle).unwrap();
        fs::write(bundle.join("safe.txt"), b"tampered").unwrap();
        let restored = dir.path().join("restored");
        assert!(restore_file_bundle_to_new_directory(&bundle, &restored).is_err());
        assert!(!restored.exists());
    }

    #[test]
    fn detached_restore_refuses_destination_nested_in_bundle() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        fs::write(&source, b"original").unwrap();
        let bundle = dir.path().join("bundle");
        create_file_bundle(&[(source.as_path(), Path::new("file.txt"))], &bundle).unwrap();
        let restored = bundle.join("restored");
        assert!(restore_file_bundle_to_new_directory(&bundle, &restored).is_err());
        assert!(!restored.exists());
        verify_file_bundle(&bundle).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlink_inputs_and_tampered_bundle_are_rejected() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, b"ok").unwrap();
        let link = dir.path().join("link");
        symlink(&file, &link).unwrap();
        let bundle = dir.path().join("bundle");
        assert!(create_file_bundle(&[(link.as_path(), Path::new("file"))], &bundle).is_err());
        assert!(!bundle.exists());
        create_file_bundle(&[(file.as_path(), Path::new("file"))], &bundle).unwrap();
        fs::remove_file(bundle.join("file")).unwrap();
        symlink(&file, bundle.join("file")).unwrap();
        assert!(verify_file_bundle(&bundle).is_err());
    }
}
