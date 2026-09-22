use super::{
    discovery::DiscoveryReport,
    local_io::{ReadLimits, ScopedReader},
    local_paths, message_parts, ProviderHealth, SessionProvider, SessionSummary,
};
use crate::config::ExternalProviderConfig;
use crate::model::{NormalizedSession, SourceKind};
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct NativeProvider {
    source: SourceKind,
    roots: Vec<PathBuf>,
    explicit: bool,
}
impl NativeProvider {
    pub fn new(source: SourceKind, config: &ExternalProviderConfig) -> Result<Self> {
        ensure!(
            super::catalog::EXTERNAL_SOURCES.contains(&source),
            "not an external provider"
        );
        let roots = local_paths::roots(source, config);
        ensure!(roots.len() <= 32, "at most 32 provider roots are allowed");
        Ok(Self {
            source,
            roots,
            explicit: !config.path.trim().is_empty() || !config.paths.is_empty(),
        })
    }
    fn parse(
        &self,
        io: &ScopedReader,
        path: &Path,
        metadata_only: bool,
        expected: Option<&str>,
    ) -> Result<(Vec<NormalizedSession>, bool)> {
        ensure!(
            local_paths::admitted(self.source, path),
            "file is not in this provider's transcript allowlist"
        );
        let (mut sessions, complete) = match self.source {
            SourceKind::QwenCode => super::qwen::read(io, path, metadata_only)?,
            SourceKind::Continue => (super::continue_dev::read(io, path, metadata_only)?, true),
            SourceKind::CursorAgent => (super::cursor_agent::read(io, path, metadata_only)?, true),
            SourceKind::Cline | SourceKind::RooCode | SourceKind::KiloCode => (
                super::cline_family::read(io, path, self.source, metadata_only)?,
                true,
            ),
            SourceKind::Aider => (super::aider::read(io, path, metadata_only)?, true),
            SourceKind::KimiCode => (super::kimi::read(io, path, metadata_only)?, true),
            SourceKind::Cursor => super::cursor::read(io, path, metadata_only, expected)?,
            SourceKind::GithubCopilot => super::copilot::read(io, path, metadata_only)?,
            SourceKind::Antigravity => super::antigravity::read(io, path, metadata_only)?,
            _ => anyhow::bail!("unregistered external source"),
        };
        if matches!(self.source, SourceKind::QwenCode | SourceKind::Continue) {
            let owner = io.checked_path(path.parent().unwrap_or(Path::new("")))?;
            for s in &mut sessions {
                s.metadata.insert(
                    "upstream_session_id".into(),
                    s.external_session_id.clone().into(),
                );
                s.external_session_id = message_parts::scoped_id(&owner, &s.external_session_id);
            }
        }
        Ok((sessions, complete))
    }
    fn scan(&self) -> Result<DiscoveryReport> {
        let mut report = DiscoveryReport::default();
        if self.roots.is_empty() {
            report.incomplete("not_configured", 0);
            return Ok(report);
        }
        let mut roots_seen = HashSet::new();
        let mut files_seen = HashSet::new();
        let mut ids = HashMap::<String, PathBuf>::new();
        let mut found = false;
        for (store, root) in self.roots.iter().enumerate() {
            match std::fs::symlink_metadata(root) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && !self.explicit => continue,
                Err(_) => {
                    report.incomplete("root_unavailable", store);
                    continue;
                }
                Ok(_) => {}
            }
            let io = match ScopedReader::new(root.clone(), ReadLimits::default()) {
                Ok(io) => io,
                Err(_) => {
                    report.incomplete("root_not_readable", store);
                    continue;
                }
            };
            if !roots_seen.insert(io.root().to_path_buf()) {
                continue;
            }
            match local_paths::marker(&io, self.source) {
                Ok(false) => {
                    if self.explicit {
                        report.incomplete("unsupported_layout", store);
                    }
                    continue;
                }
                Err(_) => {
                    report.incomplete("marker_not_readable", store);
                    continue;
                }
                Ok(true) => {}
            }
            found = true;
            let candidates = match local_paths::candidates(&io, self.source) {
                Ok(paths) => paths,
                Err(_) => {
                    report.incomplete("discovery_incomplete", store);
                    continue;
                }
            };
            if self.source == SourceKind::Antigravity && candidates.is_empty() {
                report.incomplete("unsupported_antigravity_message_store", store);
                continue;
            }
            report.covered_paths.push(io.root().to_path_buf());
            for relative in candidates {
                let path = match io.checked_path(&relative) {
                    Ok(path) => path,
                    Err(_) => {
                        report.incomplete("source_changed", store);
                        continue;
                    }
                };
                if !files_seen.insert(path.clone()) {
                    continue;
                }
                let (sessions, complete) = match self.parse(&io, &relative, true, None) {
                    Ok(parsed) => parsed,
                    Err(error)
                        if self.source == SourceKind::GithubCopilot
                            && relative.extension().and_then(|value| value.to_str())
                                == Some("json") =>
                    {
                        tracing::warn!(
                            source = self.source.as_str(),
                            store_index = store,
                            path = %relative.display(),
                            error = %error,
                            "Skipping unreadable standalone Copilot chat session"
                        );
                        // Flat VS Code chat-session files are independent snapshots.
                        // Quarantine one unreadable historical snapshot so hundreds of
                        // valid neighbors remain usable, but suppress missing detection
                        // for this source so previously imported data is never deleted.
                        report.suppress_missing_detection();
                        continue;
                    }
                    Err(error) => {
                        tracing::warn!(
                            source = self.source.as_str(),
                            store_index = store,
                            path = %relative.display(),
                            error = %error,
                            "Provider candidate could not be parsed"
                        );
                        report.incomplete("transcript_or_schema_unreadable", store);
                        continue;
                    }
                };
                if !complete {
                    tracing::warn!(
                        source = self.source.as_str(),
                        store_index = store,
                        path = %relative.display(),
                        "Provider candidate was only partially readable"
                    );
                    report.incomplete("partial_store", store);
                }
                for s in sessions {
                    if let Some(previous) = ids.get(&s.external_session_id) {
                        // Modern global Cursor data is authoritative; workspace index is a fallback.
                        if self.source == SourceKind::Cursor
                            && previous.ends_with("globalStorage/state.vscdb")
                        {
                            continue;
                        }
                        if previous == &path {
                            continue;
                        }
                        report.incomplete("ambiguous_session_identity", store);
                        report
                            .sessions
                            .retain(|summary| summary.external_session_id != s.external_session_id);
                        continue;
                    }
                    ids.insert(s.external_session_id.clone(), path.clone());
                    report.sessions.push(message_parts::summary(&s));
                    ensure!(
                        report.sessions.len() <= 100_000,
                        "provider session budget exceeded"
                    );
                }
            }
        }
        if !found {
            report.incomplete("not_found", 0);
        }
        report.sessions.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then(a.external_session_id.cmp(&b.external_session_id))
        });
        Ok(report)
    }
    fn health(&self) -> ProviderHealth {
        if self.roots.is_empty() {
            return ProviderHealth::NotConfigured;
        }
        let mut detected = false;
        for root in &self.roots {
            if !root.exists() {
                continue;
            }
            let io = match ScopedReader::new(root.clone(), ReadLimits::default()) {
                Ok(io) => io,
                Err(_) => {
                    return ProviderHealth::Error {
                        message: "Provider root is unreadable or linked".into(),
                    }
                }
            };
            match local_paths::marker(&io, self.source) {
                Ok(true) => {
                    detected = true;
                    if self.source == SourceKind::Cursor {
                        let paths = match local_paths::candidates(&io, self.source) {
                            Ok(paths) => paths,
                            Err(_) => {
                                return ProviderHealth::Error {
                                    message: "Cursor database discovery is incomplete".into(),
                                }
                            }
                        };
                        if let Some(path) = paths.first() {
                            let schema = io.open_readonly(path).and_then(|conn| {
                                conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('cursorDiskKV','ItemTable')", [], |r| r.get::<_, i64>(0)).map_err(Into::into)
                            });
                            match schema {
                                Ok(0) => {
                                    return ProviderHealth::Unsupported {
                                        message: "Cursor database schema is unsupported".into(),
                                    }
                                }
                                Err(_) => {
                                    return ProviderHealth::Error {
                                        message: "Cursor database schema is unreadable".into(),
                                    }
                                }
                                Ok(_) => {}
                            }
                        }
                    }
                    if self.source != SourceKind::Antigravity {
                        return ProviderHealth::Ok;
                    }
                    if let Ok(files) = local_paths::candidates(&io, self.source) {
                        if !files.is_empty() {
                            return ProviderHealth::Ok;
                        }
                    }
                }
                Err(_) => {
                    return ProviderHealth::Error {
                        message: "Provider store metadata is unreadable".into(),
                    }
                }
                Ok(false) => {}
            }
        }
        if detected || self.explicit && self.roots.iter().any(|p| p.exists()) {
            ProviderHealth::Unsupported { message: "Detected store has no supported local conversation layout; token statistics are not conversations".into() }
        } else {
            ProviderHealth::NotFound {
                message: format!(
                    "{} local session store not found",
                    self.source.display_name()
                ),
            }
        }
    }
}
#[async_trait]
impl SessionProvider for NativeProvider {
    fn source(&self) -> SourceKind {
        self.source
    }
    fn parser_version(&self) -> &'static str {
        match self.source {
            SourceKind::QwenCode => "qwen-jsonl-v1",
            SourceKind::Continue => "continue-session-v1",
            SourceKind::CursorAgent => "cursor-agent-jsonl-v1",
            SourceKind::Cline => "cline-task-v1",
            SourceKind::RooCode => "roo-task-v1",
            SourceKind::KiloCode => "kilo-task-v1",
            SourceKind::Aider => "aider-markdown-v1",
            SourceKind::KimiCode => "kimi-session-v1",
            SourceKind::Cursor => "cursor-sqlite-v1",
            SourceKind::GithubCopilot => "copilot-history-v1",
            SourceKind::Antigravity => "antigravity-local-v1",
            _ => "unsupported",
        }
    }
    async fn discover_report(&self) -> Result<DiscoveryReport> {
        let owned = self.clone();
        tokio::task::spawn_blocking(move || owned.scan())
            .await
            .context("provider scan worker failed")?
    }
    async fn discover_sessions(&self) -> Result<Vec<SessionSummary>> {
        let report = self.discover_report().await?;
        ensure!(
            report.complete,
            "provider scan incomplete; diagnostic codes: {}",
            report
                .diagnostics
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        Ok(report.sessions)
    }
    async fn load_session(&self, summary: &SessionSummary) -> Result<NormalizedSession> {
        ensure!(
            summary.source == self.source,
            "summary belongs to another provider"
        );
        let owned = self.clone();
        let summary = summary.clone();
        tokio::task::spawn_blocking(move || {
            let path = summary
                .source_path
                .as_ref()
                .context("provider summary has no source path")?;
            for root in &owned.roots {
                let Ok(io) = ScopedReader::new(root.clone(), ReadLimits::default()) else {
                    continue;
                };
                let Ok(relative) = io.relative(path) else {
                    continue;
                };
                let (sessions, complete) =
                    owned.parse(&io, &relative, false, Some(&summary.external_session_id))?;
                ensure!(
                    complete,
                    "provider session load incomplete; previous data retained"
                );
                return sessions
                    .into_iter()
                    .find(|s| s.external_session_id == summary.external_session_id)
                    .context("source no longer contains the requested session");
            }
            anyhow::bail!("source path is outside configured provider roots")
        })
        .await
        .context("provider load worker failed")?
    }
    async fn health_check(&self) -> ProviderHealth {
        let owned = self.clone();
        tokio::task::spawn_blocking(move || owned.health())
            .await
            .unwrap_or_else(|_| ProviderHealth::Error {
                message: "provider health worker failed".into(),
            })
    }
}
