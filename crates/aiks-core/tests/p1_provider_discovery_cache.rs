use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use aiks_core::{
    ai::AiModelConfig,
    model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind},
    pipeline::{
        embedding_client::EmbeddingConfig, repo::PipelineRepo, PipelineJob, PipelineOrchestrator,
        PipelineWorker,
    },
    providers::{ProviderHealth, ProviderRegistry, SessionProvider, SessionSummary},
    storage::{SourceSessionRepo, StateDb},
};
use async_trait::async_trait;

struct CountingProvider {
    summaries: Vec<SessionSummary>,
    discover_calls: Arc<AtomicUsize>,
    load_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl SessionProvider for CountingProvider {
    fn source(&self) -> SourceKind {
        SourceKind::ClaudeCode
    }

    fn parser_version(&self) -> &'static str {
        "counting-v1"
    }

    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        self.discover_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.summaries.clone())
    }

    async fn load_session(&self, summary: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        self.load_calls.fetch_add(1, Ordering::SeqCst);
        Ok(NormalizedSession {
            source: SourceKind::ClaudeCode,
            external_session_id: summary.external_session_id.clone(),
            title: summary.title.clone(),
            project_name: summary.project_name.clone(),
            project_path: summary.project_path.clone(),
            source_path: summary.source_path.clone(),
            started_at: summary.started_at,
            updated_at: summary.updated_at,
            model: None,
            messages: vec![NormalizedMessage {
                external_id: format!("msg-{}", summary.external_session_id),
                parent_id: None,
                role: MessageRole::User,
                created_at: None,
                model: None,
                blocks: vec![ContentBlock::Text {
                    text: format!("useful content for {}", summary.external_session_id),
                }],
                usage: None,
                metadata: Default::default(),
            }],
            usage: None,
            metadata: Default::default(),
        })
    }

    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Ok
    }
}

fn summaries(count: usize) -> Vec<SessionSummary> {
    (0..count)
        .map(|i| SessionSummary {
            source: SourceKind::ClaudeCode,
            external_session_id: format!("session-{i}"),
            title: Some(format!("Session {i}")),
            project_name: Some("provider-cache".into()),
            project_path: None,
            source_path: None,
            started_at: None,
            updated_at: None,
            message_count: 1,
        })
        .collect()
}

#[tokio::test]
async fn multi_session_backfill_discovers_provider_once_not_once_per_job() {
    const SESSION_COUNT: usize = 8;

    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    let discover_calls = Arc::new(AtomicUsize::new(0));
    let load_calls = Arc::new(AtomicUsize::new(0));
    let provider_summaries = summaries(SESSION_COUNT);

    let registry = Arc::new(ProviderRegistry::new(vec![Box::new(CountingProvider {
        summaries: provider_summaries.clone(),
        discover_calls: Arc::clone(&discover_calls),
        load_calls: Arc::clone(&load_calls),
    })]));

    let ai = AiModelConfig {
        enabled: false,
        ..Default::default()
    };
    let worker = PipelineWorker::start_with_limit(
        Arc::clone(&db),
        registry,
        ai,
        EmbeddingConfig::default(),
        4,
    );
    let orchestrator = PipelineOrchestrator::new(Arc::clone(&db));

    let mut run_ids = Vec::new();
    for summary in &provider_summaries {
        let session_id = SourceSessionRepo::new(&db)
            .upsert(
                summary.source.as_str(),
                &summary.external_session_id,
                None,
                None,
                summary.project_name.as_deref(),
                summary.title.as_deref(),
                None,
                Some(&format!("hash-{}", summary.external_session_id)),
                Some("counting-v1"),
            )
            .unwrap();
        let run_id = orchestrator
            .enqueue(
                session_id,
                Some(&format!("hash-{}", summary.external_session_id)),
            )
            .unwrap();
        worker
            .submit(PipelineJob {
                pipeline_run_id: run_id.clone(),
                session_id,
                session_external_id: summary.external_session_id.clone(),
                source: summary.source.as_str().to_string(),
                session_title: summary.title.clone(),
                project_name: summary.project_name.clone(),
            })
            .unwrap();
        run_ids.push(run_id);
    }

    let completed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let done = run_ids.iter().all(|run_id| {
                PipelineRepo::new(&db)
                    .get_run_detail(run_id)
                    .ok()
                    .flatten()
                    .map(|run| run.status == "RAW_ONLY")
                    .unwrap_or(false)
            });
            if done {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await;

    assert!(
        completed.is_ok(),
        "pipeline backfill did not finish in time"
    );
    assert_eq!(load_calls.load(Ordering::SeqCst), SESSION_COUNT);
    assert_eq!(
        discover_calls.load(Ordering::SeqCst),
        1,
        "one backfill cycle should reuse one provider discovery snapshot"
    );
}
