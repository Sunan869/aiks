use std::sync::Arc;
use std::time::Duration;

use aiks_core::ai::{AiModelConfig, ModelService};
use aiks_core::pipeline::input::SnapshotInput;
use aiks_core::pipeline::job_repo::PipelineJobRepo;
use aiks_core::pipeline::repo::PipelineRepo;
use aiks_core::providers::build_registry;
use aiks_core::service::{validate_submission, ServiceStore, SnapshotReceipt};
use aiks_core::storage::StateDb;
use aiks_core::{
    Config, EmbeddingConfig, PipelineJob, PipelineWorker, SearchCorpus, SourceKind,
    UnifiedSearchFilter, UnifiedSearchService,
};
use serde_json::json;

#[path = "support/service_fixture.rs"]
mod fixture;

fn ai_off() -> AiModelConfig {
    AiModelConfig {
        enabled: false,
        ..Default::default()
    }
}

fn embedding_off() -> EmbeddingConfig {
    EmbeddingConfig {
        enabled: false,
        ..Default::default()
    }
}

async fn wait_raw(db: &StateDb, receipt: &SnapshotReceipt) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let detail = PipelineRepo::new(db)
                .get_run_detail(&receipt.pipeline_run_id)
                .unwrap()
                .unwrap();
            if detail.status == "RAW_ONLY" {
                break;
            }
            assert_ne!(detail.status, "FAILED", "snapshot processing failed");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("snapshot must finish without its source files");
}

#[tokio::test]
async fn real_continue_snapshot_remains_searchable_after_source_directory_is_deleted() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    std::fs::create_dir(source.path().join("sessions")).unwrap();
    std::fs::write(
        source.path().join("sessions/example.json"),
        json!({
            "sessionId":"s1", "title":"Offline processing",
            "history":[
                {"message":{"role":"user","content":"OFFLINE_SNAPSHOT_UNIQUE"}},
                {"message":{"role":"assistant","content":"A verified synthetic response"}}
            ]
        })
        .to_string(),
    )
    .unwrap();
    let config: Config = toml::from_str(&format!(
        "[providers.continue]\nenabled=true\npath={}\n",
        serde_json::to_string(&source.path().to_string_lossy()).unwrap()
    ))
    .unwrap();
    let registry = build_registry(&config);
    let provider = registry.get(SourceKind::Continue).unwrap();
    let summaries = provider.discover_sessions().await.unwrap();
    assert_eq!(summaries.len(), 1);
    let session = provider.load_session(&summaries[0]).await.unwrap();
    let db = Arc::new(StateDb::open_exclusive(&root.path().join("state.db")).unwrap());
    let store = ServiceStore::open(db.clone()).unwrap();
    let ctx = store.local_context();
    let reg = store
        .register_source(&ctx, SourceKind::Continue, "device")
        .unwrap();
    let mut submission =
        fixture::submission(ctx.space_id(), ctx.instance_id(), &reg, "u1", 0, "unused");
    submission.session = session;
    let receipt = store
        .accept(&ctx, &validate_submission(&submission).unwrap())
        .unwrap()
        .0;
    source.close().unwrap();
    drop(registry);
    let worker = PipelineWorker::start_from_snapshots(db.clone(), ai_off(), embedding_off());
    wait_raw(&db, &receipt).await;
    let models = Arc::new(ModelService::new(ai_off(), embedding_off()).unwrap());
    let found = UnifiedSearchService::new(db.clone(), models)
        .search(
            "OFFLINE_SNAPSHOT_UNIQUE",
            10,
            UnifiedSearchFilter {
                corpora: vec![SearchCorpus::Session],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(found
        .hits
        .iter()
        .any(|hit| hit.entity_id == receipt.session_id));
    worker.shutdown(Duration::from_secs(2)).await.unwrap();
}

#[tokio::test]
async fn persisted_jobs_resume_after_reopening_and_do_not_consume_unadopted_legacy_jobs() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("state.db");
    let receipt = {
        let db = Arc::new(StateDb::open_exclusive(&path).unwrap());
        let store = ServiceStore::open(db.clone()).unwrap();
        let ctx = store.local_context();
        let reg = store
            .register_source(&ctx, SourceKind::Continue, "device")
            .unwrap();
        let submission = fixture::submission(
            ctx.space_id(),
            ctx.instance_id(),
            &reg,
            "u1",
            0,
            "RESUME_UNIQUE",
        );
        let receipt = store
            .accept(&ctx, &validate_submission(&submission).unwrap())
            .unwrap()
            .0;
        db.conn().execute_batch(
            "INSERT INTO source_session
             (id,source,external_session_id,content_hash,last_seen_at,created_at,updated_at)
             VALUES (900,'continue','legacy','hash','now','now','now');
             INSERT INTO pipeline_run (id,session_id,pipeline_version,source_hash,created_at,updated_at)
             VALUES ('legacy-run',900,'v3','hash','now','now');"
        ).unwrap();
        PipelineJobRepo::new(&db)
            .enqueue(&PipelineJob {
                pipeline_run_id: "legacy-run".into(),
                session_id: 900,
                session_external_id: "legacy".into(),
                source: "continue".into(),
                session_title: None,
                project_name: None,
            })
            .unwrap();
        receipt
    };
    let db = Arc::new(StateDb::open_exclusive(&path).unwrap());
    let worker = PipelineWorker::start_from_snapshots(db.clone(), ai_off(), embedding_off());
    wait_raw(&db, &receipt).await;
    worker.shutdown(Duration::from_secs(2)).await.unwrap();
    let legacy: String = db
        .conn()
        .query_row(
            "SELECT status FROM pipeline_job WHERE session_id=900",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(legacy, "PENDING");
    drop(worker);
    drop(db);
    assert!(
        StateDb::open_exclusive(&path).is_ok(),
        "shutdown must release worker references"
    );
}

#[test]
fn run_input_is_its_immutable_revision_and_never_falls_back_to_a_provider_path() {
    let root = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open_exclusive(&root.path().join("state.db")).unwrap());
    let store = ServiceStore::open(db.clone()).unwrap();
    let ctx = store.local_context();
    let reg = store
        .register_source(&ctx, SourceKind::Continue, "device")
        .unwrap();
    let mut submission =
        fixture::submission(ctx.space_id(), ctx.instance_id(), &reg, "u1", 0, "first");
    submission.session.source_path = Some("/nonexistent/source.json".into());
    let first = store
        .accept(&ctx, &validate_submission(&submission).unwrap())
        .unwrap()
        .0;
    submission.submission_id = "u2".into();
    submission.expected_revision = 1;
    submission.session.messages[0].blocks[0] = aiks_core::ContentBlock::Text {
        text: "second".into(),
    };
    store
        .accept(&ctx, &validate_submission(&submission).unwrap())
        .unwrap();
    let (loaded, fence) = SnapshotInput::load_for_run(&db, &first.pipeline_run_id).unwrap();
    assert_eq!(fence.revision, 1);
    assert_eq!(fence.snapshot_id, first.snapshot_id);
    assert_eq!(loaded.messages[0].blocks[0].text_content(), Some("first"));
    assert!(loaded.external_session_id.starts_with("svc:"));
    assert!(loaded.source_path.is_none());
    db.conn()
        .execute(
            "UPDATE service_session_snapshot SET canonical_json=?1 WHERE id=?2",
            rusqlite::params![b"not-json".as_slice(), first.snapshot_id],
        )
        .unwrap();
    assert!(SnapshotInput::load_for_run(&db, &first.pipeline_run_id).is_err());
    assert!(SnapshotInput::load_for_run(&db, "not-a-run").is_err());
}
