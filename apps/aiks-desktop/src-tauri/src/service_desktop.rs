//! The service-mode desktop owns collection and child lifetimes, not business DBs.
mod ui;
use crate::service_client::{
    collector::{collect_provider, deliver_one, CollectionPolicy},
    supervisor::{binary_path, OwnedService},
    CollectorOutbox, ServiceClient,
};
use aiks_core::{
    bootstrap::{validate_runtime, BootstrapConfig},
    providers::{build_registry, catalog::descriptors, ProviderRegistry},
    runtime::SiyuanRuntime,
    storage::ownership::BusinessDbLease,
    Config,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{AppHandle, Manager};
use tokio::sync::{watch, Mutex, RwLock, Semaphore};

pub struct ServiceDesktop {
    phase: RwLock<&'static str>,
    error: RwLock<Option<&'static str>>,
    client: RwLock<Option<ServiceClient>>,
    outbox: RwLock<Option<Arc<CollectorOutbox>>>,
    owner: Mutex<Option<OwnedService>>,
    content: Mutex<Option<Arc<SiyuanRuntime>>>,
    profile_lease: std::sync::Mutex<Option<BusinessDbLease>>,
    browsers: Mutex<
        std::collections::HashMap<
            aiks_core::SourceKind,
            Arc<crate::service_client::browse::LocalSessionBrowser>,
        >,
    >,
    provider_config: Config,
    providers: Arc<ProviderRegistry>,
    gate: Semaphore,
    lifecycle_gate: Mutex<()>,
    stopping: AtomicBool,
    cancel: watch::Sender<bool>,
    delivery: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub close_to_tray: bool,
}
impl ServiceDesktop {
    pub fn new(config: Config) -> Self {
        let providers = Arc::new(build_registry(&config));
        let close_to_tray = config.desktop.close_to_tray;
        let (cancel, _) = watch::channel(false);
        Self {
            phase: RwLock::new("starting"),
            error: RwLock::new(None),
            client: RwLock::new(None),
            outbox: RwLock::new(None),
            owner: Mutex::new(None),
            content: Mutex::new(None),
            profile_lease: std::sync::Mutex::new(None),
            browsers: Mutex::new(std::collections::HashMap::new()),
            provider_config: config,
            providers,
            gate: Semaphore::new(1),
            lifecycle_gate: Mutex::new(()),
            stopping: AtomicBool::new(false),
            cancel,
            delivery: Mutex::new(None),
            close_to_tray,
        }
    }
    pub async fn start(self: &Arc<Self>, app: &AppHandle) -> anyhow::Result<()> {
        let lifecycle_guard = self.lifecycle_gate.lock().await;
        self.ensure_running()?;
        let result = self
            .start_inner(app)
            .await
            .and_then(|()| self.ensure_running());
        if result.is_err() {
            self.stopping.store(true, Ordering::Release);
            self.cancel.send_replace(true);
            self.stop_owned().await;
            *self.phase.write().await = "failed";
            *self.error.write().await = Some("service_start_failed");
        }
        drop(lifecycle_guard);
        result
    }
    fn ensure_running(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.stopping.load(Ordering::Acquire),
            "Service startup cancelled"
        );
        Ok(())
    }
    async fn start_inner(self: &Arc<Self>, app: &AppHandle) -> anyhow::Result<()> {
        self.ensure_running()?;
        let mut cancelled = self.cancel.subscribe();
        let root = crate::app_state::data_dir().join("service-local");
        let lease = BusinessDbLease::acquire(&root.join("desktop-owner"))?;
        *self
            .profile_lease
            .lock()
            .map_err(|_| anyhow::anyhow!("Profile lock unavailable"))? = Some(lease);
        for folder in ["data", "config", "logs", "siyuan/workspace"] {
            let path = root.join(folder);
            validate_private_path(&path, false)?;
            std::fs::create_dir_all(path)?;
        }
        let root = root.canonicalize()?;
        let outbox = self.local_store().await.map_err(anyhow::Error::msg)?;
        let resource = app.path().resource_dir()?;
        let binary = binary_path(&resource)?;
        let runtime_root = crate::bootstrap::locate_runtime_root(app)?;
        let bootstrap = BootstrapConfig::new(runtime_root, root.clone(), "AIKS Service Knowledge");
        validate_runtime(&bootstrap)?;
        let runtime = Arc::new(SiyuanRuntime::new(bootstrap.runtime_config()));
        *self.content.lock().await = Some(runtime.clone());
        self.ensure_running()?;
        let info = tokio::select! {
            _=cancelled.changed()=>anyhow::bail!("Service startup cancelled"),
            result=runtime.start()=>result?,
        };
        self.ensure_running()?;
        let path = prepare_config(&root, &info.base_url)?;
        let identity_path = root.join("instance-id");
        validate_private_path(&identity_path, true)?;
        let expected = if identity_path.exists() {
            anyhow::ensure!(
                std::fs::metadata(&identity_path)?.len() <= 128,
                "Invalid stored instance identity"
            );
            Some(std::fs::read_to_string(&identity_path)?.trim().to_owned())
        } else {
            None
        };
        let owner = tokio::select! {
            _=cancelled.changed()=>anyhow::bail!("Service startup cancelled"),
            result=OwnedService::start(&binary,&path,expected.as_deref())=>result?,
        };
        let client = owner.client();
        let instance = client.connection().instance_id().to_owned();
        if identity_path.exists() {
            let existing = std::fs::read_to_string(&identity_path)?;
            if existing.trim() != instance {
                let mut owner = owner;
                let _ = owner.shutdown().await;
                anyhow::bail!("Service identity changed; explicitly reconcile the local profile");
            }
        } else {
            write_private(&identity_path, instance.as_bytes(), true)?;
        }
        *self.owner.lock().await = Some(owner);
        *self.client.write().await = Some(client);
        *self.outbox.write().await = Some(outbox);
        self.ensure_running()?;
        *self.phase.write().await = "ready";
        let state = self.clone();
        *self.delivery.lock().await = Some(tokio::spawn(async move {
            let mut cancelled = state.cancel.subscribe();
            loop {
                tokio::select! {
                    _=cancelled.changed()=>break,
                    _=tokio::time::sleep(Duration::from_secs(3))=>{}
                }
                if state.stopping.load(Ordering::Acquire) {
                    break;
                }
                let Ok(_permit) = state.gate.try_acquire() else {
                    continue;
                };
                let Ok((client, outbox)) = state.connection().await else {
                    continue;
                };
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
                    .min(u64::MAX as u128) as u64;
                tokio::select! {
                    _=cancelled.changed()=>break,
                    _=deliver_one(&client,outbox,now)=>{}
                }
            }
        }));
        Ok(())
    }
    pub async fn connection(&self) -> Result<(ServiceClient, Arc<CollectorOutbox>), String> {
        if self.stopping.load(Ordering::Acquire) {
            return Err("service_stopping".into());
        }
        let client = self
            .client
            .read()
            .await
            .clone()
            .ok_or("service_not_ready")?;
        let outbox = self
            .outbox
            .read()
            .await
            .clone()
            .ok_or("service_not_ready")?;
        Ok((client, outbox))
    }
    pub async fn status(&self) -> Value {
        let phase = *self.phase.read().await;
        let mut capabilities = None;
        let mut connection_error = None;
        if phase == "ready" {
            if let Ok((client, _)) = self.connection().await {
                match tokio::time::timeout(Duration::from_secs(3), client.capabilities()).await {
                    Ok(Ok(value)) => capabilities = Some(value),
                    _ => connection_error = Some("service_unavailable"),
                }
            }
        }
        let providers = descriptors(&self.provider_config, &[])
            .into_iter()
            .filter(|p| p.configurable)
            .map(|p| json!({"key":p.key,"display_name":p.display_name,"enabled":p.enabled}))
            .collect::<Vec<_>>();
        json!({"mode":"service_local","phase":if connection_error.is_some(){"unavailable"}else{phase},
            "error_code":connection_error.or(*self.error.read().await),"capabilities":capabilities,"providers":providers})
    }
    pub async fn collect(&self, sources: Vec<String>) -> Result<Value, String> {
        if sources.is_empty() || sources.len() > 16 {
            return Err("invalid_selection".into());
        }
        let _permit = self.gate.try_acquire().map_err(|_| "collector_busy")?;
        let (client, outbox) = self.connection().await?;
        let mut selected = Vec::new();
        for key in sources {
            let source = aiks_core::SourceKind::from_str(&key).ok_or("invalid_source")?;
            if selected.contains(&source) {
                continue;
            }
            if self.providers.get(source).is_none() {
                return Err("source_disabled_or_unavailable".into());
            }
            selected.push(source);
        }
        let mut reports = Vec::new();
        let mut cancelled = self.cancel.subscribe();
        for source in selected {
            let provider = self
                .providers
                .get(source)
                .ok_or("source_disabled_or_unavailable")?;
            let policy = CollectionPolicy {
                source_key: self.source_key(source)?,
                ..Default::default()
            };
            let report = tokio::select! {
                _=cancelled.changed()=>return Err("collection_cancelled".into()),
                result=collect_provider(provider,&client,outbox.clone(),&policy)=>result.map_err(|e|e.to_string())?
            };
            reports.push(json!({"source":source.as_str(),"report":report}));
        }
        Ok(json!({"sources":reports,"delivery":"queued"}))
    }
    pub async fn shutdown(&self) {
        self.stopping.store(true, Ordering::Release);
        self.cancel.send_replace(true);
        let _lifecycle_guard = self.lifecycle_gate.lock().await;
        *self.phase.write().await = "stopping";
        self.stop_owned().await;
        *self.outbox.write().await = None;
        self.browsers.lock().await.clear();
    }
    async fn stop_owned(&self) {
        let delivery = self.delivery.lock().await.take();
        if let Some(task) = delivery {
            task.abort();
            let _ = task.await;
        }
        let _permit = tokio::time::timeout(Duration::from_secs(5), self.gate.acquire()).await;
        let owner = self.owner.lock().await.take();
        if let Some(mut owner) = owner {
            match owner.shutdown().await {
                Ok(false) => {}
                _ => {
                    *self.error.write().await = Some("shutdown_required_termination");
                }
            }
        }
        let content = self.content.lock().await.take();
        if let Some(runtime) = content {
            runtime.stop().await;
        }
        *self.client.write().await = None;
        // Keep client-only preferences available after a failed service startup.
        if let Ok(mut lease) = self.profile_lease.lock() {
            lease.take();
        }
        *self.phase.write().await = "stopped";
    }
}

fn prepare_config(root: &Path, content_url: &str) -> anyhow::Result<PathBuf> {
    let models = root.join("config/models.toml");
    validate_private_path(&models, true)?;
    if !models.exists() {
        write_private(&models,b"[ai]\nenabled=false\nbase_url=\"http://127.0.0.1:11434/v1\"\nmodel=\"\"\n\n[embedding]\nenabled=false\nbase_url=\"http://127.0.0.1:11434/v1\"\nmodel=\"\"\n",true)?;
    }
    let metadata = std::fs::symlink_metadata(&models)?;
    anyhow::ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 1024 * 1024,
        "Invalid service model configuration"
    );
    let model: toml::Value = toml::from_str(&std::fs::read_to_string(models)?)?;
    let table = model
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("Invalid model configuration"))?;
    anyhow::ensure!(
        table
            .keys()
            .all(|k| matches!(k.as_str(), "ai" | "embedding")),
        "Unexpected model configuration section"
    );
    let mut value = json!({"database":root.join("data/aiks.db"),"mode":"personal","listen":"127.0.0.1:0",
        "ai":{"enabled":false,"base_url":"","model":""},"embedding":{"enabled":false},
        "siyuan":{"base_url":content_url,"token":"","notebook_name":"AIKS Service Knowledge"}});
    for section in ["ai", "embedding"] {
        if let Some(config) = table.get(section) {
            let mut item = serde_json::to_value(config)?;
            let object = item
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("Invalid model table"))?;
            object.entry("enabled").or_insert(Value::Bool(false));
            for key in ["base_url", "model"] {
                object.entry(key).or_insert(Value::String(String::new()));
            }
            value[section] = item;
        }
    }
    let text = toml::to_string_pretty(&value)?;
    let path = root.join("config/runtime.toml");
    write_private(&path, text.as_bytes(), false)?;
    Ok(path)
}
fn write_private(path: &Path, content: &[u8], new: bool) -> anyhow::Result<()> {
    use std::io::Write;
    validate_private_path(path, true)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if new {
        options.create_new(true);
    } else {
        options.create(true).truncate(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(content)?;
    file.sync_all()?;
    Ok(())
}

// Defensive native boundary, not an OS sandbox against a same-user attacker.
fn validate_private_path(path: &Path, file: bool) -> anyhow::Result<()> {
    for (index, component) in path.ancestors().enumerate() {
        let meta = match std::fs::symlink_metadata(component) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        anyhow::ensure!(
            !meta.file_type().is_symlink(),
            "Linked profile path is not supported"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            anyhow::ensure!(
                meta.file_attributes() & 0x400 == 0,
                "Reparse profile path is not supported"
            );
        }
        if index == 0 && file {
            anyhow::ensure!(
                meta.is_file(),
                "Profile configuration must be a regular file"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                anyhow::ensure!(
                    meta.nlink() == 1,
                    "Hard-linked profile file is not supported"
                );
            }
        } else {
            anyhow::ensure!(meta.is_dir(), "Profile ancestor must be a directory");
        }
    }
    Ok(())
}

#[cfg(test)]
mod service_desktop_tests {
    use super::*;
    #[tokio::test]
    async fn shutdown_cancels_startup_before_waiting_and_cleanup_is_serialized() {
        let state = Arc::new(ServiceDesktop::new(Config::default()));
        let lifecycle = state.lifecycle_gate.lock().await;
        let mut cancelled = state.cancel.subscribe();
        let other = state.clone();
        let stopping = tokio::spawn(async move {
            other.shutdown().await;
        });
        tokio::time::timeout(Duration::from_secs(2), cancelled.changed())
            .await
            .unwrap()
            .unwrap();
        assert!(state.ensure_running().is_err());
        assert!(!stopping.is_finished());
        drop(lifecycle);
        stopping.await.unwrap();
        state.shutdown().await;
        assert_eq!(*state.phase.read().await, "stopped");
    }
    #[test]
    fn profile_links_and_model_defaults_cannot_escape_the_new_space() {
        let root = std::env::temp_dir().join(format!("aiks-profile-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("config")).unwrap();
        let path = prepare_config(&root, "http://127.0.0.1:12345").unwrap();
        let value: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["ai"]["enabled"].as_bool(), Some(false));
        assert_eq!(value["ai"]["model"].as_str(), Some(""));
        assert_eq!(value["embedding"]["enabled"].as_bool(), Some(false));
        let models = root.join("config/models.toml");
        std::fs::write(&models, "[ai]\nmodel='explicit-name'\n").unwrap();
        let path = prepare_config(&root, "http://127.0.0.1:12345").unwrap();
        let value: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["ai"]["enabled"].as_bool(), Some(false));
        assert_eq!(value["ai"]["base_url"].as_str(), Some(""));
        #[cfg(unix)]
        {
            std::fs::remove_file(&models).unwrap();
            let target = root.join("private-models");
            std::fs::write(&target, "DO_NOT_TOUCH").unwrap();
            std::os::unix::fs::symlink(&target, &models).unwrap();
            assert!(prepare_config(&root, "http://127.0.0.1:12345").is_err());
            assert_eq!(std::fs::read_to_string(target).unwrap(), "DO_NOT_TOUCH");
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
