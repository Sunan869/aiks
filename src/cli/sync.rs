use std::sync::Arc;

use crate::config::Config;
use crate::providers::build_registry;
use crate::sink::SiYuanSink;
use crate::storage::StateDb;
use crate::sync::{SyncEngine, SyncOptions};

/// `aiks sync` - Sync sessions to SiYuan.
pub async fn run(
    config: Arc<Config>,
    source: Option<String>,
    dry_run: bool,
    overwrite: bool,
) -> anyhow::Result<()> {
    if dry_run {
        println!("DRY RUN mode - no changes will be made\n");
    }

    let db_path = config.state_db_path();
    let db = StateDb::open(&db_path)?;

    let registry = build_registry(&config);
    let sink = SiYuanSink::new(config.siyuan.clone())?;

    // Check SiYuan health before syncing
    if !sink.health_check().await {
        anyhow::bail!(
            "SiYuan is not reachable at {}. Please ensure SiYuan is running with HTTP API enabled.",
            config.siyuan.base_url
        );
    }

    let engine = SyncEngine::new(config.clone());
    let opts = SyncOptions {
        source_filter: source,
        dry_run,
        overwrite,
    };

    println!("Starting sync...");
    let stats = engine.run_sync(&db, &registry, &sink, &opts).await?;

    println!("\nSync complete:");
    println!("  Discovered:  {}", stats.discovered);
    println!("  New:         {}", stats.new_count);
    println!("  Updated:     {}", stats.updated_count);
    println!("  Unchanged:   {}", stats.unchanged_count);
    println!("  Skipped:     {}", stats.skipped_count);
    println!("  Conflicts:   {}", stats.conflict_count);
    println!("  Failed:      {}", stats.failed_count);

    if stats.conflict_count > 0 {
        println!(
            "\nNote: {} conflict(s) detected. Use --overwrite to force sync.",
            stats.conflict_count
        );
    }

    if stats.failed_count > 0 {
        println!(
            "\nWarning: {} session(s) failed to sync. Run 'aiks status' for details.",
            stats.failed_count
        );
    }

    Ok(())
}
