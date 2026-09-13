use aiks_core::{AiksEngine, SyncOptions};

pub async fn run(engine: &AiksEngine, source: Option<String>, dry_run: bool, overwrite: bool) -> anyhow::Result<()> {
    if dry_run {
        println!("DRY RUN mode — scanning and computing changes (no SiYuan writes)\n");
        // dry_run: discover + hash check only, no SiYuan required
        let scan = engine.scan(source.as_deref()).await;
        println!("Discovered {} sessions", scan.total);
        println!("(dry-run: actual change detection requires state DB comparison)");
        return Ok(());
    }

    println!("Starting sync...");
    let opts = SyncOptions {
        source_filter: source,
        dry_run,
        overwrite,
    };
    let stats = engine.sync(opts).await?;
    println!("\nSync complete:");
    println!("  Discovered:  {}", stats.discovered);
    println!("  New:         {}", stats.new_count);
    println!("  Updated:     {}", stats.updated_count);
    println!("  Unchanged:   {}", stats.unchanged_count);
    println!("  Skipped:     {}", stats.skipped_count);
    println!("  Conflicts:   {}", stats.conflict_count);
    println!("  Failed:      {}", stats.failed_count);
    Ok(())
}
