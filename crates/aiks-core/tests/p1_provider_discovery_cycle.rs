//! A temporarily empty pending queue is not idle while a task is loading.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use aiks_core::{
    ai::AiModelConfig,
    model::{NormalizedSession, SourceKind},
    pipeline::{EmbeddingConfig, PipelineJob, PipelineOrchestrator, PipelineWorker},
    providers::{ProviderHealth, ProviderRegistry, SessionProvider, SessionSummary},
    storage::{SourceSessionRepo, StateDb},
};
use async_trait::async_trait;
use tokio::sync::Semaphore;

#[path = "support/service_fixture.rs"]
mod fixture;

struct GatedProvider {
    discoveries: Arc<AtomicUsize>,
    entered: Arc<Semaphore>,
    release: Arc<Semaphore>,
}

fn summary(index: usize) -> SessionSummary {
    SessionSummary {
        source: SourceKind::Continue,
        external_session_id: format!("cycle-{index}"),
        title: Some(format!("Cycle {index}")),
        project_name: None,
        project_path: None,
        source_path: None,
        started_at: None,
        updated_at: None,
        message_count: 1,
    }
}

#[async_trait]
impl SessionProvider for GatedProvider {
    fn source(&self) -> SourceKind {
        SourceKind::Continue
    }

    fn parser_version(&self) -> &'static str {
        "synthetic-cycle-v1"
    }

    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        self.discoveries.fetch_add(1, Ordering::SeqCst);
        Ok((0..3).map(summary).collect())
    }

    async fn load_session(&self, summary: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        self.entered.add_permits(1);
        self.release.acquire().await?.forget();
        let mut input = fixture::submission(
            "space",
            "instance",
            "registration",
            "u1",
            0,
            "Cycle content",
        );
        input.session.external_session_id = summary.external_session_id.clone();
        input.session.title = summary.title.clone();
        Ok(input.session)
    }

    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Ok
    }
}

fn submit(db: &Arc<StateDb>, worker: &PipelineWorker, index: usize) -> String {
    let summary = summary(index);
    let id = SourceSessionRepo::new(db)
        .upsert(
            "continue",
            &summary.external_session_id,
            None,
            None,
            None,
            summary.title.as_deref(),
            None,
            Some("synthetic-hash"),
            Some("synthetic-cycle-v1"),
        )
        .unwrap();
    let run = PipelineOrchestrator::new(db.clone())
        .enqueue(id, Some("synthetic-hash"))
        .unwrap();
    worker
        .submit(PipelineJob {
            pipeline_run_id: run.clone(),
            session_id: id,
            session_external_id: summary.external_session_id,
            source: "continue".into(),
            session_title: summary.title,
            project_name: None,
        })
        .unwrap();
    run
}

async fn entered(gate: &Semaphore) {
    tokio::time::timeout(Duration::from_secs(5), gate.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();
}

async fn done(db: &StateDb, runs: &[String]) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let complete = runs.iter().all(|run| {
                let status: String = db
                    .conn()
                    .query_row(
                        "SELECT status FROM pipeline_job WHERE pipeline_run_id=?1",
                        [run],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert!(!matches!(status.as_str(), "FAILED" | "SUPERSEDED"));
                status == "DONE"
            });
            if complete {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn active_load_keeps_discovery_snapshot_but_a_later_idle_cycle_refreshes_it() {
    let root = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&root.path().join("state.db")).unwrap());
    let discoveries = Arc::new(AtomicUsize::new(0));
    let loaded = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    let registry = Arc::new(ProviderRegistry::new(vec![Box::new(GatedProvider {
        discoveries: discoveries.clone(),
        entered: loaded.clone(),
        release: release.clone(),
    })]));
    let worker = PipelineWorker::start_with_limit(
        db.clone(),
        registry,
        AiModelConfig {
            enabled: false,
            ..Default::default()
        },
        EmbeddingConfig {
            enabled: false,
            ..Default::default()
        },
        4,
    );
    let first = submit(&db, &worker, 0);
    entered(&loaded).await;
    // The load is held across multiple supervisor polls, not merely slowed down.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let second = submit(&db, &worker, 1);
    entered(&loaded).await;
    assert_eq!(
        discoveries.load(Ordering::SeqCst),
        1,
        "active work is not an idle queue"
    );
    release.add_permits(2);
    done(&db, &[first, second]).await;
    // Give the drained supervisor a poll so this really is a distinct cycle.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let third = submit(&db, &worker, 2);
    entered(&loaded).await;
    assert_eq!(
        discoveries.load(Ordering::SeqCst),
        2,
        "true idle must discard stale discovery"
    );
    release.add_permits(1);
    done(&db, &[third]).await;
    worker.shutdown(Duration::from_secs(2)).await.unwrap();
}
