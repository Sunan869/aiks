use crate::{build_router, LocalAuth, ServiceConfig, ServiceRuntime};
use serde::Deserialize;
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    token: String,
}

pub async fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut listen = None;
    let mut bootstrap = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" if path.is_none() => {
                path = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("Missing config"))?,
                ))
            }
            "--listen" if listen.is_none() => {
                listen = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("Missing listen address"))?,
                )
            }
            "--bootstrap-stdin" if !bootstrap => bootstrap = true,
            _ => anyhow::bail!("Unsupported argument"),
        }
    }
    anyhow::ensure!(bootstrap, "Bootstrap stdin is required");
    let path = path.ok_or_else(|| anyhow::anyhow!("Explicit config is required"))?;
    let mut file = tokio::fs::File::open(path).await?.take(1024 * 1024 + 1);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await?;
    anyhow::ensure!(bytes.len() <= 1024 * 1024, "Config exceeds budget");
    let mut config: ServiceConfig = toml::from_str(std::str::from_utf8(&bytes)?)?;
    if let Some(listen) = listen {
        config.listen = listen.parse()?;
    }
    config.validate()?;
    let mut stdin = BufReader::new(tokio::io::stdin().take(4097));
    let mut frame = String::new();
    tokio::time::timeout(Duration::from_secs(5), stdin.read_line(&mut frame)).await??;
    anyhow::ensure!(
        frame.ends_with('\n') && frame.len() <= 4096,
        "Invalid bootstrap frame"
    );
    let boot: Bootstrap = serde_json::from_str(&frame)?;
    // Validate credentials before acquiring the database or starting work.
    LocalAuth::new(&boot.token, "pending-instance")?;
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    let address = listener.local_addr()?;
    let runtime = Arc::new(ServiceRuntime::open(config.runtime_config()).await?);
    let identity = runtime.context();
    let auth = LocalAuth::new(&boot.token, identity.instance_id())?.with_authority(address)?;
    let ready = json!({"api_version":1,"instance_id":identity.instance_id(),"boot_nonce":auth.boot_nonce(),"address":address.to_string()});
    drop(boot);
    drop(frame);
    drop(stdin);
    let mut stdout = tokio::io::stdout();
    stdout.write_all(format!("{ready}\n").as_bytes()).await?;
    stdout.flush().await?;
    // Client pipe EOF is intentionally not a shutdown signal.
    let server = axum::serve(listener, build_router(runtime.clone(), auth))
        .with_graceful_shutdown(shutdown_signal());
    let result = server.await;
    let drained = runtime.shutdown(Duration::from_secs(10)).await;
    result?;
    drained?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}}
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
