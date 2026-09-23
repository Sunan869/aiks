//! Same native implementation as the Desktop. No real user data or model calls.
#[path = "../../aiks-desktop/src-tauri/src/service_client/mod.rs"]
pub mod service_client;
use service_client::{CollectorOutbox, ClientError, ServiceClient, ServiceConnection};
use service_client::preferences::{Onboarding, SourceScope};
use service_client::browse::LocalSessionBrowser;
use service_client::collector::{collect_provider, deliver_one, CollectionPolicy};
use aiks_core::{config::ExternalProviderConfig, model::SourceKind, providers::{native::NativeProvider, SessionProvider}};
use std::sync::Arc;
use serde_json::json;
mod support;
use support::RunningService;

fn provider(root:&std::path::Path)->NativeProvider {
    std::fs::create_dir_all(root.join("sessions")).unwrap();
    for (id,title,text) in [("one","项目迁移","READ_ONLY_PREVIEW token=SYNTHETIC_SECRET_12345"),("two","临时测试","SECOND_SESSION")] {
        std::fs::write(root.join("sessions").join(format!("{id}.json")),json!({"sessionId":id,"title":title,"workspaceDirectory":"/private/project","history":[{"message":{"role":"user","content":text}}]}).to_string()).unwrap();
    }
    NativeProvider::new(SourceKind::Continue,&ExternalProviderConfig{enabled:true,path:root.to_string_lossy().into(),paths:vec![]}).unwrap()
}

#[test]
fn skipping_onboarding_is_durable_without_a_service_connection_or_any_upload() {
    let root=tempfile::tempdir().unwrap();let path=root.path().join("collector.db");
    let store=CollectorOutbox::open(&path).unwrap();
    assert_eq!(store.ui_preferences().unwrap().onboarding,Onboarding::Unseen);
    store.finish_onboarding(Onboarding::Skipped).unwrap();
    assert!(store.ui_preferences().unwrap().selected_sources.is_empty());
    drop(store);
    let store=CollectorOutbox::open(&path).unwrap();
    assert_eq!(store.ui_preferences().unwrap().onboarding,Onboarding::Skipped);
    assert!(store.statuses("any","any").unwrap().is_empty());
}

#[tokio::test]
async fn scanning_and_preview_only_read_local_data_and_use_opaque_row_keys() {
    let root=tempfile::tempdir().unwrap();let provider=provider(&root.path().join("tool"));
    let browser=LocalSessionBrowser::scan(&provider).await.unwrap();
    let page=browser.page("迁移",0,20,&[]).unwrap();
    assert_eq!(page.total,1);assert_eq!(page.items[0].title,"项目迁移");
    assert!(!serde_json::to_string(&page).unwrap().contains("external_session_id"));
    assert!(!serde_json::to_string(&page).unwrap().contains("source_path"));
    let preview=browser.preview(&provider,&page.items[0].key).await.unwrap();
    let body=serde_json::to_string(&preview).unwrap();
    assert!(body.contains("READ_ONLY_PREVIEW"));assert!(!body.contains("SYNTHETIC_SECRET_12345"));
    assert!(!body.contains("/private/project"));
    assert!(browser.resolve("invented-key").is_err());
    assert_eq!(browser.page("",0,1,&[]).unwrap().items.len(),1);
    assert!(browser.page("",0,101,&[]).is_err());
    assert!(!root.path().join("collector.db").exists());
}

#[tokio::test]
async fn exclusion_stops_unsent_content_is_source_scoped_and_survives_restart() {
    let s=RunningService::start().await;let root=tempfile::tempdir().unwrap();
    let provider=provider(&root.path().join("tool"));
    let client=ServiceClient::new(ServiceConnection::local(&s.base,&s.instance_id,&s.space_id,&s.token).unwrap()).unwrap();
    let path=root.path().join("collector.db");let store=Arc::new(CollectorOutbox::open(&path).unwrap());
    let policy=CollectionPolicy::default();
    collect_provider(&provider,&client,store.clone(),&policy).await.unwrap();
    let scope=SourceScope::new(&s.instance_id,&s.space_id,SourceKind::Continue,"default").unwrap();
    let one=provider.discover_sessions().await.unwrap().into_iter().find(|r|r.title.as_deref()==Some("项目迁移")).unwrap().external_session_id;
    let change=store.set_excluded(&scope,std::slice::from_ref(&one),true).unwrap();
    assert_eq!(change.paused,1);assert_eq!(change.already_received,0);
    let other=SourceScope::new(&s.instance_id,&s.space_id,SourceKind::WorkBuddy,"default").unwrap();
    assert!(store.excluded(&other).unwrap().is_empty());
    let other=SourceScope::new("another-instance",&s.space_id,SourceKind::Continue,"default").unwrap();
    assert!(store.excluded(&other).unwrap().is_empty());
    drop(store);let store=Arc::new(CollectorOutbox::open(&path).unwrap());
    assert_eq!(store.excluded(&scope).unwrap(),vec![one.clone()]);
    let report=collect_provider(&provider,&client,store.clone(),&policy).await.unwrap();
    assert_eq!(report.excluded,1,"collector must honor saved rules without the UI resending IDs");
    let receipt=deliver_one(&client,store.clone(),0).await.unwrap().unwrap();
    let session=client.session(&receipt.session_id).await.unwrap();
    assert!(session.to_string().contains("SECOND_SESSION"));
    assert!(deliver_one(&client,store.clone(),0).await.unwrap().is_none());
    store.set_excluded(&scope,std::slice::from_ref(&one),false).unwrap();
    let receipt=deliver_one(&client,store.clone(),0).await.unwrap().unwrap();
    let change=store.set_excluded(&scope,std::slice::from_ref(&one),true).unwrap();
    assert_eq!(change.paused,0);assert!(change.already_received>0);
    assert!(client.session(&receipt.session_id).await.is_ok(),"excluding does not delete server knowledge");
    s.stop().await;
}

#[tokio::test]
async fn exclusion_does_not_claim_to_recall_an_inflight_request() {
    let s=RunningService::start().await;let root=tempfile::tempdir().unwrap();
    let provider=provider(&root.path().join("tool"));
    let client=ServiceClient::new(ServiceConnection::local(&s.base,&s.instance_id,&s.space_id,&s.token).unwrap()).unwrap();
    let store=Arc::new(CollectorOutbox::open(&root.path().join("collector.db")).unwrap());
    collect_provider(&provider,&client,store.clone(),&CollectionPolicy::default()).await.unwrap();
    let claim=store.next_for(&s.instance_id,&s.space_id,0).unwrap().unwrap();
    let id=claim.pending().submission().session.external_session_id.clone();
    let scope=SourceScope::new(&s.instance_id,&s.space_id,SourceKind::Continue,"default").unwrap();
    assert!(matches!(store.set_excluded(&scope,&[id],true),Err(ClientError::Busy)));
    assert!(store.excluded(&scope).unwrap().is_empty());
    s.stop().await;
}
