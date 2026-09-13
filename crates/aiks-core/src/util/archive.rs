use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use flate2::write::GzEncoder;
use flate2::Compression;

use crate::model::NormalizedSession;

/// Archive stores NormalizedSession as gzip-compressed JSON.
///
/// Location: {archive_dir}/{source}/{session_id}.json.gz
pub struct Archive {
    base_dir: PathBuf,
    enabled: bool,
}

impl Archive {
    pub fn new(base_dir: PathBuf, enabled: bool) -> Self {
        Self { base_dir, enabled }
    }

    /// Build the archive file path for a session.
    pub fn session_path(&self, source: &str, session_id: &str) -> PathBuf {
        self.base_dir
            .join(source)
            .join(format!("{}.json.gz", session_id))
    }

    /// Write a NormalizedSession to the archive.
    ///
    /// Silently does nothing if archiving is disabled.
    pub fn write(&self, session: &NormalizedSession) -> anyhow::Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let source = session.source.as_str();
        let dir = self.base_dir.join(source);
        fs::create_dir_all(&dir)
            .with_context(|| format!("create archive dir: {}", dir.display()))?;

        let path = self.session_path(source, &session.external_session_id);

        let json = serde_json::to_vec(session).context("serialize session to JSON")?;

        let file = fs::File::create(&path)
            .with_context(|| format!("create archive file: {}", path.display()))?;

        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(&json).context("write compressed data")?;
        encoder.finish().context("finalize gzip")?;

        Ok(())
    }

    /// Read a NormalizedSession from the archive.
    pub fn read(&self, source: &str, session_id: &str) -> anyhow::Result<NormalizedSession> {
        let path = self.session_path(source, session_id);
        let data = fs::read(&path)
            .with_context(|| format!("read archive: {}", path.display()))?;

        let mut decoder = flate2::read::GzDecoder::new(data.as_slice());
        let mut json_bytes = Vec::new();
        std::io::Read::read_to_end(&mut decoder, &mut json_bytes)
            .context("decompress archive")?;

        serde_json::from_slice(&json_bytes).context("deserialize session from archive")
    }

    /// Check if an archive exists for a session.
    pub fn exists(&self, source: &str, session_id: &str) -> bool {
        if !self.enabled {
            return false;
        }
        self.session_path(source, session_id).exists()
    }

    /// List all archived session IDs for a given source.
    pub fn list_sessions(&self, source: &str) -> Vec<String> {
        let dir = self.base_dir.join(source);
        if !dir.exists() {
            return Vec::new();
        }

        fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.strip_suffix(".json.gz").map(|id| id.to_string())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn make_test_session() -> NormalizedSession {
        NormalizedSession {
            source: SourceKind::ClaudeCode,
            external_session_id: "test-archive-session".to_string(),
            title: Some("Archive Test".to_string()),
            project_name: None,
            project_path: Some("/home/user/project".to_string()),
            source_path: None,
            started_at: None,
            updated_at: None,
            model: Some("claude-opus-4-5".to_string()),
            messages: vec![NormalizedMessage {
                external_id: "msg-1".to_string(),
                parent_id: None,
                role: MessageRole::User,
                created_at: None,
                model: None,
                blocks: vec![ContentBlock::Text {
                    text: "Hello from archive test".to_string(),
                }],
                usage: None,
                metadata: HashMap::new(),
            }],
            usage: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn write_and_read_roundtrip() {
        let dir = tempdir().unwrap();
        let archive = Archive::new(dir.path().to_path_buf(), true);
        let session = make_test_session();

        archive.write(&session).unwrap();
        assert!(archive.exists("claude_code", "test-archive-session"));

        let recovered = archive.read("claude_code", "test-archive-session").unwrap();
        assert_eq!(recovered.external_session_id, session.external_session_id);
        assert_eq!(recovered.messages.len(), 1);
    }

    #[test]
    fn disabled_archive_does_not_write() {
        let dir = tempdir().unwrap();
        let archive = Archive::new(dir.path().to_path_buf(), false);
        let session = make_test_session();

        archive.write(&session).unwrap();
        assert!(!archive.exists("claude_code", "test-archive-session"));
    }

    #[test]
    fn list_sessions_returns_written_ids() {
        let dir = tempdir().unwrap();
        let archive = Archive::new(dir.path().to_path_buf(), true);
        let session = make_test_session();

        archive.write(&session).unwrap();
        let ids = archive.list_sessions("claude_code");
        assert!(ids.contains(&"test-archive-session".to_string()));
    }
}
