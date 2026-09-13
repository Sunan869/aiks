use std::sync::Arc;

use crate::config::Config;
use crate::storage::{StateDb, SourceSessionRepo, SyncRunRepo, SyncTargetRepo};

/// `aiks status` - Show sync status.
pub async fn run(config: Arc<Config>) -> anyhow::Result<()> {
    let db_path = config.state_db_path();
    let db = StateDb::open(&db_path)?;

    let run_repo = SyncRunRepo::new(&db);
    let session_repo = SourceSessionRepo::new(&db);
    let target_repo = SyncTargetRepo::new(&db);

    // Last sync run
    println!("=== Last Sync Run ===");
    match run_repo.last_run()? {
        Some(run) => {
            println!("  Started:     {}", run.started_at);
            if let Some(finished) = &run.finished_at {
                println!("  Finished:    {}", finished);
            } else {
                println!("  Finished:    (in progress)");
            }
            println!("  Trigger:     {}", run.trigger_type.as_deref().unwrap_or("unknown"));
            println!("  Discovered:  {}", run.discovered);
            println!("  Changed:     {}", run.changed);
            println!("  Synced:      {}", run.synced);
            println!("  Failed:      {}", run.failed);
        }
        None => println!("  No sync run recorded yet."),
    }

    // Session counts
    println!("\n=== Session State ===");
    let all_sessions = session_repo.list_all()?;

    let by_source = {
        let mut map: std::collections::BTreeMap<String, Vec<_>> = std::collections::BTreeMap::new();
        for s in &all_sessions {
            map.entry(s.source.clone()).or_default().push(s);
        }
        map
    };

    for (source, sessions) in &by_source {
        let missing = sessions.iter().filter(|s| s.is_missing).count();
        println!(
            "  {} - {} sessions ({} missing)",
            source,
            sessions.len(),
            missing
        );
    }

    // Pending / failed syncs
    println!("\n=== Pending Sync Targets ===");
    let pending = target_repo.list_pending("siyuan")?;
    if pending.is_empty() {
        println!("  None");
    } else {
        println!("  {} pending sync(s):", pending.len());
        for target in &pending {
            println!(
                "  [{}] session_id={} error={}",
                target.status.as_str(),
                target.session_id,
                target.last_error.as_deref().unwrap_or("-")
            );
        }
    }

    println!("\nTotal tracked sessions: {}", all_sessions.len());

    Ok(())
}
