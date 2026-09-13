use aiks_core::AiksEngine;

pub async fn run(engine: &AiksEngine, source: Option<String>) -> anyhow::Result<()> {
    let result = engine.scan(source.as_deref()).await;
    println!("Discovered {} sessions:\n", result.total);
    for (src, count) in &result.by_source {
        println!("  {} — {} sessions", src, count);
    }
    println!();
    for s in &result.summaries {
        let title = s.title.as_deref().unwrap_or("(untitled)");
        let updated = s.updated_at
            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        println!("  {} | {} msgs | {} | {}", 
            &s.external_session_id[..s.external_session_id.len().min(8)],
            s.message_count, updated, title);
    }
    Ok(())
}
