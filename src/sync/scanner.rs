/// Incremental file state scanner.
///
/// Tracks JSONL file offsets, sizes, and hashes for efficient incremental scanning.
/// Detects file truncation and replacement.
use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::storage::{SourceFileState, SourceFileStateRepo, StateDb};

/// Result of checking a file for changes.
#[derive(Debug, Clone, PartialEq)]
pub enum FileChangeStatus {
    /// File is new (never seen before)
    New,
    /// File has been modified (size/mtime changed)
    Modified,
    /// File has been truncated (size shrank)
    Truncated,
    /// File has been replaced (size/mtime changed but different hash pattern)
    Replaced,
    /// File is unchanged
    Unchanged,
}

pub struct IncrementalScanner;

impl IncrementalScanner {
    /// Check if a file has changed compared to stored state.
    pub fn check_file(
        db: &StateDb,
        path: &Path,
        source: &str,
        parser_version: &str,
    ) -> anyhow::Result<FileChangeStatus> {
        let repo = SourceFileStateRepo::new(db);
        let path_str = path.to_string_lossy().to_string();

        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                return Ok(FileChangeStatus::New); // Can't read = treat as new
            }
        };

        let current_size = metadata.len() as i64;
        let current_mtime = metadata
            .modified()
            .ok()
            .map(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                dt.to_rfc3339()
            });

        let stored = repo.find(&path_str)?;

        let Some(stored) = stored else {
            return Ok(FileChangeStatus::New);
        };

        // Parser version changed → force re-parse
        if stored.parser_version.as_deref() != Some(parser_version) {
            return Ok(FileChangeStatus::Modified);
        }

        let stored_size = stored.file_size.unwrap_or(0);

        // File was truncated
        if current_size < stored_size {
            return Ok(FileChangeStatus::Truncated);
        }

        // Size unchanged and mtime unchanged → unchanged
        if stored.file_size == Some(current_size)
            && stored.modified_at == current_mtime
        {
            return Ok(FileChangeStatus::Unchanged);
        }

        // Size/mtime changed → need hash check for JSON files
        if stored.file_hash.is_some() {
            // For whole-file hashed files (JSON, not JSONL)
            let current_hash = Self::compute_file_hash(path).ok();
            if current_hash == stored.file_hash {
                // Hash unchanged - update metadata but report unchanged
                return Ok(FileChangeStatus::Unchanged);
            }
        }

        Ok(FileChangeStatus::Modified)
    }

    /// Record a file as having been processed.
    pub fn record_file(
        db: &StateDb,
        path: &Path,
        source: &str,
        parser_version: &str,
        last_offset: Option<i64>,
        file_hash: Option<&str>,
    ) -> anyhow::Result<()> {
        let repo = SourceFileStateRepo::new(db);
        let path_str = path.to_string_lossy().to_string();

        let metadata = fs::metadata(path).ok();
        let file_size = metadata.as_ref().map(|m| m.len() as i64);
        let modified_at = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                dt.to_rfc3339()
            });

        repo.upsert(
            &path_str,
            source,
            file_size,
            modified_at.as_deref(),
            last_offset,
            file_hash,
            Some(parser_version),
        )
    }

    /// Compute SHA-256 hash of a file.
    pub fn compute_file_hash(path: &Path) -> anyhow::Result<String> {
        let data = fs::read(path)?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        Ok(hex::encode(hasher.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StateDb;
    use tempfile::tempdir;

    #[test]
    fn detects_new_file() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("test.db")).unwrap();
        let file = dir.path().join("session.jsonl");
        std::fs::write(&file, b"test content").unwrap();

        let status = IncrementalScanner::check_file(&db, &file, "claude_code", "claude-v1").unwrap();
        assert_eq!(status, FileChangeStatus::New);
    }

    #[test]
    fn detects_unchanged_after_record() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("test.db")).unwrap();
        let file = dir.path().join("session.jsonl");
        std::fs::write(&file, b"test content").unwrap();

        IncrementalScanner::record_file(&db, &file, "claude_code", "claude-v1", Some(12), None).unwrap();
        
        // Small sleep to ensure mtime would change if file is rewritten
        let status = IncrementalScanner::check_file(&db, &file, "claude_code", "claude-v1").unwrap();
        assert_eq!(status, FileChangeStatus::Unchanged);
    }

    #[test]
    fn detects_truncation() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("test.db")).unwrap();
        let file = dir.path().join("session.jsonl");
        std::fs::write(&file, b"longer content here").unwrap();

        IncrementalScanner::record_file(&db, &file, "claude_code", "claude-v1", Some(20), None).unwrap();

        // Truncate the file
        std::fs::write(&file, b"shorter").unwrap();

        let status = IncrementalScanner::check_file(&db, &file, "claude_code", "claude-v1").unwrap();
        assert_eq!(status, FileChangeStatus::Truncated);
    }

    #[test]
    fn detects_parser_version_change() {
        let dir = tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("test.db")).unwrap();
        let file = dir.path().join("session.jsonl");
        std::fs::write(&file, b"content").unwrap();

        IncrementalScanner::record_file(&db, &file, "claude_code", "claude-v1", None, None).unwrap();

        // Check with new parser version
        let status = IncrementalScanner::check_file(&db, &file, "claude_code", "claude-v2").unwrap();
        assert_eq!(status, FileChangeStatus::Modified);
    }
}
