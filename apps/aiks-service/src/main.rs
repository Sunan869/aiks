#[tokio::main]
async fn main() {
    if aiks_service::bootstrap::run().await.is_err() {
        // Never include raw config, credentials, dependency URLs or payloads.
        eprintln!("AIKS service startup or shutdown failed");
        std::process::exit(1);
    }
}
