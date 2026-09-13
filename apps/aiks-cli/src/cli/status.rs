use aiks_core::AiksEngine;

pub async fn run(engine: &AiksEngine) -> anyhow::Result<()> {
    let status = engine.status()?;
    println!("=== AIKS Status ===");
    println!("  Total sessions: {}", status.total_sessions);
    println!("  Synced:         {}", status.synced);
    println!("  Pending:        {}", status.pending);
    println!("  Conflict:       {}", status.conflict);
    println!("  Failed:         {}", status.failed);
    if let Some(at) = &status.last_sync_at {
        println!("  Last sync: {}", at);
        println!("    Discovered: {}  Changed: {}  Synced: {}  Failed: {}",
            status.last_sync_discovered, status.last_sync_changed,
            status.last_sync_synced, status.last_sync_failed);
    } else {
        println!("  No sync run recorded yet.");
    }
    Ok(())
}
