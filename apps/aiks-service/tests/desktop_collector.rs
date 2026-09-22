#[path = "../../aiks-desktop/src-tauri/src/service_client/mod.rs"]
pub mod service_client;
use service_client::*;
use service_client::collector::*;
mod support;
use support::RunningService;
use std::{sync::Arc,time::Duration};
use aiks_core::{config::ExternalProviderConfig,model::SourceKind,providers::{native::NativeProvider,claude::ClaudeProvider,SessionProvider}};
use serde_json::{json,Value};

fn write_continue(root:&std::path::Path,id:&str,text:&str) {
    std::fs::create_dir_all(root.join("sessions")).unwrap();
    std::fs::write(root.join("sessions").join(format!("{id}.json")),json!({
        "sessionId":id,"title":"Collector test","workspaceDirectory":"/private/workspace",
        "history":[{"message":{"role":"user","content":text}}]
    }).to_string()).unwrap();
}
fn native(root:&std::path::Path)->NativeProvider {
    NativeProvider::new(SourceKind::Continue,&ExternalProviderConfig{enabled:true,path:root.to_string_lossy().into(),paths:vec![]}).unwrap()
}

#[tokio::test]
async fn real_provider_is_sanitized_before_queueing_and_survives_source_removal() {
    let s=RunningService::start().await;let root=tempfile::tempdir().unwrap();let tools=root.path().join("continue");
    write_continue(&tools,"one","COLLECTOR_UNIQUE token=SYNTHETIC_CAPTURE_12345");
    let provider=native(&tools);
    let c=ServiceClient::new(ServiceConnection::local(&s.base,&s.instance_id,&s.space_id,&s.token).unwrap()).unwrap();
    let o=Arc::new(CollectorOutbox::open(&root.path().join("collector.db")).unwrap());
    let report=collect_provider(&provider,&c,o.clone(),&CollectionPolicy::default()).await.unwrap();
    assert_eq!((report.queued,report.failed),(1,0));assert!(report.complete);
    let second=collect_provider(&provider,&c,o.clone(),&CollectionPolicy::default()).await.unwrap();
    assert_eq!(second.queued,0);
    let claim=o.next_for(&s.instance_id,&s.space_id,0).unwrap().unwrap();
    let body=serde_json::to_string(claim.pending().submission()).unwrap();
    assert!(body.contains("COLLECTOR_UNIQUE"));assert!(!body.contains("SYNTHETIC_CAPTURE_12345"));
    assert!(claim.pending().submission().session.source_path.is_none());
    assert!(claim.pending().submission().session.project_path.is_none());
    o.record_failure(&claim,ClientError::Retryable,0).unwrap();
    std::fs::remove_dir_all(&tools).unwrap();
    let receipt=deliver_one(&c,o.clone(),1_000_000).await.unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(10),async {
        loop {let job=c.get_job(&receipt.job_id).await.unwrap();if job["status"]=="DONE"{break;}
            assert_ne!(job["status"],"FAILED");tokio::time::sleep(Duration::from_millis(20)).await;}
    }).await.unwrap();
    assert_eq!(c.search("COLLECTOR_UNIQUE").await.unwrap()["hits"].as_array().unwrap().len(),1);
    let status=o.statuses(&s.instance_id,&s.space_id).unwrap();
    assert_eq!(status[0].state,"acknowledged");assert_eq!(status[0].receipt.as_ref().unwrap(),&receipt);
    s.stop().await;
}

#[tokio::test]
async fn a_known_registration_can_queue_offline_and_excluded_sessions_are_not_uploaded() {
    let s=RunningService::start().await;let root=tempfile::tempdir().unwrap();let tools=root.path().join("continue");
    write_continue(&tools,"one","FIRST");let provider=native(&tools);
    let c=ServiceClient::new(ServiceConnection::local(&s.base,&s.instance_id,&s.space_id,&s.token).unwrap()).unwrap();
    let o=Arc::new(CollectorOutbox::open(&root.path().join("collector.db")).unwrap());
    let policy=CollectionPolicy::default();
    assert_eq!(collect_provider(&provider,&c,o.clone(),&policy).await.unwrap().queued,1);
    deliver_one(&c,o.clone(),0).await.unwrap().unwrap();
    let old_id=provider.discover_sessions().await.unwrap()[0].external_session_id.clone();
    s.stop().await;
    write_continue(&tools,"two","OFFLINE_QUEUED");
    let policy=CollectionPolicy{exclude_ids:vec![old_id],..Default::default()};
    let report=collect_provider(&provider,&c,o.clone(),&policy).await.unwrap();
    assert_eq!(report.queued,1);assert_eq!(report.excluded,1);
    let rows=o.statuses(c.connection().instance_id(),c.connection().space_id()).unwrap();
    assert_eq!(rows.iter().filter(|r|r.state=="pending").count(),1);
}

#[tokio::test]
async fn malformed_legacy_jsonl_is_not_labeled_a_complete_snapshot() {
    let s=RunningService::start().await;let root=tempfile::tempdir().unwrap();
    let tools=root.path().join("claude");let project=tools.join("projects").join("-synthetic");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("legacy-id.jsonl"),format!("{}\n{{\"type\":",json!({"type":"user","uuid":"one","message":{"role":"user","content":"DO_NOT_TRUST_PARTIAL"}}))).unwrap();
    let provider=ClaudeProvider::new(Some(tools)).unwrap();
    assert_eq!(provider.discover_sessions().await.unwrap().len(),1);
    let c=ServiceClient::new(ServiceConnection::local(&s.base,&s.instance_id,&s.space_id,&s.token).unwrap()).unwrap();
    let o=Arc::new(CollectorOutbox::open(&root.path().join("collector.db")).unwrap());
    let result=collect_provider(&provider,&c,o.clone(),&CollectionPolicy::default()).await.unwrap();
    assert_eq!(result.queued,0);assert_eq!(result.failed,1);assert!(!result.complete);
    let rows:Vec<UploadStatus>=o.statuses(&s.instance_id,&s.space_id).unwrap();assert!(rows.is_empty());
    let count:Value=c.sessions().await.unwrap();assert!(count["items"].as_array().unwrap().is_empty());
    s.stop().await;
}
