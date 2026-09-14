use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::model::SourceKind;

/// A file change event indicating which provider was affected
#[derive(Debug, Clone)]
pub struct WatchEvent {
    pub source: SourceKind,
    pub path: PathBuf,
}

/// Watch configuration per provider
#[derive(Debug, Clone)]
struct WatchTarget {
    source: SourceKind,
    path: PathBuf,
    recursive: bool,
}

/// The filesystem watcher manager.
///
/// Usage:
/// 1. Create with `FileWatcher::new(config, event_tx)`
/// 2. Call `start()` to begin watching
/// 3. Consume events from `event_rx`
/// 4. Call `stop()` to clean up
pub struct FileWatcher {
    config: Arc<Config>,
    event_tx: mpsc::UnboundedSender<WatchEvent>,
}

impl FileWatcher {
    pub fn new(config: Arc<Config>, event_tx: mpsc::UnboundedSender<WatchEvent>) -> Self {
        Self { config, event_tx }
    }

    /// Build the list of paths to watch based on configuration.
    fn watch_targets(&self) -> Vec<WatchTarget> {
        let mut targets = Vec::new();

        // Claude Code: ~/.claude/projects/
        if self.config.providers.claude.enabled {
            let base = if !self.config.providers.claude.path.is_empty() {
                PathBuf::from(&self.config.providers.claude.path)
            } else if let Some(home) = dirs::home_dir() {
                home.join(".claude").join("projects")
            } else {
                return targets;
            };
            if base.exists() {
                targets.push(WatchTarget {
                    source: SourceKind::ClaudeCode,
                    path: base,
                    recursive: true,
                });
            }
        }

        // Codex: ~/.codex/sessions/
        if self.config.providers.codex.enabled {
            let base = if !self.config.providers.codex.path.is_empty() {
                PathBuf::from(&self.config.providers.codex.path).join("sessions")
            } else if let Some(home) = dirs::home_dir() {
                home.join(".codex").join("sessions")
            } else {
                return targets;
            };
            if base.exists() {
                targets.push(WatchTarget {
                    source: SourceKind::Codex,
                    path: base,
                    recursive: true,
                });
            }
        }

        // Gemini CLI: ~/.gemini/tmp/
        if self.config.providers.gemini.enabled {
            let base = if !self.config.providers.gemini.path.is_empty() {
                PathBuf::from(&self.config.providers.gemini.path).join("tmp")
            } else if let Some(home) = dirs::home_dir() {
                home.join(".gemini").join("tmp")
            } else {
                return targets;
            };
            if base.exists() {
                targets.push(WatchTarget {
                    source: SourceKind::GeminiCli,
                    path: base,
                    recursive: true,
                });
            }
        }

        // OpenCode: watch the DB file + WAL
        if self.config.providers.opencode.enabled {
            let db_path = if !self.config.providers.opencode.path.is_empty() {
                PathBuf::from(&self.config.providers.opencode.path)
            } else {
                crate::providers::opencode::OpenCodeProvider::default_path()
                    .unwrap_or_else(|_| PathBuf::from("opencode.db"))
            };
            if let Some(db_dir) = db_path.parent() {
                if db_dir.exists() {
                    targets.push(WatchTarget {
                        source: SourceKind::OpenCode,
                        path: db_dir.to_path_buf(),
                        recursive: false,
                    });
                }
            }
        }

        targets
    }

    /// Determine which SourceKind is responsible for a changed path.
    fn path_to_source(path: &std::path::Path, targets: &[WatchTarget]) -> Option<SourceKind> {
        for target in targets {
            if path.starts_with(&target.path) {
                return Some(target.source);
            }
        }
        None
    }

    /// Start watching. Returns a handle that stops watching when dropped.
    ///
    /// This spawns a blocking thread for the watcher and sends events over `event_tx`.
    pub fn start(&self) -> anyhow::Result<WatcherHandle> {
        let targets = self.watch_targets();
        if targets.is_empty() {
            info!("No watch targets found (no AI tool directories exist yet)");
            return Ok(WatcherHandle { _inner: None });
        }

        let event_tx = self.event_tx.clone();
        let targets_clone = targets.clone();

        let (tx, rx) = std::sync::mpsc::channel::<DebounceEventResult>();

        let debounce_duration = Duration::from_secs(self.config.sync.debounce_seconds);

        // Spawn the debouncer on a background thread
        let mut debouncer: Debouncer<notify::RecommendedWatcher, FileIdMap> =
            new_debouncer(debounce_duration, None, tx).map_err(|e| {
                anyhow::anyhow!("Failed to create file watcher: {}", e)
            })?;

        for target in &targets {
            let mode = if target.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            match debouncer.watcher().watch(&target.path, mode) {
                Ok(()) => info!(path = %target.path.display(), source = ?target.source, "Watching"),
                Err(e) => warn!(path = %target.path.display(), error = %e, "Cannot watch path"),
            }
        }

        // Spawn thread to forward events over mpsc
        let _handle = std::thread::spawn(move || {
            loop {
                match rx.recv() {
                    Ok(Ok(events)) => {
                        let mut seen_sources = std::collections::HashSet::new();
                        for event in events {
                            for path in &event.paths {
                                // Filter: only care about create/modify, not metadata
                                let is_relevant = matches!(
                                    event.kind,
                                    notify::EventKind::Create(_)
                                        | notify::EventKind::Modify(_)
                                        | notify::EventKind::Remove(_)
                                );
                                if !is_relevant {
                                    continue;
                                }
                                if let Some(source) =
                                    Self::path_to_source(path, &targets_clone)
                                {
                                    if seen_sources.insert(source) {
                                        debug!(
                                            path = %path.display(),
                                            source = ?source,
                                            "File change detected"
                                        );
                                        let _ = event_tx.send(WatchEvent {
                                            source,
                                            path: path.clone(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    Ok(Err(errors)) => {
                        for e in errors {
                            warn!(error = %e, "Watcher error");
                        }
                    }
                    Err(_) => {
                        // Channel closed — watcher stopped
                        break;
                    }
                }
            }
        });

        info!(
            targets = targets.len(),
            "File watcher started"
        );

        Ok(WatcherHandle {
            _inner: Some(Box::new(debouncer)),
        })
    }
}

/// Handle that keeps the watcher alive. Drop to stop watching.
pub struct WatcherHandle {
    _inner: Option<Box<dyn std::any::Any + Send>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::sync::Arc;

    #[test]
    fn no_targets_when_dirs_absent() {
        let mut config = Config::default();
        // Point to non-existent paths
        config.providers.claude.path = "/nonexistent/claude".to_string();
        config.providers.codex.path = "/nonexistent/codex".to_string();
        config.providers.gemini.path = "/nonexistent/gemini".to_string();
        config.providers.opencode.path = "/nonexistent/opencode.db".to_string();

        let (tx, _rx) = mpsc::unbounded_channel();
        let watcher = FileWatcher::new(Arc::new(config), tx);
        let targets = watcher.watch_targets();
        assert!(targets.is_empty(), "Nonexistent dirs should yield no targets");
    }
}
