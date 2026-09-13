use std::sync::Arc;

use crate::config::Config;
use crate::providers::build_registry;
use crate::sink::SiYuanSink;
use crate::storage::StateDb;

/// `aiks doctor` - Check system health.
pub async fn run(config: Arc<Config>) -> anyhow::Result<()> {
    println!("=== AIKS Doctor ===\n");

    // 1. State database
    let db_path = config.state_db_path();
    print!("State DB: {} ... ", db_path.display());
    match StateDb::open(&db_path) {
        Ok(_) => println!("OK"),
        Err(e) => println!("FAIL: {}", e),
    }

    // 2. Providers
    println!("\nProviders:");
    let registry = build_registry(&config);
    let health_results = registry.health_check_all().await;

    for (source, health) in &health_results {
        let status = if health.is_ok() { "OK" } else { "WARN" };
        println!("  {:12} [{}] {}", source.display_name(), status, health.message());
    }

    // 3. SiYuan
    println!("\nSiYuan:");
    print!("  Connection to {} ... ", config.siyuan.base_url);
    let siyuan = SiYuanSink::new(config.siyuan.clone())?;
    if siyuan.health_check().await {
        println!("OK");

        // Try to find/create notebook
        print!("  Notebook '{}' ... ", config.siyuan.notebook_name);
        match siyuan.ensure_notebook().await {
            Ok(id) => println!("OK (id: {})", id),
            Err(e) => println!("WARN: {}", e),
        }
    } else {
        println!("FAIL (not reachable)");
        println!("  Hint: Make sure SiYuan is running with HTTP API enabled.");
        if config.siyuan.token.is_empty() {
            println!("  Hint: Set SIYUAN_TOKEN env var or token in config.toml");
        }
    }

    println!("\nAll checks completed.");
    Ok(())
}
