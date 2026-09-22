//! A bounded batch must eventually reach every selected source session.
#[path = "../../aiks-desktop/src-tauri/src/service_client/mod.rs"]
pub mod service_client;
use aiks_core::{
    config::ExternalProviderConfig, model::SourceKind, providers::native::NativeProvider,
};
use serde_json::json;
use service_client::{
    collector::{collect_provider, CollectionPolicy},
    CollectorOutbox, ServiceClient, ServiceConnection,
};
use std::sync::Arc;
mod support;

#[tokio::test]
async fn repeated_bounded_capture_advances_past_unchanged_sessions_and_survives_restart() {
    let service = support::RunningService::start().await;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("continue");
    std::fs::create_dir_all(source.join("sessions")).unwrap();
    for i in 0..7 {
        std::fs::write(source.join("sessions").join(format!("{i}.json")),json!({"sessionId":format!("session-{i}"),"title":"Batch fixture","history":[{"message":{"role":"user","content":format!("BATCH_BODY_{i}")}}]}).to_string()).unwrap();
    }
    let provider = NativeProvider::new(
        SourceKind::Continue,
        &ExternalProviderConfig {
            enabled: true,
            path: source.to_string_lossy().into(),
            paths: vec![],
        },
    )
    .unwrap();
    let client = ServiceClient::new(
        ServiceConnection::local(
            &service.base,
            &service.instance_id,
            &service.space_id,
            &service.token,
        )
        .unwrap(),
    )
    .unwrap();
    let queue = root.path().join("collector.db");
    let policy = CollectionPolicy {
        max_sessions: 3,
        ..Default::default()
    };
    let mut counts = Vec::new();
    for _ in 0..3 {
        let outbox = Arc::new(CollectorOutbox::open(&queue).unwrap());
        let report = collect_provider(&provider, &client, outbox, &policy)
            .await
            .unwrap();
        counts.push(report.queued);
    }
    assert_eq!(
        counts,
        vec![3, 3, 1],
        "a later batch must not rescan only the first unchanged entries"
    );
    let outbox = Arc::new(CollectorOutbox::open(&queue).unwrap());
    let report = collect_provider(&provider, &client, outbox.clone(), &policy)
        .await
        .unwrap();
    assert_eq!(report.queued, 0);
    assert_eq!(
        outbox
            .statuses(&service.instance_id, &service.space_id)
            .unwrap()
            .len(),
        7
    );
    service.stop().await;
}
