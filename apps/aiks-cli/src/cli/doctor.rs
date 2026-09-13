use aiks_core::AiksEngine;

pub async fn run(engine: &AiksEngine) -> anyhow::Result<()> {
    println!("=== AIKS Doctor ===\n");
    let result = engine.doctor().await;
    for check in &result.checks {
        let symbol = if check.ok { "OK" } else { "WARN" };
        println!("  {:20} [{}] {}", check.name, symbol, check.message);
    }
    println!("\nAll checks completed.");
    Ok(())
}
