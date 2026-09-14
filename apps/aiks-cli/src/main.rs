mod cli;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use aiks_core::{AiksEngine, AiksEngineConfig};
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let env_filter = if cli.verbose {
        EnvFilter::new("aiks=debug,info")
    } else {
        EnvFilter::new("aiks=info,warn")
    };

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .init();

    let engine_config = AiksEngineConfig {
        config_path: cli.config.clone(),
        ..Default::default()
    };

    match &cli.command {
        Commands::Doctor => {
            let engine = AiksEngine::initialize(engine_config)?;
            cli::doctor::run(&engine).await?;
        }
        Commands::Scan { source } => {
            let engine = AiksEngine::initialize(engine_config)?;
            cli::scan::run(&engine, source.clone()).await?;
        }
        Commands::Sync { source, dry_run, overwrite } => {
            let engine = AiksEngine::initialize(engine_config)?;
            cli::sync::run(&engine, source.clone(), *dry_run, *overwrite).await?;
        }
        Commands::Status => {
            let engine = AiksEngine::initialize(engine_config)?;
            cli::status::run(&engine).await?;
        }
        Commands::Daemon => {
            let engine = AiksEngine::initialize(engine_config)?;
            cli::daemon::run(&engine).await?;
        }
        Commands::Resync { source, session_id } => {
            let engine = AiksEngine::initialize(engine_config)?;
            cli::resync::run(&engine, source.clone(), session_id.clone()).await?;
        }
        Commands::RebuildState { yes } => {
            // B14/R01: rebuild only the sync index — knowledge data is preserved
            cli::resync::rebuild_sync_index(engine_config, *yes).await?;
        }
        Commands::ResetData { yes } => {
            // B14/R01: full destructive reset — deletes ALL data including knowledge
            cli::resync::reset_all_data(engine_config, *yes).await?;
        }
    }

    Ok(())
}
