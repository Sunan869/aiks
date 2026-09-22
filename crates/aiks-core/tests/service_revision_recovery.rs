use std::{sync::Arc, time::Duration};

use aiks_core::{
    ai::config::AiModelConfig,
    model::{pipeline::PipelineStage, SourceKind},
    pipeline::{job_repo::PipelineJobRepo, repo::PipelineRepo, EmbeddingConfig, PipelineWorker},
    service::{validate_submission, ServiceStore},
    storage::StateDb,
};

#[path = "support/service_fixture.rs"]
mod fixture;

#[tokio::test]
async fn restart_retires_obsolete_retry_and_expired_lease_but_completes_current_revision() {
    for retry in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state.db");
        let (old, current) = {
            let db = Arc::new(StateDb::open_exclusive(&path).unwrap());
            let store = ServiceStore::open(db.clone()).unwrap();
            let ctx = store.local_context();
            let reg = store
                .register_source(&ctx, SourceKind::Continue, "device")
                .unwrap();
            let mut request = fixture::submission(
                ctx.space_id(),
                ctx.instance_id(),
                &reg,
                "first",
                0,
                "OLD_RETRY_TEXT",
            );
            let old = store
                .accept(&ctx, &validate_submission(&request).unwrap())
                .unwrap()
                .0;
            let jobs = PipelineJobRepo::new(&db);
            let claimed = jobs.claim_next_snapshot().unwrap().unwrap();
            assert_eq!(claimed.durable_job_id, old.job_id);
            PipelineRepo::new(&db)
                .mark_started(&old.pipeline_run_id)
                .unwrap();
            if retry {
                PipelineRepo::new(&db)
                    .mark_failed(
                        &old.pipeline_run_id,
                        "AI_EXTRACTED",
                        "synthetic model failure",
                    )
                    .unwrap();
                jobs.mark_failed(&old.job_id, "synthetic model failure")
                    .unwrap();
            }
            request.submission_id = "current".into();
            request.expected_revision = 1;
            request.session.messages[0].blocks[0] = aiks_core::ContentBlock::Text {
                text: "CURRENT_AFTER_RESTART".into(),
            };
            let current = store
                .accept(&ctx, &validate_submission(&request).unwrap())
                .unwrap()
                .0;
            if !retry {
                db.conn().execute(
                    "UPDATE pipeline_job SET lease_until='2000-01-01T00:00:00+00:00' WHERE id=?1",
                    [&old.job_id],
                ).unwrap();
            }
            (old, current)
        };
        let db = Arc::new(StateDb::open_exclusive(&path).unwrap());
        let worker = PipelineWorker::start_from_snapshots(
            db.clone(),
            AiModelConfig {
                enabled: false,
                ..Default::default()
            },
            EmbeddingConfig {
                enabled: false,
                ..Default::default()
            },
        );
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let status: String = db
                    .conn()
                    .query_row(
                        "SELECT status FROM pipeline_job WHERE id=?1",
                        [&current.job_id],
                        |row| row.get(0),
                    )
                    .unwrap();
                if status == "DONE" {
                    break;
                }
                assert!(!matches!(status.as_str(), "FAILED" | "SUPERSEDED"));
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        worker.shutdown(Duration::from_secs(2)).await.unwrap();
        let conn = db.conn();
        let (status, attempts): (String, i64) = conn
            .query_row(
                "SELECT status,attempt FROM pipeline_job WHERE id=?1",
                [&old.job_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "SUPERSEDED");
        assert_eq!(
            attempts, 1,
            "an obsolete snapshot must not make another model attempt"
        );
        let status: String = conn
            .query_row(
                "SELECT status FROM pipeline_run WHERE id=?1",
                [&old.pipeline_run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "SUPERSEDED");
        let (indexed, completed): (u32, u32) = conn
            .query_row(
                "SELECT indexed_revision,completed_revision FROM service_derived_state",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((indexed, completed), (2, 2));
        let content: String = conn
            .query_row("SELECT content FROM session_search_fts", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(content.contains("CURRENT_AFTER_RESTART"));
        assert!(!content.contains("OLD_RETRY_TEXT"));
    }
}

#[test]
fn superseded_status_has_stable_wire_identity_and_is_not_ready_or_failed() {
    let status = PipelineStage::from_str("SUPERSEDED");
    assert_eq!(status, PipelineStage::Superseded);
    assert_ne!(status, PipelineStage::Ready);
    assert_ne!(status, PipelineStage::Failed);
    assert_eq!(status.as_str(), "SUPERSEDED");
    assert_eq!(serde_json::to_string(&status).unwrap(), "\"SUPERSEDED\"");
    assert_eq!(
        serde_json::from_str::<PipelineStage>("\"SUPERSEDED\"").unwrap(),
        status
    );
}
