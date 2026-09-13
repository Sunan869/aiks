pub mod doctor;
pub mod scan;
pub mod sync;
pub mod status;
pub mod daemon;
pub mod resync;

use std::path::PathBuf;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "aiks",
    version,
    about = "AI Knowledge Sync — Local-first AI session knowledge sync"
)]
pub struct Cli {
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,
    #[arg(short, long, global = true)]
    pub verbose: bool,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Doctor,
    Scan {
        #[arg(long)] source: Option<String>,
    },
    Sync {
        #[arg(long)] source: Option<String>,
        #[arg(long)] dry_run: bool,
        #[arg(long)] overwrite: bool,
    },
    Status,
    Daemon,
    Resync {
        #[arg(long)] source: Option<String>,
        #[arg(long)] session_id: Vec<String>,
    },
    RebuildState {
        #[arg(long)] yes: bool,
    },
}
