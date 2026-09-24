use crate::{build_router, LocalAuth, ServiceConfig, ServiceRuntime};
use aiks_core::team::IdentityProvider;
use serde::Deserialize;
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    token: String,
    #[serde(default)]
    owner_control: bool,
    #[serde(default)]
    owner_lifetime: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerCommand {
    command: String,
    boot_nonce: String,
}

pub async fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut listen = None;
    let mut bootstrap = false;
    let mut check_only = false;
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
            "--check-config" if !check_only => check_only = true,
            _ => anyhow::bail!("Unsupported argument"),
        }
    }
    let path = path.ok_or_else(|| anyhow::anyhow!("Explicit config is required"))?;
    let config_path = path.clone();
    let mut file = tokio::fs::File::open(&path).await?.take(1024 * 1024 + 1);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await?;
    anyhow::ensure!(bytes.len() <= 1024 * 1024, "Config exceeds budget");
    let mut config: ServiceConfig = toml::from_str(std::str::from_utf8(&bytes)?)?;
    if config.mode == "team" {
        anyhow::ensure!(config_path.is_absolute(), "Team config path must be absolute");
        anyhow::ensure!(listen.is_none(), "Team listen override is not supported");
    } else if let Some(listen) = listen {
        config.listen = listen.parse()?;
    }
    if check_only {
        config.check_configuration_with(|name| std::env::var(name).ok())?;
        println!("configuration_valid");
        return Ok(());
    }
    match config.mode.as_str() {
        "personal" => run_personal(config, bootstrap).await,
        "team" => run_team(config, bootstrap).await,
        _ => anyhow::bail!("Unsupported service mode"),
    }
}

async fn run_personal(mut config: ServiceConfig, bootstrap: bool) -> anyhow::Result<()> {
    anyhow::ensure!(bootstrap, "Bootstrap stdin is required");
    config.validate()?;
    config.resolve_model_credentials_with(|name| std::env::var(name).ok())?;
    config.resolve_siyuan_credentials_with(|name| std::env::var(name).ok())?;
    // Two bounded frames, bootstrap plus an explicitly opted-in owner command.
    let mut stdin = BufReader::new(tokio::io::stdin().take(8194));
    let mut frame = String::new();
    tokio::time::timeout(Duration::from_secs(5), stdin.read_line(&mut frame)).await??;
    anyhow::ensure!(
        frame.ends_with('\n') && frame.len() <= 4096,
        "Invalid bootstrap frame"
    );
    let boot: Bootstrap = serde_json::from_str(&frame)?;
    anyhow::ensure!(
        !boot.owner_lifetime || boot.owner_control,
        "Invalid owner lifetime"
    );
    LocalAuth::new(&boot.token, "pending-instance")?;
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    let address = listener.local_addr()?;
    let runtime = Arc::new(ServiceRuntime::open(config.runtime_config()).await?);
    let identity = runtime.context();
    let auth = LocalAuth::new(&boot.token, identity.instance_id())?.with_authority(address)?;
    let nonce = auth.boot_nonce().to_owned();
    let (controlled, lifetime) = (boot.owner_control, boot.owner_lifetime);
    let ready = json!({"api_version":1,"instance_id":identity.instance_id(),"space_id":identity.space_id(),"boot_nonce":nonce,"address":address.to_string()});
    drop(boot);
    drop(frame);
    let mut stdout = tokio::io::stdout();
    stdout.write_all(format!("{ready}\n").as_bytes()).await?;
    stdout.flush().await?;
    let stop = async move {
        tokio::select! {
            _ = shutdown_signal() => {},
            _ = owner_shutdown(stdin, controlled, lifetime, nonce) => {},
        }
    };
    let result = axum::serve(listener, build_router(runtime.clone(), auth))
        .with_graceful_shutdown(stop)
        .await;
    let drained = runtime.shutdown(Duration::from_secs(10)).await;
    result?;
    drained?;
    Ok(())
}

async fn run_team(mut config: ServiceConfig, bootstrap: bool) -> anyhow::Result<()> {
    use crate::team::{
        config::validate_static,
        dingtalk::DingTalkClient,
        secrets::resolve_secret,
        server::TeamServer,
    };
    anyhow::ensure!(!bootstrap, "Team mode does not use bootstrap stdin");
    config.check_configuration_with(|name| std::env::var(name).ok())?;
    let settings = validate_static(&config.team).map_err(|issues| issues[0])?;
    let secret = resolve_secret(settings.secret_source())?;
    config.resolve_model_credentials_with(|name| std::env::var(name).ok())?;
    config.resolve_siyuan_credentials_with(|name| std::env::var(name).ok())?;
    crate::team::server::validate_team_content_origin(&config)?;
    let provider: Arc<dyn IdentityProvider> = Arc::new(DingTalkClient::new(settings.clone(), secret)?);
    // Config and every secret are validated before opening the database or socket.
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    let server = TeamServer::build(&config, settings, provider)?;
    let result = axum::serve(
        listener,
        server
            .router()
            .into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await;
    let drained = server.shutdown(Duration::from_secs(10)).await;
    result?;
    drained?;
    Ok(())
}

async fn owner_shutdown<R: AsyncRead + Unpin>(
    mut stdin: BufReader<R>,
    enabled: bool,
    lifetime: bool,
    nonce: String,
) {
    if enabled {
        let mut frame = String::new();
        if let Ok(size) = stdin.read_line(&mut frame).await {
            // Only the managed sidecar opts into parent-pipe lifetime. Independent
            // services retain the original stdin-EOF-does-not-stop contract.
            if size == 0 && lifetime {
                return;
            }
            if size > 0 && size <= 4096 && frame.ends_with('\n') {
                if let Ok(command) = serde_json::from_str::<OwnerCommand>(&frame) {
                    if command.command == "shutdown" && command.boot_nonce == nonce {
                        return;
                    }
                }
            }
        }
    }
    std::future::pending::<()>().await;
}

pub(crate) async fn shutdown_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
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
