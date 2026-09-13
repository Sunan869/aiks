use std::sync::Arc;

use crate::config::Config;
use crate::providers::build_registry;

/// `aiks scan` - Discover sessions without syncing.
pub async fn run(config: Arc<Config>, source: Option<String>) -> anyhow::Result<()> {
    let registry = build_registry(&config);
    let summaries = registry.discover_all().await;

    let summaries: Vec<_> = if let Some(src) = &source {
        summaries
            .into_iter()
            .filter(|s| s.source.as_str() == src.as_str())
            .collect()
    } else {
        summaries
    };

    println!("Discovered {} sessions:\n", summaries.len());

    // Group by source
    let mut by_source: std::collections::BTreeMap<String, Vec<_>> = std::collections::BTreeMap::new();
    for s in summaries {
        by_source
            .entry(s.source.display_name().to_string())
            .or_default()
            .push(s);
    }

    for (source_name, sessions) in &by_source {
        println!("[{}] {} sessions", source_name, sessions.len());
        for sess in sessions {
            let title = sess.title.as_deref().unwrap_or("(untitled)");
            let project = sess.project_path.as_deref().unwrap_or("");
            let updated = sess
                .updated_at
                .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            println!(
                "  {} | {} msgs | {} | {}",
                &sess.external_session_id[..sess.external_session_id.len().min(8)],
                sess.message_count,
                updated,
                title
            );
        }
        println!();
    }

    Ok(())
}
