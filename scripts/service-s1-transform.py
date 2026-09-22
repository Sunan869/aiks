"""Temporary, reviewed source transform. Produces blobs, never commits or refs.
Remove this file in the commit that adopts the generated source blobs.
"""
from pathlib import Path
import subprocess


def read_known(path: str, sha: str) -> str:
    actual = subprocess.check_output(['git', 'rev-parse', 'HEAD:' + path], text=True).strip()
    assert actual == sha, (path, actual, sha)
    return Path(path).read_text()


def one(text: str, old: str, new: str) -> str:
    assert text.count(old) == 1, (old[:100], text.count(old))
    return text.replace(old, new, 1)


path = 'crates/aiks-core/src/service/validation.rs'
text = Path(path).read_text()
text = one(text, '''    let content_hash = canonical_hash(&serde_json::json!({
        "parser_version": request.parser_version,
        "session": session,
    }))?;''', '''    let content_hash = snapshot_content_hash(&request.parser_version, session)?;''')
text += '''

pub(crate) fn snapshot_content_hash(
    parser_version: &str,
    session: &Value,
) -> Result<String, ServiceError> {
    canonical_hash(&serde_json::json!({
        "parser_version": parser_version,
        "session": session,
    }))
}
'''
Path(path).write_text(text)

path = 'crates/aiks-core/src/pipeline/job_repo.rs'
text = Path(path).read_text()
text = one(text, '''    pub fn claim_next(&self) -> anyhow::Result<Option<ClaimedPipelineJob>> {
''', '''    pub fn claim_next(&self) -> anyhow::Result<Option<ClaimedPipelineJob>> {
        self.claim_next_for_input(false)
    }

    pub fn claim_next_snapshot(&self) -> anyhow::Result<Option<ClaimedPipelineJob>> {
        self.claim_next_for_input(true)
    }

    fn claim_next_for_input(
        &self,
        snapshot_only: bool,
    ) -> anyhow::Result<Option<ClaimedPipelineJob>> {
''')
text = one(text, '''                   AND pj.pipeline_run_id IS NOT NULL
                   AND (pj.available_at''', '''                   AND pj.pipeline_run_id IS NOT NULL
                   AND EXISTS (
                       SELECT 1 FROM service_job_input si
                       WHERE si.pipeline_run_id=pj.pipeline_run_id AND si.durable_job_id=pj.id
                   ) = ?2
                   AND (pj.available_at''')
text = one(text, '''                 LIMIT 1",
                params![now],''', '''                 LIMIT 1",
                params![now, snapshot_only],''')
Path(path).write_text(text)

path = 'crates/aiks-core/src/pipeline/worker.rs'
text = read_known(path, '7c2fbae9a8962d4eef3d88b28d20f47b5f30c98e')
text = one(text, 'use tokio::sync::{mpsc, oneshot, Mutex, Semaphore};', '''use tokio::sync::{mpsc, watch, Mutex, Semaphore};
use tokio::task::{JoinHandle, JoinSet};

use super::input::{PipelineInputSource, SnapshotInput};''')
start = text.index('pub struct PipelineWorker {')
end = text.index('/// Seed the durable queue', start)
text = text[:start] + '''pub struct PipelineWorker {
    db: Arc<StateDb>,
    wake_tx: mpsc::Sender<()>,
    snapshot_only: bool,
    stop: watch::Sender<Option<tokio::time::Instant>>,
    supervisor: Mutex<Option<JoinHandle<anyhow::Result<()>>>>,
}

struct WorkerControl {
    wake_tx: mpsc::Sender<()>,
    semaphore: Arc<Semaphore>,
    stop_rx: watch::Receiver<Option<tokio::time::Instant>>,
}

impl PipelineWorker {
    pub fn start(
        db: Arc<StateDb>,
        registry: Arc<ProviderRegistry>,
        ai_config: AiModelConfig,
        embedding_config: EmbeddingConfig,
    ) -> Self {
        let limit = ai_config.max_concurrent.max(1);
        Self::start_with_limit(db, registry, ai_config, embedding_config, limit)
    }

    pub fn start_with_limit(
        db: Arc<StateDb>,
        registry: Arc<ProviderRegistry>,
        ai_config: AiModelConfig,
        embedding_config: EmbeddingConfig,
        max_concurrent: usize,
    ) -> Self {
        Self::start_with_input(
            db, PipelineInputSource::LegacyProviders(registry),
            ai_config, embedding_config, max_concurrent,
        )
    }

    /// Service mode has no ProviderRegistry and cannot scan employee files.
    pub fn start_from_snapshots(
        db: Arc<StateDb>,
        ai_config: AiModelConfig,
        embedding_config: EmbeddingConfig,
    ) -> Self {
        let limit = ai_config.max_concurrent.max(1);
        Self::start_with_input(
            db, PipelineInputSource::PersistedSnapshots,
            ai_config, embedding_config, limit,
        )
    }

    fn start_with_input(
        db: Arc<StateDb>,
        input: PipelineInputSource,
        ai_config: AiModelConfig,
        embedding_config: EmbeddingConfig,
        max_concurrent: usize,
    ) -> Self {
        let (wake_tx, wake_rx) = mpsc::channel(1);
        let (stop, stop_rx) = watch::channel(None);
        let control = WorkerControl {
            wake_tx: wake_tx.clone(),
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
            stop_rx,
        };
        let snapshot_only = input.is_snapshot();
        let supervisor = tokio::spawn(run_worker(
            wake_rx, db.clone(), input, ai_config, embedding_config, control,
        ));
        let worker = Self {
            db, wake_tx, snapshot_only, stop,
            supervisor: Mutex::new(Some(supervisor)),
        };
        worker.wake();
        worker
    }

    pub fn wake(&self) {
        let _ = self.wake_tx.try_send(());
    }

    /// Legacy submit remains compatible; service ingestion commits its own
    /// snapshot/run/job/receipt transaction before waking this worker.
    pub fn submit(&self, job: PipelineJob) -> anyhow::Result<()> {
        anyhow::ensure!(!self.snapshot_only, "Snapshot jobs require atomic ingestion");
        PipelineJobRepo::new(&self.db).enqueue(&job)?;
        self.wake();
        Ok(())
    }

    /// Stop claiming work, drain tasks until the deadline, then cancel and join
    /// any remaining children. Cancelling this future does not detach the handle.
    pub async fn shutdown(&self, grace: Duration) -> anyhow::Result<()> {
        let deadline = tokio::time::Instant::now() + grace;
        self.stop.send_if_modified(|current| {
            if current.is_none() {
                *current = Some(deadline);
                true
            } else {
                false
            }
        });
        let mut guard = self.supervisor.lock().await;
        let Some(handle) = guard.as_mut() else { return Ok(()); };
        let result = handle.await;
        *guard = None;
        result?
    }
}

impl Drop for PipelineWorker {
    fn drop(&mut self) {
        self.stop.send_replace(Some(tokio::time::Instant::now()));
    }
}

''' + text[end:]
start = text.index('async fn run_worker(')
end = text.index('/// R13: map a run_pipeline', start)
part = text[start:end]
part = one(part, '    registry: Arc<ProviderRegistry>,', '    input: PipelineInputSource,')
part = one(part, '''    wake_tx: mpsc::Sender<()>,
    semaphore: Arc<Semaphore>,
) {''', '''    control: WorkerControl,
) -> anyhow::Result<()> {
    let WorkerControl { wake_tx, semaphore, mut stop_rx } = control;
    let mut tasks = JoinSet::new();''')
part = one(part, '''    loop {
        tokio::select! {
            signal = wake_rx.recv()''', '''    'supervisor: loop {
        if stop_rx.borrow().is_some() {
            break;
        }
        tokio::select! {
            biased;
            _ = stop_rx.changed() => break,
            completed = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(Err(error)) = completed {
                    warn!(error = %error, "[PIPELINE] Task stopped unexpectedly");
                }
            }
            signal = wake_rx.recv()''')
part = one(part, '''        loop {
            let permit''', '''        loop {
            if stop_rx.borrow().is_some() {
                break 'supervisor;
            }
            let permit''')
part = one(part, 'Err(tokio::sync::TryAcquireError::Closed) => return,', 'Err(tokio::sync::TryAcquireError::Closed) => break \'supervisor,')
part = one(part, '''            let claimed = match PipelineJobRepo::new(&db).claim_next() {''', '''            let repo = PipelineJobRepo::new(&db);
            let next = if input.is_snapshot() {
                repo.claim_next_snapshot()
            } else {
                repo.claim_next()
            };
            let claimed = match next {''')
part = one(part, 'let task_registry = Arc::clone(&registry);', 'let task_input = input.clone();')
part = one(part, '''            tokio::spawn(async move {
                let durable_job_id''', '''            tasks.spawn(async move {
                let durable_job_id''')
heartbeat_start = part.index('                // Keep long AI/embedding jobs leased')
heartbeat_end = part.index('                let durable_repo', heartbeat_start)
part = part[:heartbeat_start] + '''                // Lease renewal shares the child future, so aborting a task
                // cannot leave a detached heartbeat holding the database open.
                let execution = run_pipeline(
                    &task_db, &task_input, &task_discovery_cache,
                    &task_ai, &task_embedding, &job,
                );
                tokio::pin!(execution);
                let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
                heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                heartbeat.tick().await;
                let result = loop {
                    tokio::select! {
                        result = &mut execution => break result,
                        _ = heartbeat.tick() => {
                            match PipelineJobRepo::new(&task_db).renew_lease(&durable_job_id) {
                                Ok(true) => {}
                                Ok(false) => break Err(anyhow::anyhow!("Pipeline lease was lost")),
                                Err(error) => break Err(error),
                            }
                        }
                    }
                };

''' + part[heartbeat_end:]
part = one(part, '''    info!("[PIPELINE] Durable worker stopped");
}''', '''    let deadline = (*stop_rx.borrow()).unwrap_or_else(tokio::time::Instant::now);
    let drained = tokio::time::timeout_at(deadline, async {
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result {
                warn!(error = %error, "[PIPELINE] Task stopped during shutdown");
            }
        }
    }).await;
    if drained.is_err() {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        anyhow::bail!("Pipeline shutdown deadline reached; unfinished leases remain recoverable");
    }
    info!("[PIPELINE] Durable worker stopped");
    Ok(())
}''')
text = text[:start] + part + text[end:]
text = one(text, '''    registry: &ProviderRegistry,
    discovery_cache:''', '''    input: &PipelineInputSource,
    discovery_cache:''')
start = text.index('    // Find source kind', text.index('async fn run_pipeline('))
end = text.index('    repo.record_stage(', start)
legacy = text[start:end]
text = text[:start] + '''    let session = match input {
        PipelineInputSource::LegacyProviders(registry) => {
''' + legacy + '''            session
        }
        PipelineInputSource::PersistedSnapshots => {
            match SnapshotInput::load_for_run(db, run_id) {
                Ok((session, _fence)) => session,
                Err(error) => fail_stage!("PARSED", error),
            }
        }
    };
    // A later upload may change source_session metadata while an older run
    // is leased. Its processing input must retain its own snapshot metadata.
    let mut effective_job = job.clone();
    if input.is_snapshot() {
        effective_job.session_title = session.title.clone();
        effective_job.project_name = session.project_name.clone();
    }
    let job = &effective_job;

''' + text[end:]
Path(path).write_text(text)
print('Applied guarded Task 4 candidate to validation.rs, job_repo.rs, worker.rs')
