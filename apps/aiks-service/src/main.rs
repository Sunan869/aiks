#[tokio::main]
async fn main() {
    // The collector has no parent desktop subscriber. Send structured,
    // redacted operational logs to stderr so Docker captures delivery errors.
    // Never write bootstrap protocol data or session bodies to this logger.
    let filter = tracing_subscriber::EnvFilter::try_from_env("AIKS_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,aiks_service=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_target(true)
        .with_writer(std::io::stderr)
        .init();
    if let Err(error) = aiks_service::bootstrap::run().await {
        if let Some(issue) =
            error.downcast_ref::<aiks_service::model_credentials::ModelCredentialIssue>()
        {
            eprintln!("AIKS model configuration error: {issue}");
        } else if let Some(issue) = error.downcast_ref::<aiks_service::team::config::ConfigIssue>()
        {
            eprintln!("AIKS service configuration error: {issue}");
        } else {
            // Never include raw config, credentials, dependency URLs or payloads.
            eprintln!("AIKS service startup or shutdown failed");
        }
        std::process::exit(1);
    }
}
