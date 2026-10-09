//! Safe, consistent snapshot of the local SQLite state database.
//! This intentionally does not restore over the live database or copy SiYuan.
use std::fs::{self, OpenOptions};
use std::io;
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
    let bytes = fs::read(path).with_context(|| format!("read backup: {}", path.display()))?;
    let hash = hex::encode(Sha256::digest(&bytes));
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        bail!("SQLite backup integrity check failed: {integrity}");
    }
    Ok(SqliteBackupManifest {
        format_version: 1,
        file_name,
        byte_length: bytes.len() as u64,
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
    let parent = destination.parent().context("restore destination has no parent")?;
    if !parent.is_dir() {
        bail!("restore destination directory does not exist");
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
        assert!(create_sqlite_backup(&db, &path).is_err());
        let copy = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let count: i64 = copy
            .query_row("SELECT COUNT(*) FROM source_session", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
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
