use std::sync::Arc;
use std::time::Duration;

use tokio::signal;

use crate::config::Config;
use crate::providers::build_registry;
use crate::sink::SiYuanSink;
use crate::storage::StateDb;
use crate::sync::{SyncEngine, SyncOptions};

/// `aiks daemon` - Run as a daemon with file watching + periodic sync.
pub async fn run(config: Arc<Config>) -> anyhow::Result<()> {
    println!("Starting AIKS daemon...");
    println!(
        "Scan interval: {}s | Debounce: {}s | Watch: {}",
        config.sync.scan_interval_seconds,
        config.sync.debounce_seconds,
        config.sync.watch_enabled
    );
    println!("Press Ctrl+C to stop.\n");

    let db_path = config.state_db_path();
    let db = StateDb::open(&db_path)?;

    let sink = SiYuanSink::new(config.siyuan.clone())?;
    let engine = SyncEngine::new(config.clone());
    let opts = SyncOptions {
        dry_run: false,
        overwrite: false,
        source_filter: None,
    };

    // Initial sync
    tracing::info!("Running initial sync...");
    let registry = build_registry(&config);
    if sink.health_check().await {
        match engine.run_sync(&db, &registry, &sink, &opts).await {
            Ok(stats) => tracing::info!(
                new = stats.new_count,
                updated = stats.updated_count,
                failed = stats.failed_count,
                "Initial sync complete"
            ),
            Err(e) => tracing::warn!(error = %e, "Initial sync failed"),
        }
    } else {
        tracing::warn!("SiYuan not reachable at startup, will retry on next scan");
    }

    // Set up periodic scanner
    let scan_interval = Duration::from_secs(config.sync.scan_interval_seconds);

    loop {
        tokio::select! {
            _ = tokio::time::sleep(scan_interval) => {
                tracing::info!("Periodic scan triggered");
                let registry = build_registry(&config);
                if sink.health_check().await {
                    match engine.run_sync(&db, &registry, &sink, &opts).await {
                        Ok(stats) => tracing::info!(
                            new = stats.new_count,
                            updated = stats.updated_count,
                            failed = stats.failed_count,
                            "Periodic sync complete"
                        ),
                        Err(e) => tracing::warn!(error = %e, "Periodic sync failed"),
                    }
                } else {
                    tracing::warn!("SiYuan not reachable, skipping sync");
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
