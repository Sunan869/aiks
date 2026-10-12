use std::fs;
use std::path::Path;

use aiks_core::storage::app_backup::{
    create_offline_backup, preflight_offline_restore, restore_offline_backup_to_new_directory,
    verify_offline_backup,
};
use aiks_core::storage::backup_bundle::{
    create_file_bundle, preflight_file_bundle_restore, verify_file_bundle, BundleManifest,
};
use aiks_core::storage::StateDb;
use tempfile::tempdir;

#[test]
fn portable_manifest_preserves_unicode_and_forward_slashes() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("note.sy");
    fs::write(&source, b"safe cross platform data").unwrap();
    let bundle = dir.path().join("bundle");
    let entry = Path::new("项目/工作记录.sy");
    let manifest = create_file_bundle(&[(source.as_path(), entry)], &bundle).unwrap();
    assert_eq!(manifest.entries[0].relative_path, "项目/工作记录.sy");
    let target = dir.path().join("restored");
    let preview = preflight_file_bundle_restore(&bundle, &target).unwrap();
    assert_eq!(preview.file_count, 1);
    assert_eq!(preview.total_bytes, 24);
    verify_file_bundle(&bundle).unwrap();
}

#[test]
fn windows_special_paths_are_rejected_on_every_host() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("note.sy");
    fs::write(&source, b"unchanged").unwrap();
    for (index, invalid) in [
        "../escape",
        r"safe\..\escape",
        r"C:\Windows\system.ini",
        "CON",
        "NUL.txt",
        "folder/aux.json",
        "folder/COM1.bin",
        "folder/LPT9.txt",
        "folder/name.",
        "folder/name ",
        "folder/name:stream",
        "folder/a?b",
        "bundle-manifest.json",
        "BUNDLE-MANIFEST.JSON/nested",
    ]
    .iter()
    .enumerate()
    {
        let bundle = dir.path().join(format!("bundle-{index}"));
        assert!(
            create_file_bundle(&[(source.as_path(), Path::new(invalid))], &bundle).is_err(),
            "{invalid} should be unsafe everywhere"
        );
        assert!(!bundle.exists());
    }
    assert_eq!(fs::read(&source).unwrap(), b"unchanged");
}

#[test]
fn portable_case_and_parent_collisions_fail_before_leaving_a_bundle() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("note.sy");
    fs::write(&source, b"preserved").unwrap();
    for (index, paths) in [
        ["Notes/Entry.sy", "notes/entry.sy"],
        ["notes", "NOTES/entry.sy"],
        ["notes/entry.sy", "NOTES"],
    ]
    .iter()
    .enumerate()
    {
        let bundle = dir.path().join(format!("collision-{index}"));
        let inputs = [
            (source.as_path(), Path::new(paths[0])),
            (source.as_path(), Path::new(paths[1])),
        ];
        assert!(create_file_bundle(&inputs, &bundle).is_err());
        assert!(!bundle.exists());
    }
    assert_eq!(fs::read(&source).unwrap(), b"preserved");
}

#[test]
fn tampered_manifest_rejects_backslashes_before_restore() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("note.sy");
    fs::write(&source, b"untouched").unwrap();
    let bundle = dir.path().join("bundle");
    create_file_bundle(&[(source.as_path(), Path::new("notes/safe.sy"))], &bundle).unwrap();
    let manifest_path = bundle.join("bundle-manifest.json");
    let mut manifest: BundleManifest =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest.entries[0].relative_path = r"notes\safe.sy".into();
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let target = dir.path().join("restored");
    assert!(verify_file_bundle(&bundle).is_err());
    assert!(preflight_file_bundle_restore(&bundle, &target).is_err());
    assert!(!target.exists());
}

#[test]
fn offline_restore_preview_checks_schema_paths_and_requires_detached_destination() {
    let dir = tempdir().unwrap();
    let db = StateDb::open(&dir.path().join("state.db")).unwrap();
    let source = dir.path().join("note.sy");
    fs::write(&source, b"snapshot content").unwrap();
    let bundle = dir.path().join("bundle");
    create_offline_backup(
        &db,
        &[(source.as_path(), Path::new("notes/note.sy"))],
        &bundle,
    )
    .unwrap();
    let target = dir.path().join("restored");
    let preview = preflight_offline_restore(&bundle, &target).unwrap();
    assert_eq!(preview.file_count, 3);
    assert_eq!(preview.siyuan_files, 1);
    assert!(preview.total_bytes > 16);
    assert!(preview.requires_siyuan_shutdown);
    fs::create_dir(&target).unwrap();
    assert!(preflight_offline_restore(&bundle, &target).is_err());
    fs::remove_dir(&target).unwrap();
    restore_offline_backup_to_new_directory(&bundle, &target).unwrap();
    assert_eq!(
        fs::read(target.join("siyuan/notes/note.sy")).unwrap(),
        b"snapshot content"
    );

    let manifest_path = bundle.join("bundle-manifest.json");
    let mut manifest: BundleManifest =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest
        .entries
        .retain(|entry| entry.relative_path != "sqlite/state-manifest.json");
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(verify_offline_backup(&bundle).is_err());
    assert_eq!(fs::read(&source).unwrap(), b"snapshot content");
}
