//! The service-mode desktop owns collection and child lifetimes, not business DBs.
use std::{path::{Path,PathBuf},sync::{Arc,atomic::{AtomicBool,Ordering}},time::Duration};
use aiks_core::{Config,providers::{build_registry,ProviderRegistry,catalog::descriptors},
    bootstrap::{BootstrapConfig,validate_runtime},runtime::SiyuanRuntime,storage::ownership::BusinessDbLease};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use tauri::{AppHandle,Manager};
use tokio::sync::{Mutex,RwLock,Semaphore,watch};
use crate::service_client::{ServiceClient,CollectorOutbox,supervisor::{OwnedService,binary_path},collector::{collect_provider,deliver_one,CollectionPolicy}};

pub struct ServiceDesktop {
    phase:RwLock<&'static str>,
    error:RwLock<Option<&'static str>>,
    client:RwLock<Option<ServiceClient>>,
    outbox:RwLock<Option<Arc<CollectorOutbox>>>,
    owner:Mutex<Option<OwnedService>>,
    content:Mutex<Option<Arc<SiyuanRuntime>>>,
    profile_lease:std::sync::Mutex<Option<BusinessDbLease>>,
    provider_config:Config,
    providers:Arc<ProviderRegistry>,
    gate:Semaphore,
    stopping:AtomicBool,
    cancel:watch::Sender<bool>,
    delivery:Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub close_to_tray:bool,
}
impl ServiceDesktop {
    pub fn new(config:Config)->Self {
        let providers=Arc::new(build_registry(&config));
        let close_to_tray=config.desktop.close_to_tray;
        let (cancel,_)=watch::channel(false);
        Self {phase:RwLock::new("starting"),error:RwLock::new(None),client:RwLock::new(None),
            outbox:RwLock::new(None),owner:Mutex::new(None),content:Mutex::new(None),
            profile_lease:std::sync::Mutex::new(None),provider_config:config,providers,gate:Semaphore::new(1),
            stopping:AtomicBool::new(false),cancel,delivery:Mutex::new(None),close_to_tray}
    }
    pub async fn start(self:&Arc<Self>,app:&AppHandle)->anyhow::Result<()> {
        let result=self.start_inner(app).await;
        if result.is_err() {
            self.shutdown().await;
            *self.phase.write().await="failed";
            *self.error.write().await=Some("service_start_failed");
        }
        result
    }
    async fn start_inner(self:&Arc<Self>,app:&AppHandle)->anyhow::Result<()> {
        let root=crate::app_state::data_dir().join("service-local");
        let lease=BusinessDbLease::acquire(&root.join("desktop-owner"))?;
        *self.profile_lease.lock().map_err(|_|anyhow::anyhow!("Profile lock unavailable"))?=Some(lease);
        for folder in ["data","config","logs","siyuan/workspace"] {std::fs::create_dir_all(root.join(folder))?;}
        let root=root.canonicalize()?;
        let resource=app.path().resource_dir()?;
        // Fail before spawning content when the new business binary is missing.
        let binary=binary_path(&resource)?;
        let runtime_root=crate::bootstrap::find_runtime_root(app)?;
        let bootstrap=BootstrapConfig::new(runtime_root,root.clone(),"AIKS Service Knowledge");
        validate_runtime(&bootstrap)?;
        let runtime=Arc::new(SiyuanRuntime::new(bootstrap.runtime_config()));
        // Own the runtime before awaiting startup, so all error paths clean up.
        *self.content.lock().await=Some(runtime.clone());
        let info=runtime.start().await?;
        let path=prepare_config(&root,&info.base_url)?;
        let owner=OwnedService::start(&binary,&path,None).await?;
        let client=owner.client();
        let instance=client.connection().instance_id().to_owned();
        let identity_path=root.join("instance-id");
        if identity_path.exists() {
            let existing=std::fs::read_to_string(&identity_path)?;
            if existing.trim()!=instance {
                let mut owner=owner;
                let _=owner.shutdown().await;
                anyhow::bail!("Service identity changed; explicitly reconcile the local profile");
            }
        }else{write_private(&identity_path,instance.as_bytes(),true)?;}
        let outbox=Arc::new(CollectorOutbox::open(&root.join("collector.db"))?);
        *self.owner.lock().await=Some(owner);
        *self.client.write().await=Some(client);
        *self.outbox.write().await=Some(outbox);
        *self.phase.write().await="ready";
        let state=self.clone();
        *self.delivery.lock().await=Some(tokio::spawn(async move {
            let mut cancelled=state.cancel.subscribe();
            loop {
                tokio::select! {
                    _=cancelled.changed()=>break,
                    _=tokio::time::sleep(Duration::from_secs(3))=>{}
                }
                if state.stopping.load(Ordering::Acquire){break;}
                let Ok(_permit)=state.gate.try_acquire() else {continue;};
                let Ok((client,outbox))=state.connection().await else {continue;};
                let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis().min(u64::MAX as u128) as u64;
                tokio::select! {
                    _=cancelled.changed()=>break,
                    _=deliver_one(&client,outbox,now)=>{}
                }
            }
        }));
        Ok(())
    }
    pub async fn connection(&self)->Result<(ServiceClient,Arc<CollectorOutbox>),String>{
        if self.stopping.load(Ordering::Acquire){return Err("service_stopping".into());}
        let client=self.client.read().await.clone().ok_or("service_not_ready")?;
        let outbox=self.outbox.read().await.clone().ok_or("service_not_ready")?;
        Ok((client,outbox))
    }
    pub async fn status(&self)->Value{
        let phase=*self.phase.read().await;
        let mut capabilities=None;
        let mut connection_error=None;
        if phase=="ready" {
            if let Ok((client,_))=self.connection().await {
                match tokio::time::timeout(Duration::from_secs(3),client.capabilities()).await {
                    Ok(Ok(value))=>capabilities=Some(value),
                    _=>connection_error=Some("service_unavailable"),
                }
            }
        }
        let providers=descriptors(&self.provider_config,&[]).into_iter().filter(|p|p.configurable).map(|p|
            json!({"key":p.key,"display_name":p.display_name,"enabled":p.enabled})).collect::<Vec<_>>();
        json!({"mode":"service_local","phase":if connection_error.is_some(){"unavailable"}else{phase},
            "error_code":connection_error.or(*self.error.read().await),"capabilities":capabilities,"providers":providers})
    }
    pub async fn collect(&self,sources:Vec<String>,exclude_ids:Vec<String>)->Result<Value,String>{
        if sources.is_empty() || sources.len()>16 || exclude_ids.len()>1000 {return Err("invalid_selection".into());}
        let _permit=self.gate.try_acquire().map_err(|_|"collector_busy")?;
        let (client,outbox)=self.connection().await?;
        let mut selected=Vec::new();
        for key in sources {
            let source=aiks_core::SourceKind::from_str(&key).ok_or("invalid_source")?;
            if selected.contains(&source){continue;}
            if self.providers.get(source).is_none(){return Err("source_disabled_or_unavailable".into());}
            selected.push(source);
        }
        let mut reports=Vec::new();
        let mut cancelled=self.cancel.subscribe();
        for source in selected {
            let provider=self.providers.get(source).ok_or("source_disabled_or_unavailable")?;
            let source_paths=descriptors(&self.provider_config,&[]).into_iter().find(|d|d.key==source.as_str()).map(|d|d.paths).unwrap_or_default();
            let path_key=serde_json::to_vec(&(source.as_str(),source_paths)).map_err(|_|"invalid_source")?;
            let source_key=format!("{:x}",Sha256::digest(path_key));
            let policy=CollectionPolicy{source_key,exclude_ids:exclude_ids.clone(),..Default::default()};
            let report=tokio::select! {
                _=cancelled.changed()=>return Err("collection_cancelled".into()),
                result=collect_provider(provider,&client,outbox.clone(),&policy)=>result.map_err(|e|e.to_string())?
            };
            reports.push(json!({"source":source.as_str(),"report":report}));
        }
        Ok(json!({"sources":reports,"delivery":"queued"}))
    }
    pub async fn shutdown(&self){
        if self.stopping.swap(true,Ordering::AcqRel){return;}
        *self.phase.write().await="stopping";
        self.cancel.send_replace(true);
        let delivery=self.delivery.lock().await.take();
        if let Some(task)=delivery {task.abort();let _=task.await;}
        let _permit=tokio::time::timeout(Duration::from_secs(5),self.gate.acquire()).await;
        let owner=self.owner.lock().await.take();
        if let Some(mut owner)=owner {
            match owner.shutdown().await {
                Ok(false)=>{},
                _=>{*self.error.write().await=Some("shutdown_required_termination");}
            }
        }
        let content=self.content.lock().await.take();
        if let Some(runtime)=content {runtime.stop().await;}
        *self.client.write().await=None;
        *self.outbox.write().await=None;
        if let Ok(mut lease)=self.profile_lease.lock(){lease.take();}
        *self.phase.write().await="stopped";
    }
}

fn prepare_config(root:&Path,content_url:&str)->anyhow::Result<PathBuf>{
    let models=root.join("config/models.toml");
    if !models.exists(){write_private(&models,b"[ai]\nenabled=false\nbase_url=\"http://127.0.0.1:11434/v1\"\nmodel=\"\"\n\n[embedding]\nenabled=false\nbase_url=\"http://127.0.0.1:11434/v1\"\nmodel=\"\"\n",true)?;}
    let metadata=std::fs::symlink_metadata(&models)?;
    anyhow::ensure!(metadata.is_file()&&!metadata.file_type().is_symlink()&&metadata.len()<=1024*1024,"Invalid service model configuration");
    let model:toml::Value=toml::from_str(&std::fs::read_to_string(models)?)?;
    let table=model.as_table().ok_or_else(||anyhow::anyhow!("Invalid model configuration"))?;
    anyhow::ensure!(table.keys().all(|k|matches!(k.as_str(),"ai"|"embedding")),"Unexpected model configuration section");
    let mut value=json!({"database":root.join("data/aiks.db"),"mode":"personal","listen":"127.0.0.1:0",
        "ai":{"enabled":false,"base_url":"","model":""},"embedding":{"enabled":false},
        "siyuan":{"base_url":content_url,"token":"","notebook_name":"AIKS Service Knowledge"}});
    for section in ["ai","embedding"] {
        if let Some(config)=table.get(section){
            let mut item=serde_json::to_value(config)?;
            let object=item.as_object_mut().ok_or_else(||anyhow::anyhow!("Invalid model table"))?;
            object.entry("enabled").or_insert(Value::Bool(false));
            for key in ["base_url","model"]{object.entry(key).or_insert(Value::String(String::new()));}
            value[section]=item;
        }
    }
    let text=toml::to_string_pretty(&value)?;
    let path=root.join("config/runtime.toml");
    write_private(&path,text.as_bytes(),false)?;
    Ok(path)
}
fn write_private(path:&Path,content:&[u8],new:bool)->anyhow::Result<()> {
    use std::io::Write;
    if let Ok(meta)=std::fs::symlink_metadata(path){anyhow::ensure!(meta.is_file()&&!meta.file_type().is_symlink(),"Invalid configuration file");}
    let mut options=std::fs::OpenOptions::new();
    options.write(true);
    if new {options.create_new(true);}else{options.create(true).truncate(true);}
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    let mut file=options.open(path)?;file.write_all(content)?;file.sync_all()?;Ok(())
}
