//! Native-only process ownership. IPC never supplies executable paths or PIDs.
use std::{path::{Path,PathBuf}, process::Stdio, time::Duration};
use rand::RngCore;
use serde::Deserialize;
use serde_json::json;
use tokio::{io::{AsyncBufReadExt,AsyncReadExt,AsyncWriteExt,BufReader},process::{Child,ChildStdin,Command}};
use super::{ClientError,ClientResult,ServiceClient,ServiceConnection};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadyHandshake {
    pub api_version: u32, pub instance_id: String, pub space_id: String,
    pub boot_nonce: String, pub address: String,
}
impl ReadyHandshake {
    pub fn parse(frame: &str, expected: Option<&str>) -> ClientResult<Self> {
        if frame.len()>4096 || !frame.ends_with('\n') { return Err(ClientError::InvalidResponse); }
        let value:Self=serde_json::from_str(frame).map_err(|_|ClientError::InvalidResponse)?;
        if value.api_version!=1 || uuid::Uuid::parse_str(&value.boot_nonce).is_err()
            || !super::valid_id(&value.instance_id) || !super::valid_id(&value.space_id) {
            return Err(ClientError::InvalidResponse);
        }
        let address:std::net::SocketAddr=value.address.parse().map_err(|_|ClientError::InvalidResponse)?;
        if address.port()==0 || (address.ip()!=std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
            && address.ip()!=std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)) {
            return Err(ClientError::InvalidResponse);
        }
        if expected.is_some_and(|id|id!=value.instance_id) { return Err(ClientError::WrongInstance); }
        Ok(value)
    }
}

pub struct OwnedService { child:Child, control:Option<ChildStdin>, client:ServiceClient, nonce:String }
impl OwnedService {
    pub async fn start(binary:&Path,config:&Path,expected:Option<&str>)->ClientResult<Self> {
        for path in [binary,config] {
            let meta=std::fs::symlink_metadata(path).map_err(|_|ClientError::InvalidInput)?;
            if !path.is_absolute() || !meta.is_file() || meta.file_type().is_symlink() {return Err(ClientError::InvalidInput);}
        }
        let mut bytes=[0u8;32];
        rand::rngs::OsRng.try_fill_bytes(&mut bytes).map_err(|_|ClientError::Storage)?;
        let token:String=bytes.iter().map(|b|format!("{b:02x}")).collect();
        let mut command=Command::new(binary);
        command.arg("--config").arg(config).arg("--bootstrap-stdin")
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
        #[cfg(windows)] command.creation_flags(0x08000000);
        let mut child=command.spawn().map_err(|_|ClientError::Retryable)?;
        let mut input=child.stdin.take().ok_or(ClientError::InvalidResponse)?;
        let output=child.stdout.take().ok_or(ClientError::InvalidResponse)?;
        let result=tokio::time::timeout(Duration::from_secs(20),async {
            input.write_all(format!("{}\n",json!({"token":token,"owner_control":true,"owner_lifetime":true})).as_bytes()).await.map_err(|_|ClientError::Retryable)?;
            input.flush().await.map_err(|_|ClientError::Retryable)?;
            let mut output=BufReader::new(output.take(4097));
            let mut line=String::new();
            output.read_line(&mut line).await.map_err(|_|ClientError::InvalidResponse)?;
            let ready=ReadyHandshake::parse(&line,expected)?;
            let connection=ServiceConnection::local(&format!("http://{}",ready.address),&ready.instance_id,&ready.space_id,&token)?;
            let client=ServiceClient::new(connection)?;
            client.capabilities().await?;
            Ok::<_,ClientError>((client,ready.boot_nonce))
        }).await.map_err(|_|ClientError::Retryable).and_then(|v|v);
        match result {
            Ok((client,nonce))=>Ok(Self{child,control:Some(input),client,nonce}),
            Err(error)=>{let _=child.kill().await;let _=child.wait().await;Err(error)}
        }
    }
    pub fn client(&self)->ServiceClient{self.client.clone()}
    /// Returns whether forced termination was necessary. Never targets a foreign PID.
    pub async fn shutdown(&mut self)->ClientResult<bool>{
        if self.child.try_wait().map_err(|_|ClientError::Retryable)?.is_some(){return Ok(false);}
        if let Some(mut input)=self.control.take(){
            let command=format!("{}\n",json!({"command":"shutdown","boot_nonce":self.nonce}));
            let _=tokio::time::timeout(Duration::from_secs(2),input.write_all(command.as_bytes())).await;
        }
        match tokio::time::timeout(Duration::from_secs(12),self.child.wait()).await {
            Ok(Ok(status)) if status.success()=>Ok(false),
            Ok(_)=>Err(ClientError::Retryable),
            Err(_)=>{self.child.kill().await.map_err(|_|ClientError::Retryable)?;Ok(true)}
        }
    }
}

/// The caller selects only the trusted resource root, never a webview-supplied path.
pub fn binary_path(resource_dir:&Path)->ClientResult<PathBuf>{
    let name=if cfg!(windows){"aiks-service.exe"}else{"aiks-service"};
    #[cfg(debug_assertions)] {
        let dev=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/debug").join(name);
        // Source-harness imports use a different manifest directory. Production
        // callers still resolve through the actual desktop manifest.
        if dev.is_file(){return dev.canonicalize().map_err(|_|ClientError::InvalidInput);}
    }
    let path=resource_dir.join("service").join(name);
    path.canonicalize().map_err(|_|ClientError::InvalidInput)
}
