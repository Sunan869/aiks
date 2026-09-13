pub mod doctor;
pub mod scan;
pub mod sync;
pub mod status;
pub mod daemon;
pub mod resync;

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use crate::config::Config;

#[derive(Parser)]
#[command(
    name = "aiks",
    version,
    about = "AI Knowledge Sync - Local-first AI session knowledge sync tool",
    long_about = None
)]
pub struct Cli {
    /// Path to config file (default: ~/.config/aiks/config.toml or ./config.toml)
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Check system health (providers, SiYuan connection, DB)
    Doctor,

    /// Scan and report discovered sessions without syncing
    Scan {
        /// Only scan this source (claude, codex, gemini, opencode)
        #[arg(long)]
        source: Option<String>,
    },

    /// Sync sessions to SiYuan
    Sync {
        /// Only sync this source
        #[arg(long)]
        source: Option<String>,

        /// Show what would be done without actually syncing
        #[arg(long)]
        dry_run: bool,

        /// Force overwrite conflicted documents
        #[arg(long)]
        overwrite: bool,
    },

    /// Show sync status
    Status,

    /// Run as a daemon (file watcher + periodic scan)
    Daemon,

    /// Force re-sync specific sessions
    Resync {
        /// Source to resync (claude, codex, gemini, opencode)
        #[arg(long)]
        source: Option<String>,

        /// Specific session IDs to resync
        #[arg(long)]
        session_id: Vec<String>,
    },

    /// Rebuild sync state from scratch
    RebuildState {
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

/// Load configuration from file or use defaults.
pub fn load_config(config_path: Option<PathBuf>) -> Arc<Config> {
    let path = config_path.or_else(|| {
        // Try ~/.config/aiks/config.toml
        dirs::config_dir()
            .map(|d| d.join("aiks").join("config.toml"))
            .filter(|p| p.exists())
    }).or_else(|| {
        // Try ./config.toml
        let p = PathBuf::from("config.toml");
        if p.exists() { Some(p) } else { None }
    });

    let config = if let Some(path) = path {
        match Config::from_file(&path) {
            Ok(c) => {
                tracing::info!(path = %path.display(), "Loaded config");
                c
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load config, using defaults");
                Config::default()
            }
        }
    } else {
        tracing::debug!("No config file found, using defaults");
        Config::default()
    };

    Arc::new(config)
}
