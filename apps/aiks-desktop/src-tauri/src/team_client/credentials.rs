use super::connection::{valid_token, TeamIdentity};
use crate::service_client::{valid_id, ClientError, ClientResult};
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredCredential {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
    pub identity: TeamIdentity,
}
impl StoredCredential {
    pub fn validate(&self) -> ClientResult<()> {
        if !valid_token(&self.access_token)
            || !valid_token(&self.refresh_token)
            || self.expires_at == 0
        {
            return Err(ClientError::Storage);
        }
        self.identity.validate().map_err(|_| ClientError::Storage)
    }
}

pub struct CredentialStore {
    root: PathBuf,
    memory: Mutex<HashMap<String, StoredCredential>>,
}
impl CredentialStore {
    pub fn new(root: PathBuf) -> ClientResult<Self> {
        if root.as_os_str().is_empty() {
            return Err(ClientError::InvalidInput);
        }
        std::fs::create_dir_all(&root).map_err(|_| ClientError::Storage)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700));
        }
        Ok(Self {
            root,
            memory: Mutex::new(HashMap::new()),
        })
    }
    pub fn store(
        &self,
        connection_id: &str,
        user_id: &str,
        value: &StoredCredential,
    ) -> ClientResult<()> {
        validate_key(connection_id, user_id)?;
        value.validate()?;
        let bytes = serde_json::to_vec(value).map_err(|_| ClientError::Storage)?;
        let persistent =
            platform_store(&self.root, connection_id, user_id, &bytes).unwrap_or(false);
        let mut memory = self.memory.lock().map_err(|_| ClientError::Storage)?;
        memory.insert(key(connection_id, user_id), value.clone());
        if !persistent {
            tracing::info!("Team credential secure storage unavailable; using memory-only session");
        }
        Ok(())
    }
    pub fn load(
        &self,
        connection_id: &str,
        user_id: &str,
    ) -> ClientResult<Option<StoredCredential>> {
        validate_key(connection_id, user_id)?;
        if let Some(value) = self
            .memory
            .lock()
            .map_err(|_| ClientError::Storage)?
            .get(&key(connection_id, user_id))
            .cloned()
        {
            value.validate()?;
            return Ok(Some(value));
        }
        let Some(bytes) = platform_load(&self.root, connection_id, user_id)? else {
            return Ok(None);
        };
        if bytes.len() > 64 * 1024 {
            return Err(ClientError::Storage);
        }
        let value: StoredCredential =
            serde_json::from_slice(&bytes).map_err(|_| ClientError::Storage)?;
        value.validate()?;
        self.memory
            .lock()
            .map_err(|_| ClientError::Storage)?
            .insert(key(connection_id, user_id), value.clone());
        Ok(Some(value))
    }
    pub fn delete(&self, connection_id: &str, user_id: &str) -> ClientResult<()> {
        validate_key(connection_id, user_id)?;
        self.memory
            .lock()
            .map_err(|_| ClientError::Storage)?
            .remove(&key(connection_id, user_id));
        let _ = platform_delete(&self.root, connection_id, user_id);
        Ok(())
    }
}

fn validate_key(connection: &str, user: &str) -> ClientResult<()> {
    if !valid_id(connection) || !valid_id(user) {
        return Err(ClientError::InvalidInput);
    }
    Ok(())
}
fn key(connection: &str, user: &str) -> String {
    format!("{connection}\u{1f}{user}")
}
#[cfg(target_os = "windows")]
fn file_name(connection: &str, user: &str) -> String {
    hex::encode(Sha256::digest(key(connection, user).as_bytes())) + ".bin"
}

#[cfg(target_os = "linux")]
fn platform_store(_: &Path, connection: &str, user: &str, bytes: &[u8]) -> ClientResult<bool> {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        return Ok(false);
    }
    let mut child = match Command::new("secret-tool")
        .args([
            "store",
            "--label=AIKS Team Session",
            "service",
            "aiks-team",
            "connection",
            connection,
            "user",
            user,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return Ok(false),
    };
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(bytes).map_err(|_| ClientError::Storage)?;
    }
    Ok(child.wait().map(|s| s.success()).unwrap_or(false))
}
#[cfg(target_os = "linux")]
fn platform_load(_: &Path, connection: &str, user: &str) -> ClientResult<Option<Vec<u8>>> {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        return Ok(None);
    }
    let output = match Command::new("secret-tool")
        .args([
            "lookup",
            "service",
            "aiks-team",
            "connection",
            connection,
            "user",
            user,
        ])
        .stderr(Stdio::null())
        .output()
    {
        Ok(value) if value.status.success() => value.stdout,
        _ => return Ok(None),
    };
    let mut output = output;
    while matches!(output.last(), Some(b'\n' | b'\r')) {
        output.pop();
    }
    Ok(Some(output))
}
#[cfg(target_os = "linux")]
fn platform_delete(_: &Path, connection: &str, user: &str) -> ClientResult<()> {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some() {
        let _ = Command::new("secret-tool")
            .args([
                "clear",
                "service",
                "aiks-team",
                "connection",
                connection,
                "user",
                user,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn platform_store(root: &Path, connection: &str, user: &str, bytes: &[u8]) -> ClientResult<bool> {
    let path = root.join(file_name(connection, user));
    let script = r#"Add-Type -AssemblyName System.Security;$d=[Console]::In.ReadToEnd();$b=[Text.Encoding]::UTF8.GetBytes($d);$e=[Security.Cryptography.ProtectedData]::Protect($b,$null,[Security.Cryptography.DataProtectionScope]::CurrentUser);[IO.File]::WriteAllBytes($args[0],$e)"#;
    let mut child = match Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return Ok(false),
    };
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(bytes).map_err(|_| ClientError::Storage)?;
    }
    Ok(child.wait().map(|s| s.success()).unwrap_or(false))
}
#[cfg(target_os = "windows")]
fn platform_load(root: &Path, connection: &str, user: &str) -> ClientResult<Option<Vec<u8>>> {
    use base64::Engine;
    let path = root.join(file_name(connection, user));
    if !path.exists() {
        return Ok(None);
    }
    let script = r#"Add-Type -AssemblyName System.Security;$e=[IO.File]::ReadAllBytes($args[0]);$b=[Security.Cryptography.ProtectedData]::Unprotect($e,$null,[Security.Cryptography.DataProtectionScope]::CurrentUser);[Console]::Out.Write([Convert]::ToBase64String($b))"#;
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .arg(&path)
        .stderr(Stdio::null())
        .output()
        .map_err(|_| ClientError::Storage)?;
    if !output.status.success() {
        return Err(ClientError::Storage);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(output.stdout)
        .map_err(|_| ClientError::Storage)?;
    Ok(Some(bytes))
}
#[cfg(target_os = "windows")]
fn platform_delete(root: &Path, connection: &str, user: &str) -> ClientResult<()> {
    let path = root.join(file_name(connection, user));
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ClientError::Storage),
    }
}

#[cfg(target_os = "macos")]
fn platform_store(_: &Path, connection: &str, user: &str, bytes: &[u8]) -> ClientResult<bool> {
    let account = key(connection, user);
    security_framework::passwords::set_generic_password("com.sunan869.aiks.team", &account, bytes)
        .map(|_| true)
        .map_err(|_| ClientError::Storage)
}
#[cfg(target_os = "macos")]
fn platform_load(_: &Path, connection: &str, user: &str) -> ClientResult<Option<Vec<u8>>> {
    let account = key(connection, user);
    match security_framework::passwords::get_generic_password("com.sunan869.aiks.team", &account) {
        Ok(value) => Ok(Some(value)),
        Err(_) => Ok(None),
    }
}
#[cfg(target_os = "macos")]
fn platform_delete(_: &Path, connection: &str, user: &str) -> ClientResult<()> {
    let account = key(connection, user);
    let _ =
        security_framework::passwords::delete_generic_password("com.sunan869.aiks.team", &account);
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn platform_store(_: &Path, _: &str, _: &str, _: &[u8]) -> ClientResult<bool> {
    Ok(false)
}
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn platform_load(_: &Path, _: &str, _: &str) -> ClientResult<Option<Vec<u8>>> {
    Ok(None)
}
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn platform_delete(_: &Path, _: &str, _: &str) -> ClientResult<()> {
    Ok(())
}
