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

    let settings = match &cli.config {
        Some(path) => aiks_core::Config::from_file(path)?,
        None => aiks_core::Config::default(),
    };
    let _writer =
        aiks_core::storage::ownership::BusinessDbLease::acquire(&settings.state_db_path())?;

    match &cli.command {
        Commands::Doctor => {
            let engine = AiksEngine::initialize(engine_config)?;
            cli::doctor::run(&engine).await?;
        }
        Commands::Scan { source } => {
            let engine = AiksEngine::initialize(engine_config)?;
            cli::scan::run(&engine, source.clone()).await?;
        }
        Commands::Sync {
            source,
            dry_run,
            overwrite,
        } => {
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
            cli::resync::rebuild_sync_index(engine_config, *yes).await?;
        }
        Commands::ResetData { yes } => {
            cli::resync::reset_all_data(engine_config, *yes).await?;
        }
        Commands::SyncKnowledge {
            overwrite_conflicts,
        } => {
            let engine = AiksEngine::initialize(engine_config)?;
            let stats = engine
                .sync_knowledge_to_siyuan(*overwrite_conflicts)
                .await?;
            println!(
                "Knowledge sync complete: created={} updated={} unchanged={} conflict={} failed={}",
                stats.created, stats.updated, stats.unchanged, stats.conflict, stats.failed
            );
        }
    }
    Ok(())
}
