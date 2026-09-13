use aiks_core::{AiksEngine, AiksEngineConfig};
use aiks_core::storage::rebuild_sync_index_only;

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
    
    // B22: Reset hashes using source + session_id (not just session_id)
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

/// B14: Rebuild only the sync index — preserves all V3 knowledge data.
///
/// This resets: sync_target, sync_run, source_file_state, content_hash
/// This preserves: knowledge_item, knowledge_chunk, embedding_record, pipeline_run
pub async fn rebuild_sync_index(engine_config: AiksEngineConfig, yes: bool) -> anyhow::Result<()> {
    if !yes {
        println!("This will reset the sync index (knowledge data is preserved).");
        println!("Type 'yes' to confirm: ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if input.trim() != "yes" { println!("Cancelled."); return Ok(()); }
    }
    let config = aiks_core::Config::default();
    let db_path = config.state_db_path();
    let db = aiks_core::storage::StateDb::open(&db_path)?;
    rebuild_sync_index_only(&db)?;
    println!("Sync index rebuilt at {}. Knowledge data preserved.", db_path.display());
    Ok(())
}

/// B14: Full reset — deletes ALL data including knowledge (high danger, requires explicit flag).
pub async fn reset_all_data(engine_config: AiksEngineConfig, yes: bool) -> anyhow::Result<()> {
    if !yes {
        println!("WARNING: This will delete ALL data including knowledge items.");
        println!("Type 'DELETE ALL' to confirm: ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if input.trim() != "DELETE ALL" { println!("Cancelled."); return Ok(()); }
    }
    let config = aiks_core::Config::default();
    let db_path = config.state_db_path();
    if db_path.exists() { std::fs::remove_file(&db_path)?; }
    aiks_core::storage::StateDb::open(&db_path)?;
    println!("All data reset at {}", db_path.display());
    Ok(())
}
