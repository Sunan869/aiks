use aiks_core::{AiksEngine, AiksEngineConfig};

pub async fn run(engine: &AiksEngine, source: Option<String>, session_ids: Vec<String>) -> anyhow::Result<()> {
    let all = engine.db().get_all_sessions_raw()?;
    let to_resync: Vec<_> = all.iter().filter(|s| {
        source.as_ref().map(|src| s.source == *src).unwrap_or(true)
            && (session_ids.is_empty() || session_ids.contains(&s.external_session_id))
    }).collect();
    
    println!("Marking {} session(s) for re-sync...", to_resync.len());
    for s in &to_resync {
        println!("  {} / {}", s.source, s.external_session_id);
    }
    
    // Reset hashes and sync
    engine.db().reset_hashes_for_resync(source.as_deref(), &session_ids)?;
    
    let opts = aiks_core::SyncOptions {
        source_filter: source,
        dry_run: false,
        overwrite: true,
    };
    let stats = engine.sync(opts).await?;
    println!("\nResync complete: new={} updated={} failed={}", 
        stats.new_count, stats.updated_count, stats.failed_count);
    Ok(())
}

pub async fn rebuild_state(engine_config: AiksEngineConfig, yes: bool) -> anyhow::Result<()> {
    if !yes {
        println!("This will clear all sync state. Type 'yes' to confirm: ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if input.trim() != "yes" { println!("Cancelled."); return Ok(()); }
    }
    let config = aiks_core::Config::default();
    let db_path = config.state_db_path();
    if db_path.exists() { std::fs::remove_file(&db_path)?; }
    aiks_core::storage::StateDb::open(&db_path)?;
    println!("State rebuilt at {}", db_path.display());
    Ok(())
}
