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
    /// Re-run sync for specific sessions (resets their hash so they are re-pushed)
    Resync {
        #[arg(long)] source: Option<String>,
        #[arg(long)] session_id: Vec<String>,
    },
    /// Rebuild the sync index (sync_target / sync_run / source_file_state / content_hash).
    /// Knowledge data (knowledge_item, pipeline_run, ...) is PRESERVED. Non-destructive.
    RebuildState {
        #[arg(long)] yes: bool,
    },
    /// DANGEROUS: delete the entire state database including all knowledge data.
    /// Requires explicit --yes and typing "DELETE ALL" to confirm.
    ResetData {
        #[arg(long)] yes: bool,
    },
    /// Sync distilled knowledge items into the SiYuan "AI Knowledge" notebook
    /// (knowledge-first tree: /20 Knowledge/{project}/{category}/...). Each doc
    /// links back to its raw session doc via a siyuan:// deep link.
    SyncKnowledge {
        /// Recreate docs even when SiYuan-side edits were detected (conflicts).
        #[arg(long)] overwrite_conflicts: bool,
    },
}
