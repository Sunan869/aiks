use std::sync::Arc;

use crate::config::Config;
use crate::model::SourceKind;
use crate::providers::build_registry;
use crate::sink::SiYuanSink;
use crate::storage::{StateDb, SourceSessionRepo, SyncTargetRepo};
use crate::sync::{SyncEngine, SyncOptions};

/// `aiks resync` - Force re-sync sessions.
pub async fn run(
    config: Arc<Config>,
    source: Option<String>,
    session_ids: Vec<String>,
) -> anyhow::Result<()> {
    let db_path = config.state_db_path();
    let db = StateDb::open(&db_path)?;

    let session_repo = SourceSessionRepo::new(&db);
    let target_repo = SyncTargetRepo::new(&db);

    let all = session_repo.list_all()?;

    let to_resync: Vec<_> = all
        .iter()
        .filter(|s| {
            let source_matches = source
                .as_ref()
                .map(|src| s.source == *src)
                .unwrap_or(true);
            let id_matches = session_ids.is_empty()
                || session_ids.iter().any(|id| s.external_session_id == *id);
            source_matches && id_matches
        })
        .collect();

    if to_resync.is_empty() {
        println!("No matching sessions found.");
        return Ok(());
    }

    println!("Marking {} session(s) for re-sync...", to_resync.len());

    // Clear content_hash to force re-sync
    for session in &to_resync {
        // Upsert with cleared hash - this forces re-hash on next sync
        session_repo.upsert(
            &session.source,
            &session.external_session_id,
            session.source_path.as_deref(),
            session.project_path.as_deref(),
            session.project_name.as_deref(),
            session.title.as_deref(),
            session.source_updated_at.as_deref(),
            None, // Clear hash to force re-sync
            session.parser_version.as_deref(),
        )?;
        println!("  Marked: {} / {}", session.source, session.external_session_id);
    }

    println!("\nRunning sync for marked sessions...");

    let registry = build_registry(&config);
    let sink = SiYuanSink::new(config.siyuan.clone())?;

    if !sink.health_check().await {
        anyhow::bail!("SiYuan is not reachable. Please ensure SiYuan is running.");
    }

    let engine = SyncEngine::new(config.clone());
    let opts = SyncOptions {
        source_filter: source,
        dry_run: false,
        overwrite: true, // Force overwrite on explicit resync
    };

    let stats = engine.run_sync(&db, &registry, &sink, &opts).await?;

    println!("\nResync complete:");
    println!("  New:      {}", stats.new_count);
    println!("  Updated:  {}", stats.updated_count);
    println!("  Failed:   {}", stats.failed_count);

    Ok(())
}

/// `aiks rebuild-state` - Rebuild the sync state database from scratch.
pub async fn rebuild_state(config: Arc<Config>, yes: bool) -> anyhow::Result<()> {
    if !yes {
        println!("This will clear all sync state and trigger a full re-sync on next run.");
        println!("Type 'yes' to confirm: ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if input.trim() != "yes" {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let db_path = config.state_db_path();

    if db_path.exists() {
        std::fs::remove_file(&db_path)?;
        println!("Removed state database: {}", db_path.display());
    }

    // Re-create empty database
    StateDb::open(&db_path)?;
    println!("Created new state database: {}", db_path.display());
    println!("Run 'aiks sync' to perform a full re-sync.");

    Ok(())
}
