use std::time::Duration;
use tokio::signal;
use aiks_core::{AiksEngine, SyncOptions};

pub async fn run(engine: &AiksEngine) -> anyhow::Result<()> {
    let config = engine.config();
    println!("Starting AIKS daemon (scan every {}s, watcher: {})...",
        config.sync.scan_interval_seconds, config.sync.watch_enabled);
    println!("Press Ctrl+C to stop.\n");

    // Set up file watcher
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let watcher = engine.create_watcher(event_tx);
    let _watcher_handle = if config.sync.watch_enabled {
        match watcher.start() {
            Ok(h) => { println!("File watcher active."); Some(h) }
            Err(e) => { eprintln!("Watcher error: {}. Falling back to periodic scan.", e); None }
        }
    } else {
        None
    };

    let scan_interval = Duration::from_secs(config.sync.scan_interval_seconds);
    let mut periodic = tokio::time::interval(scan_interval);
    periodic.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = periodic.tick() => {
                tracing::info!("Periodic scan triggered");
                let opts = SyncOptions { dry_run: false, overwrite: false, source_filter: None };
                match engine.sync(opts).await {
                    Ok(s) => tracing::info!(new=s.new_count, updated=s.updated_count, "Sync done"),
                    Err(e) => tracing::warn!("Sync error: {}", e),
                }
            }
            Some(event) = event_rx.recv() => {
                tracing::info!(source=?event.source, path=%event.path.display(), "File change → sync");
                let source_str = event.source.as_str().to_string();
                let opts = SyncOptions {
                    dry_run: false,
                    overwrite: false,
                    source_filter: Some(source_str),
                };
                match engine.sync(opts).await {
                    Ok(s) => tracing::debug!(new=s.new_count, updated=s.updated_count, "Event sync done"),
                    Err(e) => tracing::warn!("Event sync error: {}", e),
                }
            }
            _ = signal::ctrl_c() => {
                println!("\nShutting down...");
                break;
            }
        }
    }
    Ok(())
}
