#[tokio::main]
async fn main() {
    if let Err(error) = aiks_service::bootstrap::run().await {
        if let Some(issue) =
            error.downcast_ref::<aiks_service::model_credentials::ModelCredentialIssue>()
        {
            eprintln!("AIKS model configuration error: {issue}");
        } else {
            // Never include raw config, credentials, dependency URLs or payloads.
            eprintln!("AIKS service startup or shutdown failed");
        }
        std::process::exit(1);
    }
}
