pub mod connection;
pub mod credentials;
pub mod login;

use connection::{service_client, ConnectionRecord};
use credentials::{CredentialStore, StoredCredential};
use login::{verifier, PendingLogin, ReqwestTeamAuthTransport, TeamAuthTransport};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, RwLock};

use crate::service_client::{ClientError, ClientResult, ServiceClient};

#[derive(Clone, Debug, Serialize)]
pub struct TeamConnectionStatus {
    pub connection_id: String,
    pub origin: String,
    pub state: &'static str,
    pub instance_id: Option<String>,
    pub company_id: Option<String>,
    pub user_id: Option<String>,
    pub space_id: Option<String>,
    pub display_name: Option<String>,
}

pub struct BrowserHandoff {
    pub connection_id: String,
    pub url: String,
}

pub struct TeamClientManager {
    records: RwLock<BTreeMap<String, ConnectionRecord>>,
    pending: Mutex<BTreeMap<String, PendingLogin>>,
    credentials: Arc<CredentialStore>,
    transport: Arc<dyn TeamAuthTransport>,
    refresh_gate: Mutex<()>,
    metadata_path: PathBuf,
}
impl TeamClientManager {
    pub fn open(root: PathBuf) -> ClientResult<Self> {
        Self::with_transport(root, Arc::new(ReqwestTeamAuthTransport::new()?))
    }
    pub fn with_transport(
        root: PathBuf,
        transport: Arc<dyn TeamAuthTransport>,
    ) -> ClientResult<Self> {
        std::fs::create_dir_all(&root).map_err(|_| ClientError::Storage)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700));
        }
        let metadata_path = root.join("connections.json");
        let records = read_records(&metadata_path)?;
        let credentials = Arc::new(CredentialStore::new(root.join("credentials"))?);
        Ok(Self {
            records: RwLock::new(records),
            pending: Mutex::new(BTreeMap::new()),
            credentials,
            transport,
            refresh_gate: Mutex::new(()),
            metadata_path,
        })
    }

    pub async fn add_connection(&self, origin: &str) -> ClientResult<TeamConnectionStatus> {
        let record = ConnectionRecord::new(origin)?;
        let status = status(&record);
        let mut records = self.records.write().await;
        records.insert(record.connection_id.clone(), record);
        persist_records(&self.metadata_path, &records)?;
        Ok(status)
    }

    pub async fn begin_login(&self, connection_id: &str) -> ClientResult<BrowserHandoff> {
        let record = self.record(connection_id).await?;
        let endpoint = record.endpoint()?;
        let (verifier, hash) = verifier()?;
        let start = self.transport.start(&endpoint, &hash).await?;
        endpoint.validate_browser_handoff(&start.authorize_url)?;
        let pending = PendingLogin {
            attempt_id: start.attempt_id,
            verifier,
            authorize_url: start.authorize_url.clone(),
        };
        self.pending
            .lock()
            .await
            .insert(connection_id.to_owned(), pending);
        Ok(BrowserHandoff {
            connection_id: connection_id.to_owned(),
            url: start.authorize_url,
        })
    }

    pub async fn finish_login(&self, connection_id: &str) -> ClientResult<TeamConnectionStatus> {
        let _refresh_guard = self.refresh_gate.lock().await;
        let record = self.record(connection_id).await?;
        let endpoint = record.endpoint()?;
        let pending = self
            .pending
            .lock()
            .await
            .get(connection_id)
            .cloned()
            .ok_or(ClientError::LoginPending)?;
        let tokens = self
            .transport
            .exchange(&endpoint, &pending.attempt_id, pending.verifier())
            .await?;
        tokens.validate()?;
        let expires_at = now()?
            .checked_add(tokens.expires_in)
            .ok_or(ClientError::InvalidResponse)?;
        let stored = StoredCredential {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at,
            identity: tokens.identity.clone(),
        };
        let credentials = self.credentials.clone();
        let connection = connection_id.to_owned();
        let user = tokens.identity.user_id.clone();
        let saved = stored.clone();
        tokio::task::spawn_blocking(move || credentials.store(&connection, &user, &saved))
            .await
            .map_err(|_| ClientError::Storage)??;
        let mut records = self.records.write().await;
        let target = records
            .get_mut(connection_id)
            .ok_or(ClientError::NotFound)?;
        target.active_identity = Some(tokens.identity);
        persist_records(&self.metadata_path, &records)?;
        self.pending.lock().await.remove(connection_id);
        Ok(status(target))
    }

    pub async fn logout(&self, connection_id: &str) -> ClientResult<()> {
        let _refresh_guard = self.refresh_gate.lock().await;
        let record = self.record(connection_id).await?;
        self.pending.lock().await.remove(connection_id);
        if let Some(identity) = record.active_identity.clone() {
            let credentials = self.credentials.clone();
            let connection = connection_id.to_owned();
            let user = identity.user_id.clone();
            let stored = tokio::task::spawn_blocking({
                let credentials = credentials.clone();
                let connection = connection.clone();
                let user = user.clone();
                move || credentials.load(&connection, &user)
            })
            .await
            .map_err(|_| ClientError::Storage)??;
            if let Some(stored) = stored {
                let _ = self
                    .transport
                    .logout(&record.endpoint()?, &stored.access_token)
                    .await;
            }
            tokio::task::spawn_blocking(move || credentials.delete(&connection, &user))
                .await
                .map_err(|_| ClientError::Storage)??;
        }
        let mut records = self.records.write().await;
        let target = records
            .get_mut(connection_id)
            .ok_or(ClientError::NotFound)?;
        target.active_identity = None;
        persist_records(&self.metadata_path, &records)?;
        Ok(())
    }

    pub async fn statuses(&self) -> Vec<TeamConnectionStatus> {
        let records = self
            .records
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut result = Vec::with_capacity(records.len());
        for record in records {
            let mut current = status(&record);
            if let Some(identity) = &record.active_identity {
                let credentials = self.credentials.clone();
                let connection = record.connection_id.clone();
                let user = identity.user_id.clone();
                let available =
                    tokio::task::spawn_blocking(move || credentials.load(&connection, &user))
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .flatten()
                        .is_some();
                if !available {
                    current.state = "credentials_missing";
                }
            }
            result.push(current);
        }
        result
    }

    pub async fn client(&self, connection_id: &str) -> ClientResult<ServiceClient> {
        let _refresh_guard = self.refresh_gate.lock().await;
        let record = self.record(connection_id).await?;
        let identity = record
            .active_identity
            .clone()
            .ok_or(ClientError::Unauthorized)?;
        let credentials = self.credentials.clone();
        let connection = connection_id.to_owned();
        let user = identity.user_id.clone();
        let mut stored = tokio::task::spawn_blocking(move || credentials.load(&connection, &user))
            .await
            .map_err(|_| ClientError::Storage)??
            .ok_or(ClientError::Unauthorized)?;
        if stored.identity != identity {
            return Err(ClientError::Unauthorized);
        }
        if stored.expires_at <= now()?.saturating_add(60) {
            let refreshed = self
                .transport
                .refresh(&record.endpoint()?, &stored.refresh_token)
                .await?;
            refreshed.validate()?;
            if refreshed.identity != identity {
                return Err(ClientError::Unauthorized);
            }
            stored = StoredCredential {
                access_token: refreshed.access_token,
                refresh_token: refreshed.refresh_token,
                expires_at: now()?
                    .checked_add(refreshed.expires_in)
                    .ok_or(ClientError::InvalidResponse)?,
                identity: refreshed.identity,
            };
            let credentials = self.credentials.clone();
            let connection = connection_id.to_owned();
            let user = identity.user_id.clone();
            let saved = stored.clone();
            tokio::task::spawn_blocking(move || credentials.store(&connection, &user, &saved))
                .await
                .map_err(|_| ClientError::Storage)??;
        }
        service_client(&record, &identity, &stored.access_token)
    }

    pub async fn active_target(
        &self,
        connection_id: &str,
    ) -> ClientResult<crate::service_client::TargetIdentity> {
        self.record(connection_id)
            .await?
            .active_identity
            .ok_or(ClientError::Unauthorized)?
            .target()
    }

    async fn record(&self, connection_id: &str) -> ClientResult<ConnectionRecord> {
        if !crate::service_client::valid_id(connection_id) {
            return Err(ClientError::InvalidInput);
        }
        self.records
            .read()
            .await
            .get(connection_id)
            .cloned()
            .ok_or(ClientError::NotFound)
    }
}

fn status(record: &ConnectionRecord) -> TeamConnectionStatus {
    let identity = record.active_identity.as_ref();
    TeamConnectionStatus {
        connection_id: record.connection_id.clone(),
        origin: record.origin.clone(),
        state: if identity.is_some() {
            "signed_in"
        } else {
            "signed_out"
        },
        instance_id: identity.map(|v| v.instance_id.clone()),
        company_id: identity.map(|v| v.company_id.clone()),
        user_id: identity.map(|v| v.user_id.clone()),
        space_id: identity.map(|v| v.space_id.clone()),
        display_name: identity.map(|v| v.display_name.clone()),
    }
}

fn read_records(path: &Path) -> ClientResult<BTreeMap<String, ConnectionRecord>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let meta = std::fs::symlink_metadata(path).map_err(|_| ClientError::Storage)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 1024 * 1024 {
        return Err(ClientError::Storage);
    }
    let rows: Vec<ConnectionRecord> =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| ClientError::Storage)?)
            .map_err(|_| ClientError::Storage)?;
    if rows.len() > 64 {
        return Err(ClientError::Storage);
    }
    let mut result = BTreeMap::new();
    for row in rows {
        row.validate().map_err(|_| ClientError::Storage)?;
        if result.insert(row.connection_id.clone(), row).is_some() {
            return Err(ClientError::Storage);
        }
    }
    Ok(result)
}

fn persist_records(path: &Path, records: &BTreeMap<String, ConnectionRecord>) -> ClientResult<()> {
    if records.len() > 64 {
        return Err(ClientError::TooLarge);
    }
    let bytes = serde_json::to_vec_pretty(&records.values().collect::<Vec<_>>())
        .map_err(|_| ClientError::Storage)?;
    if bytes.len() > 1024 * 1024 {
        return Err(ClientError::TooLarge);
    }
    let parent = path.parent().ok_or(ClientError::Storage)?;
    std::fs::create_dir_all(parent).map_err(|_| ClientError::Storage)?;
    let temp = parent.join(format!(".connections-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(&temp).map_err(|_| ClientError::Storage)?;
    file.write_all(&bytes).map_err(|_| ClientError::Storage)?;
    file.sync_all().map_err(|_| ClientError::Storage)?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|_| ClientError::Storage)?;
    }
    std::fs::rename(&temp, path).map_err(|_| ClientError::Storage)?;
    Ok(())
}

fn now() -> ClientResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .map_err(|_| ClientError::Storage)
}

fn trusted(view: &tauri::Webview) -> Result<(), String> {
    let url = view.url().map_err(|_| "untrusted_shell")?;
    let packaged = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"));
    let dev = cfg!(debug_assertions)
        && url.scheme() == "http"
        && url.host_str() == Some("localhost")
        && url.port() == Some(1420);
    if view.label() != "control" || !(packaged || dev) {
        return Err("untrusted_shell".into());
    }
    Ok(())
}
fn managed(app: &tauri::AppHandle) -> Result<Arc<TeamClientManager>, String> {
    use tauri::Manager;
    app.try_state::<Arc<TeamClientManager>>()
        .map(|state| state.inner().clone())
        .ok_or("team_client_not_ready".into())
}

#[tauri::command]
pub async fn team_add_connection(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    origin: String,
) -> Result<TeamConnectionStatus, String> {
    trusted(&webview)?;
    managed(&app)?
        .add_connection(&origin)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn team_begin_login(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    connection_id: String,
) -> Result<serde_json::Value, String> {
    use tauri_plugin_shell::ShellExt;
    trusted(&webview)?;
    let handoff = managed(&app)?
        .begin_login(&connection_id)
        .await
        .map_err(|e| e.to_string())?;
    app.shell()
        .open(&handoff.url, None)
        .map_err(|_| "browser_open_failed".to_string())?;
    Ok(serde_json::json!({"connection_id":handoff.connection_id,"state":"browser_opened"}))
}

#[tauri::command]
pub async fn team_finish_login(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    connection_id: String,
) -> Result<TeamConnectionStatus, String> {
    trusted(&webview)?;
    managed(&app)?
        .finish_login(&connection_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn team_logout(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    connection_id: String,
) -> Result<(), String> {
    trusted(&webview)?;
    managed(&app)?
        .logout(&connection_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn team_connection_status(
    app: tauri::AppHandle,
    webview: tauri::Webview,
) -> Result<Vec<TeamConnectionStatus>, String> {
    trusted(&webview)?;
    Ok(managed(&app)?.statuses().await)
}
