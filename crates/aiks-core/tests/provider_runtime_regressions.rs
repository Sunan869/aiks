use aiks_core::{
    config::{Config, ExternalProviderConfig},
    model::{hash::compute_session_hash, SourceKind},
    pipeline::{PipelineJob, PipelineOrchestrator, PipelineWorker},
    providers::{native::NativeProvider, ProviderRegistry, SessionProvider},
    storage::{SourceSessionRepo, StateDb},
};
use serde_json::json;
use std::{path::Path, sync::Arc, time::Duration};

fn provider(source: SourceKind, root: &Path) -> NativeProvider {
    NativeProvider::new(
        source,
        &ExternalProviderConfig {
            path: root.to_string_lossy().into_owned(),
            ..Default::default()
        },
    )
    .unwrap()
}
#[tokio::test]
async fn corrupt_cursor_database_is_not_reported_healthy() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("globalStorage")).unwrap();
    std::fs::write(root.path().join("globalStorage/state.vscdb"), b"not sqlite").unwrap();
    assert!(!provider(SourceKind::Cursor, root.path())
        .health_check()
        .await
        .is_ok());
}
#[tokio::test]
async fn continue_metadata_and_message_updates_keep_identity_and_torn_content_is_not_loaded() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("sessions")).unwrap();
    let path = root.path().join("sessions/s.json");
    let mut v = json!({"sessionId":"s1","title":"before","history":[{"message":{"role":"user","content":"original"}},{"message":{"role":"assistant","content":"answer"}}]});
    std::fs::write(&path, v.to_string()).unwrap();
    let p = provider(SourceKind::Continue, root.path());
    let summary = p.discover_sessions().await.unwrap().remove(0);
    let first = p.load_session(&summary).await.unwrap();
    v["title"] = json!("renamed");
    std::fs::write(&path, v.to_string()).unwrap();
    let second = p.load_session(&summary).await.unwrap();
    assert_eq!(first.external_session_id, second.external_session_id);
    assert_ne!(compute_session_hash(&first), compute_session_hash(&second));
    v["history"]
        .as_array_mut()
        .unwrap()
        .push(json!({"message":{"role":"user","content":"appended"}}));
    std::fs::write(&path, v.to_string()).unwrap();
    let third = p.load_session(&summary).await.unwrap();
    assert_eq!(third.messages.len(), 3);
    assert_eq!(third.messages[0].created_at, first.messages[0].created_at);
    assert_ne!(compute_session_hash(&second), compute_session_hash(&third));
    std::fs::write(&path, "{\"sessionId\":").unwrap();
    assert!(p.load_session(&summary).await.is_err());
}
#[tokio::test]
async fn a_valid_session_can_finish_pipeline_when_its_neighbor_is_damaged() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("sessions")).unwrap();
    std::fs::write(root.path().join("sessions/good.json"),json!({"sessionId":"s1","history":[{"message":{"role":"user","content":"valid useful question"}},{"message":{"role":"assistant","content":"valid useful answer"}}]}).to_string()).unwrap();
    std::fs::write(root.path().join("sessions/bad.json"), "{broken").unwrap();
    let p = provider(SourceKind::Continue, root.path());
    let report = p.discover_report().await.unwrap();
    assert!(!report.complete);
    assert_eq!(report.sessions.len(), 1);
    let summary = &report.sessions[0];
    let session = p.load_session(summary).await.unwrap();
    let hash = compute_session_hash(&session);
    let data = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&data.path().join("state.db")).unwrap());
    let session_id = SourceSessionRepo::new(&db)
        .upsert(
            "continue",
            &summary.external_session_id,
            summary.source_path.as_deref().and_then(Path::to_str),
            None,
            None,
            summary.title.as_deref(),
            None,
            Some(&hash),
            Some(p.parser_version()),
        )
        .unwrap();
    let config = Config::default();
    let worker = PipelineWorker::start_with_limit(
        db.clone(),
        Arc::new(ProviderRegistry::new(vec![Box::new(p)])),
        config.ai,
        config.embedding,
        1,
    );
    let run_id = PipelineOrchestrator::new(db.clone())
        .enqueue(session_id, Some(&hash))
        .unwrap();
    worker
        .submit(PipelineJob {
            pipeline_run_id: run_id.clone(),
            session_id,
            session_external_id: summary.external_session_id.clone(),
            source: "continue".into(),
            session_title: summary.title.clone(),
            project_name: None,
        })
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status: String = db
                .conn()
                .query_row(
                    "SELECT status FROM pipeline_run WHERE id=?1",
                    [&run_id],
                    |r| r.get(0),
                )
                .unwrap();
            if status == "RAW_ONLY" {
                break;
            }
            assert_ne!(
                status, "FAILED",
                "a corrupt neighbor must not block the valid session"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
