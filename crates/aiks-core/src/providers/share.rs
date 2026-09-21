use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::model::{NormalizedSession, SourceKind};
use crate::providers::{ProviderHealth, SessionProvider, SessionSummary};
use crate::share_import::SHARE_PARSER_VERSION;

pub struct CachedShareProvider {
    source: SourceKind,
    root: PathBuf,
}

impl CachedShareProvider {
    pub fn new(source: SourceKind, root: PathBuf) -> Self {
        Self { source, root }
    }

    fn source_dir(&self) -> PathBuf {
        self.root.join(self.source.as_str())
    }

    fn read_session(path: &Path) -> anyhow::Result<NormalizedSession> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
}

#[async_trait]
impl SessionProvider for CachedShareProvider {
    fn source(&self) -> SourceKind {
        self.source
    }

    fn parser_version(&self) -> &'static str {
        SHARE_PARSER_VERSION
    }

    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        let dir = self.source_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }

            match Self::read_session(&path) {
                Ok(session) if session.source == self.source => {
                    sessions.push(SessionSummary {
                        source: self.source,
                        external_session_id: session.external_session_id,
                        title: session.title,
                        project_name: session.project_name,
                        project_path: session.project_path,
                        source_path: Some(path),
                        started_at: session.started_at,
                        updated_at: session.updated_at,
                        message_count: session.messages.len(),
                    });
                }
                Ok(_) => tracing::warn!(path = %path.display(), "Ignoring Share cache with mismatched source"),
                Err(error) => tracing::warn!(
                    path = %path.display(),
                    error = %error,
                    "Ignoring invalid cached Share URL session"
                ),
            }
        }

        sessions.sort_by(|left, right| {
            right
                .updated_at
                .unwrap_or(DateTime::<Utc>::MIN_UTC)
                .cmp(&left.updated_at.unwrap_or(DateTime::<Utc>::MIN_UTC))
        });
        Ok(sessions)
    }

    async fn load_session(&self, summary: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        anyhow::ensure!(summary.source == self.source, "Share session source mismatch");
        let path = summary.source_path.as_ref().cloned().unwrap_or_else(|| {
            self.source_dir()
                .join(format!("{}.json", summary.external_session_id))
        });
        let session = Self::read_session(&path)?;
        anyhow::ensure!(session.source == self.source, "Cached Share source mismatch");
        Ok(session)
    }

    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Ok
    }
}
