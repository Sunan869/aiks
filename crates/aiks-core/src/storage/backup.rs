//! Safe, consistent snapshot of the local SQLite state database.
//! This intentionally does not restore over the live database or copy SiYuan.
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::StateDb;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteBackupManifest {
    pub format_version: u32,
    pub file_name: String,
    pub byte_length: u64,
    pub sha256: String,
    pub integrity: String,
}

pub fn create_sqlite_backup(db: &StateDb, destination: &Path) -> Result<SqliteBackupManifest> {
    if destination.exists() {
        bail!("backup destination already exists; refusing to overwrite");
    }
    let parent = destination
        .parent()
        .context("backup destination has no parent")?;
    if !parent.is_dir() {
        bail!("backup destination directory does not exist");
    }
    let file_name = destination
        .file_name()
        .and_then(|s| s.to_str())
        .context("backup file name must be valid UTF-8")?
        .to_owned();

    // SQLite VACUUM INTO copies a transactional snapshot and includes WAL pages.
    // Holding the StateDb lock prevents AIKS-owned writes during this statement.
    // Never use fs::copy on a WAL-mode live database.
    {
        let conn = db.conn();
        conn.execute(
            "VACUUM main INTO ?1",
            params![destination.to_string_lossy().as_ref()],
        )
        .context("SQLite consistent backup")?;
    }
    let result = inspect_sqlite_backup(destination, file_name);
    if result.is_err() {
        // Only delete the file created by this call, never existing files.
        let _ = fs::remove_file(destination);
    }
    result
}

pub fn inspect_sqlite_backup(path: &Path, file_name: String) -> Result<SqliteBackupManifest> {
    // Hash in bounded memory even when the state database is several GiB.
    // FTS5's integrity checker needs a writable connection even when it
    // only validates the index. This is a detached backup, never the live DB.
    // Verify its digest AFTER SQLite closes so any unexpected mutation cannot
    // pass against a previously calculated digest.
    {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        let integrity: String =
            connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            bail!("SQLite backup integrity check failed: {integrity}");
        }
    }
    let mut input =
        fs::File::open(path).with_context(|| format!("read backup: {}", path.display()))?;
    let size = input.metadata()?.len();
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let hash = hex::encode(hasher.finalize());
    let integrity = "ok".to_string();
    Ok(SqliteBackupManifest {
        format_version: 1,
        file_name,
        byte_length: size,
        sha256: hash,
        integrity,
    })
}

/// Validate a detached backup against a previously recorded manifest.
/// Never touches or replaces the running StateDb.
pub fn verify_sqlite_backup(path: &Path, manifest: &SqliteBackupManifest) -> Result<()> {
    if manifest.format_version != 1 {
        bail!("unsupported backup manifest version");
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("invalid backup path")?;
    if name != manifest.file_name {
        bail!("backup file name mismatch");
    }
    let actual = inspect_sqlite_backup(path, name.to_owned())?;
    if actual.byte_length != manifest.byte_length || actual.sha256 != manifest.sha256 {
        bail!("backup digest or size mismatch");
    }
    Ok(())
}

/// Restore only into a *new* detached file after digest and integrity checks.
/// Never replace an active database, and never mutate the original snapshot.
pub fn restore_sqlite_backup_to_new_path(
    source: &Path,
    manifest: &SqliteBackupManifest,
    destination: &Path,
) -> Result<()> {
    verify_sqlite_backup(source, manifest)?;
    let parent = destination
        .parent()
        .context("restore destination has no parent")?;
    if !parent.is_dir() {
        bail!("restore destination directory does not exist");
    }
    if destination == source {
        bail!("restore destination must differ from the snapshot");
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .context("restore destination must not exist")?;
    let result = (|| -> Result<()> {
        let mut input = fs::File::open(source)?;
        io::copy(&mut input, &mut output)?;
        output.sync_all()?;
        drop(output);
        let restored = inspect_sqlite_backup(
            destination,
            destination
                .file_name()
                .and_then(|name| name.to_str())
                .context("invalid restored file name")?
                .to_owned(),
        )?;
        if restored.sha256 != manifest.sha256 || restored.byte_length != manifest.byte_length {
            bail!("restored file digest mismatch");
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

/// Store an explicit manifest next to the snapshot without overwriting a file.
/// Caller controls the destination; no path is derived from untrusted JSON.
pub fn save_backup_manifest(path: &Path, manifest: &SqliteBackupManifest) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(manifest)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .context("backup manifest already exists or cannot be created")?;
    let result = (|| -> Result<()> {
        use std::io::Write;
        output.write_all(&bytes)?;
        output.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

/// Load a manifest, validate the snapshot, and reject missing or incompatible
/// core schema tables before exposing the backup as a restoration candidate.
pub fn load_and_verify_backup(
    snapshot: &Path,
    manifest_path: &Path,
) -> Result<SqliteBackupManifest> {
    let file = fs::File::open(manifest_path)?;
    let manifest: SqliteBackupManifest = serde_json::from_reader(file)?;
    verify_sqlite_backup(snapshot, &manifest)?;
    let conn = Connection::open_with_flags(snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    // Detect incomplete migrations, not merely the presence of table names.
    // Check the columns needed by the live synchronization and pipeline code.
    for (table, columns) in [
        (
            "source_session",
            &["id", "source", "external_session_id", "content_hash"][..],
        ),
        ("sync_target", &["session_id", "sink", "status"][..]),
        (
            "pipeline_run",
            &["id", "session_id", "status", "pipeline_version"][..],
        ),
        (
            "pipeline_job",
            &[
                "id",
                "session_id",
                "pipeline_run_id",
                "status",
                "generation",
            ][..],
        ),
    ] {
        let mut query = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let actual = query
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for column in columns {
            if !actual.iter().any(|name| name == column) {
                bail!("backup schema incompatible: missing {table}.{column}");
            }
        }
    }
    // Foreign-key validation uses a read-only transaction, so it does not
    // change the detached snapshot or attempt to migrate it.
    let invalid: i64 =
        conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if invalid != 0 {
        bail!("backup has {invalid} foreign-key violations");
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_includes_wal_data_and_never_overwrites_existing_backup() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        db.conn()
            .execute(
                "INSERT INTO source_session
                 (source, external_session_id, last_seen_at, created_at, updated_at)
                 VALUES ('codex', 'wal-record', 'now', 'now', 'now')",
                [],
            )
            .unwrap();
        let path = dir.path().join("snapshot.sqlite");
        let manifest = create_sqlite_backup(&db, &path).unwrap();
        assert_eq!(manifest.integrity, "ok");
        assert_eq!(manifest.sha256.len(), 64);
        verify_sqlite_backup(&path, &manifest).unwrap();
        assert_eq!(
            manifest.sha256,
            inspect_sqlite_backup(&path, manifest.file_name.clone())
                .unwrap()
                .sha256
        );
        assert!(create_sqlite_backup(&db, &path).is_err());
        let copy = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let count: i64 = copy
            .query_row("SELECT COUNT(*) FROM source_session", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn repeated_fts_integrity_checks_preserve_snapshot_digest() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let snapshot = dir.path().join("snapshot.sqlite");
        let manifest = create_sqlite_backup(&db, &snapshot).unwrap();
        for _ in 0..3 {
            verify_sqlite_backup(&snapshot, &manifest).unwrap();
            let actual = inspect_sqlite_backup(&snapshot, manifest.file_name.clone()).unwrap();
            assert_eq!(actual.sha256, manifest.sha256);
            assert_eq!(actual.byte_length, manifest.byte_length);
        }
    }

    #[test]
    fn detached_restore_preserves_source_and_refuses_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        db.conn()
            .execute(
                "INSERT INTO source_session
                 (source, external_session_id, last_seen_at, created_at, updated_at)
                 VALUES ('codex', 'restorable', 'now', 'now', 'now')",
                [],
            )
            .unwrap();
        let snapshot = dir.path().join("snapshot.sqlite");
        let restored = dir.path().join("restored.sqlite");
        let manifest = create_sqlite_backup(&db, &snapshot).unwrap();
        restore_sqlite_backup_to_new_path(&snapshot, &manifest, &restored).unwrap();
        assert!(restore_sqlite_backup_to_new_path(&snapshot, &manifest, &restored).is_err());
        let connection =
            Connection::open_with_flags(&restored, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM source_session", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        verify_sqlite_backup(&snapshot, &manifest).unwrap();
    }

    #[test]
    fn manifest_round_trip_and_detached_restore_are_verified() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let snapshot = dir.path().join("backup.sqlite");
        let manifest_path = dir.path().join("backup-manifest.json");
        let restored = dir.path().join("restore.sqlite");
        let manifest = create_sqlite_backup(&db, &snapshot).unwrap();
        save_backup_manifest(&manifest_path, &manifest).unwrap();
        assert!(save_backup_manifest(&manifest_path, &manifest).is_err());
        let loaded = load_and_verify_backup(&snapshot, &manifest_path).unwrap();
        restore_sqlite_backup_to_new_path(&snapshot, &loaded, &restored).unwrap();
        let mut wrong = loaded;
        wrong.format_version = 999;
        assert!(verify_sqlite_backup(&snapshot, &wrong).is_err());
    }

    #[test]
    fn backup_verification_rejects_corrupt_json_and_hash_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let snapshot = dir.path().join("backup.sqlite");
        let manifest_path = dir.path().join("backup.json");
        let manifest = create_sqlite_backup(&db, &snapshot).unwrap();
        fs::write(&manifest_path, b"not-json").unwrap();
        assert!(load_and_verify_backup(&snapshot, &manifest_path).is_err());
        fs::remove_file(&manifest_path).unwrap();
        let mut incorrect = manifest.clone();
        incorrect.byte_length += 1;
        save_backup_manifest(&manifest_path, &incorrect).unwrap();
        assert!(load_and_verify_backup(&snapshot, &manifest_path).is_err());
        verify_sqlite_backup(&snapshot, &manifest).unwrap();
    }

    #[test]
    fn detached_restore_rejects_preexisting_file_without_modifying_it() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let snapshot = dir.path().join("backup.sqlite");
        let dest = dir.path().join("existing.sqlite");
        let manifest = create_sqlite_backup(&db, &snapshot).unwrap();
        fs::write(&dest, b"do-not-replace").unwrap();
        assert!(restore_sqlite_backup_to_new_path(&snapshot, &manifest, &dest).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"do-not-replace");
    }

    #[test]
    fn restore_refuses_to_write_over_original_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let snapshot = dir.path().join("snapshot.sqlite");
        let manifest = create_sqlite_backup(&db, &snapshot).unwrap();
        assert!(restore_sqlite_backup_to_new_path(&snapshot, &manifest, &snapshot).is_err());
        verify_sqlite_backup(&snapshot, &manifest).unwrap();
    }

    #[test]
    fn corrupt_snapshot_is_rejected_without_creating_restore_destination() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let snapshot = dir.path().join("snapshot.sqlite");
        let restored = dir.path().join("restored.sqlite");
        let manifest = create_sqlite_backup(&db, &snapshot).unwrap();
        let mut bytes = fs::read(&snapshot).unwrap();
        bytes[0] ^= 0xff;
        fs::write(&snapshot, bytes).unwrap();
        assert!(restore_sqlite_backup_to_new_path(&snapshot, &manifest, &restored).is_err());
        assert!(!restored.exists());
    }

    #[test]
    fn restore_validation_rejects_incomplete_schema_without_mutating_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let snapshot = dir.path().join("incomplete.sqlite");
        let connection = Connection::open(&snapshot).unwrap();
        connection.execute_batch(
            "CREATE TABLE source_session (id INTEGER PRIMARY KEY, source TEXT);
             CREATE TABLE sync_target (session_id INTEGER, sink TEXT, status TEXT);
             CREATE TABLE pipeline_run (id TEXT, session_id INTEGER, status TEXT, pipeline_version TEXT);
             CREATE TABLE pipeline_job (id TEXT, session_id INTEGER, pipeline_run_id TEXT, status TEXT, generation INTEGER);",
        ).unwrap();
        drop(connection);
        let manifest = inspect_sqlite_backup(&snapshot, "incomplete.sqlite".to_string()).unwrap();
        let manifest_path = dir.path().join("manifest.json");
        save_backup_manifest(&manifest_path, &manifest).unwrap();
        let error = load_and_verify_backup(&snapshot, &manifest_path).unwrap_err();
        assert!(error
            .to_string()
            .contains("source_session.external_session_id"));
        verify_sqlite_backup(&snapshot, &manifest).unwrap();
    }

    #[test]
    fn incompatible_backup_with_broken_foreign_key_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let snapshot = dir.path().join("invalid.sqlite");
        create_sqlite_backup(&db, &snapshot).unwrap();
        // Deliberately corrupt only the detached snapshot. The full AIKS
        // schema remains present so the FK checker, not the schema gate,
        // must detect the invalid reference.
        let connection = Connection::open(&snapshot).unwrap();
        connection
            .execute_batch(
                "PRAGMA foreign_keys=OFF;
             INSERT INTO sync_target(session_id, sink, status)
             VALUES (999999, 'siyuan', 'PENDING');",
            )
            .unwrap();
        drop(connection);
        let manifest = inspect_sqlite_backup(&snapshot, "invalid.sqlite".to_string()).unwrap();
        let file = dir.path().join("backup.json");
        save_backup_manifest(&file, &manifest).unwrap();
        let error = load_and_verify_backup(&snapshot, &file).unwrap_err();
        assert!(error.to_string().contains("foreign-key violations"));
        verify_sqlite_backup(&snapshot, &manifest).unwrap();
        let snapshot_violations: i64 = Connection::open_with_flags(
            &snapshot,
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_check",
            [],
            |row| row.get(0),
        )
        .unwrap();
        assert_eq!(snapshot_violations, 1);
        assert_eq!(
            db.conn()
                .query_row("SELECT COUNT(*) FROM sync_target", [], |row| row
                    .get::<_, i64>(0),)
                .unwrap(),
            0
        );
    }

    #[test]
    fn manifest_refuses_unsupported_version_even_for_valid_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let snapshot = dir.path().join("backup.sqlite");
        let manifest_path = dir.path().join("manifest.json");
        let mut manifest = create_sqlite_backup(&db, &snapshot).unwrap();
        manifest.format_version = 2;
        save_backup_manifest(&manifest_path, &manifest).unwrap();
        assert!(load_and_verify_backup(&snapshot, &manifest_path).is_err());
    }

    #[test]
    fn verification_rejects_tampered_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("state.db")).unwrap();
        let path = dir.path().join("snapshot.sqlite");
        let manifest = create_sqlite_backup(&db, &path).unwrap();
        let mut tampered = manifest.clone();
        tampered.sha256 = "0".repeat(64);
        assert!(verify_sqlite_backup(&path, &tampered).is_err());
        verify_sqlite_backup(&path, &manifest).unwrap();
    }
}
