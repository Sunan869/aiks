pub mod cli;
pub mod config;
pub mod model;
pub mod providers;
pub mod renderer;
pub mod sink;
pub mod storage;
pub mod sync;
pub mod util;

use clap::Parser;
use tracing_subscriber::{fmt, EnvFilter};

use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let env_filter = if cli.verbose {
        EnvFilter::new("aiks=debug,info")
    } else {
        EnvFilter::new("aiks=info,warn")
    };

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_level(true)
        .compact()
        .init();

    let config = cli::load_config(cli.config);

    match cli.command {
        Commands::Doctor => {
            cli::doctor::run(config).await?;
        }

        Commands::Scan { source } => {
            cli::scan::run(config, source).await?;
        }

        Commands::Sync {
            source,
            dry_run,
            overwrite,
        } => {
            cli::sync::run(config, source, dry_run, overwrite).await?;
        }

        Commands::Status => {
            cli::status::run(config).await?;
        }

        Commands::Daemon => {
            cli::daemon::run(config).await?;
        }

        Commands::Resync { source, session_id } => {
            cli::resync::run(config, source, session_id).await?;
        }

        Commands::RebuildState { yes } => {
            cli::resync::rebuild_state(config, yes).await?;
        }
    }

    Ok(())
}
