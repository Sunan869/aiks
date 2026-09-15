use std::sync::Arc;

use aiks_core::{
    ai::config::AiModelConfig,
    pipeline::{
        job_repo::{FailureDisposition, PipelineJobRepo},
        recover_interrupted_runs, EmbeddingConfig, PipelineJob, PipelineOrchestrator,
        PipelineWorker,
    },
    providers::ProviderRegistry,
    storage::{SourceSessionRepo, StateDb},
};

fn insert_session(db: &StateDb, external_id: &str, hash: &str) -> i64 {
    SourceSessionRepo::new(db)
        .upsert(
            "claude_code",
            external_id,
            Some("P1 queue regression"),
            Some("aiks"),
            None,
            None,
            None,
            Some(hash),
            Some("p1-v1"),
        )
        .unwrap()
}

fn make_worker(db: Arc<StateDb>) -> PipelineWorker {
    let mut ai = AiModelConfig::default();
    ai.enabled = false;
    PipelineWorker::start_with_limit(
        db,
        Arc::new(ProviderRegistry::new(vec![])),
        ai,
        EmbeddingConfig::default(),
        1,
    )
}

fn make_job(run_id: String, session_id: i64, external_id: &str) -> PipelineJob {
    PipelineJob {
        pipeline_run_id: run_id,
        session_id,
        session_external_id: external_id.to_string(),
        source: "claude_code".to_string(),
        session_title: Some("P1 queue regression".to_string()),
        project_name: Some("aiks".to_string()),
    }
}

fn make_durable_job(db: &Arc<StateDb>, external_id: &str) -> PipelineJob {
    let session_id = insert_session(db, external_id, "hash-v1");
    let run_id = PipelineOrchestrator::new(db.clone())
        .enqueue(session_id, Some("hash-v1"))
        .unwrap();
    make_job(run_id, session_id, external_id)
}

#[tokio::test]
async fn submit_persists_pipeline_job_before_returning() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    let session_id = insert_session(&db, "p1-queue-session", "hash-v1");
    let run_id = PipelineOrchestrator::new(db.clone())
        .enqueue(session_id, Some("hash-v1"))
        .unwrap();
    let worker = make_worker(db.clone());

    worker
        .submit(make_job(run_id, session_id, "p1-queue-session"))
        .unwrap();

    let count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM pipeline_job WHERE source = 'claude_code' AND external_session_id = 'p1-queue-session'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "submit must durably persist work before returning");
}

#[tokio::test]
async fn duplicate_submit_has_only_one_active_durable_job() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    let session_id = insert_session(&db, "p1-duplicate-session", "hash-v1");
    let run_id = PipelineOrchestrator::new(db.clone())
        .enqueue(session_id, Some("hash-v1"))
        .unwrap();
    let worker = make_worker(db.clone());

    worker
        .submit(make_job(
            run_id.clone(),
            session_id,
            "p1-duplicate-session",
        ))
        .unwrap();
    worker
        .submit(make_job(run_id, session_id, "p1-duplicate-session"))
        .unwrap();

    let active: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM pipeline_job WHERE source = 'claude_code' AND external_session_id = 'p1-duplicate-session' AND status IN ('PENDING', 'RUNNING')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        active, 1,
        "duplicate submission must collapse to one active durable job"
    );
}

#[tokio::test]
async fn recover_interrupted_runs_seeds_the_durable_queue() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    let session_id = insert_session(&db, "p1-recovery-session", "hash-v1");
    PipelineOrchestrator::new(db.clone())
        .enqueue(session_id, Some("hash-v1"))
        .unwrap();
    let worker = make_worker(db.clone());

    let recovered = recover_interrupted_runs(&db, &worker);
    assert_eq!(recovered, 1);

    let count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM pipeline_job WHERE source = 'claude_code' AND external_session_id = 'p1-recovery-session'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "recovery must seed the durable job table");
}

#[test]
fn enqueue_persists_canonical_session_identity_not_redundant_external_id() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    let session_id = insert_session(&db, "canonical-session", "hash-v1");
    let run_id = PipelineOrchestrator::new(db.clone())
        .enqueue(session_id, Some("hash-v1"))
        .unwrap();

    let job = make_job(run_id.clone(), session_id, "caller-supplied-wrong-id");
    PipelineJobRepo::new(&db).enqueue(&job).unwrap();

    let (stored_session_id, stored_run_id, stored_external_id): (i64, String, String) = db
        .conn()
        .query_row(
            "SELECT session_id, pipeline_run_id, external_session_id FROM pipeline_job LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert_eq!(stored_session_id, session_id);
    assert_eq!(stored_run_id, run_id);
    assert_eq!(stored_external_id, "canonical-session");
}

#[test]
fn expired_running_lease_is_reclaimable() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    let job = make_durable_job(&db, "lease-recovery-session");
    let repo = PipelineJobRepo::new(&db);
    repo.enqueue(&job).unwrap();

    let first = repo.claim_next().unwrap().unwrap();
    assert_eq!(first.attempt, 1);

    db.conn()
        .execute(
            "UPDATE pipeline_job SET lease_until = '2000-01-01T00:00:00Z' WHERE id = ?1",
            [&first.durable_job_id],
        )
        .unwrap();

    let recovered = repo.claim_next().unwrap().unwrap();
    assert_eq!(recovered.durable_job_id, first.durable_job_id);
    assert_eq!(recovered.attempt, 2);
}

#[test]
fn retry_budget_is_persisted_and_terminal_after_three_attempts() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    let job = make_durable_job(&db, "retry-budget-session");
    let repo = PipelineJobRepo::new(&db);
    repo.enqueue(&job).unwrap();

    for expected_attempt in 1..=3 {
        let claimed = repo.claim_next().unwrap().unwrap();
        assert_eq!(claimed.attempt, expected_attempt);

        let disposition = repo
            .mark_failed(&claimed.durable_job_id, "forced regression failure")
            .unwrap();

        if expected_attempt < 3 {
            assert_eq!(disposition, FailureDisposition::Retry);
            db.conn()
                .execute(
                    "UPDATE pipeline_job SET available_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
                    [&claimed.durable_job_id],
                )
                .unwrap();
        } else {
            assert_eq!(disposition, FailureDisposition::Terminal);
        }
    }

    let (status, attempt): (String, i64) = db
        .conn()
        .query_row(
            "SELECT status, attempt FROM pipeline_job LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "FAILED");
    assert_eq!(attempt, 3);
    assert!(repo.claim_next().unwrap().is_none());
}
