use std::{collections::HashMap, sync::Arc, time::Duration};

use aiks_core::{
    ai::config::AiModelConfig,
    indexing::{EmbeddingProvider, SessionIndexInput, SessionIndexService},
    model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind},
    pipeline::{EmbeddingConfig, PipelineJob, PipelineOrchestrator, PipelineWorker},
    providers::{ProviderHealth, ProviderRegistry, SessionProvider, SessionSummary},
    storage::{SourceSessionRepo, StateDb},
};
use async_trait::async_trait;

const SESSION_ID: &str = "v42-session-index";
const SEARCH_TEXT: &str = "kubernetes 节点磁盘空间不足 /var/lib/kubelet/pods";

fn message(text: &str) -> NormalizedMessage {
    NormalizedMessage {
        external_id: "v42-message".into(),
        parent_id: None,
        role: MessageRole::User,
        created_at: None,
        model: None,
        blocks: vec![ContentBlock::Text { text: text.into() }],
        usage: None,
        metadata: HashMap::new(),
    }
}

fn session() -> NormalizedSession {
    NormalizedSession {
        source: SourceKind::ClaudeCode,
        external_session_id: SESSION_ID.into(),
        title: Some("V4.2 Session Index".into()),
        project_name: Some("aiks".into()),
        project_path: None,
        source_path: None,
        started_at: None,
        updated_at: None,
        model: None,
        messages: vec![message(SEARCH_TEXT)],
        usage: None,
        metadata: HashMap::new(),
    }
}

struct FakeProvider;

#[async_trait]
impl SessionProvider for FakeProvider {
    fn source(&self) -> SourceKind {
        SourceKind::ClaudeCode
    }

    fn parser_version(&self) -> &'static str {
        "v42-test"
    }

    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Ok
    }

    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        Ok(vec![SessionSummary {
            source: SourceKind::ClaudeCode,
            external_session_id: SESSION_ID.into(),
            title: Some("V4.2 Session Index".into()),
            project_name: Some("aiks".into()),
            project_path: None,
            source_path: None,
            started_at: None,
            updated_at: None,
            message_count: 1,
        }])
    }

    async fn load_session(&self, _: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        Ok(session())
    }
}

fn insert_session(db: &StateDb) -> i64 {
    SourceSessionRepo::new(db)
        .upsert(
            "claude_code",
            SESSION_ID,
            Some("V4.2 Session Index"),
            Some("aiks"),
            None,
            None,
            None,
            Some("v42-hash"),
            Some("v42-test"),
        )
        .unwrap()
}

async fn wait_for_terminal(db: &StateDb, run_id: &str) -> String {
    for _ in 0..200 {
        let status: String = db
            .conn()
            .query_row(
                "SELECT status FROM pipeline_run WHERE id = ?1",
                [run_id],
                |row| row.get(0),
            )
            .unwrap();
        if status != "DISCOVERED" && status != "PROCESSING" {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("pipeline run did not reach a terminal state");
}

// Contract: lexical session search is available independently of AI extraction.
#[tokio::test]
async fn ai_disabled_pipeline_still_builds_searchable_session_index() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    let session_id = insert_session(&db);
    let run_id = PipelineOrchestrator::new(db.clone())
        .enqueue(session_id, Some("v42-hash"))
        .unwrap();

    let worker = PipelineWorker::start_with_limit(
        db.clone(),
        Arc::new(ProviderRegistry::new(vec![Box::new(FakeProvider)])),
        AiModelConfig {
            enabled: false,
            ..Default::default()
        },
        EmbeddingConfig {
            enabled: false,
            ..Default::default()
        },
        1,
    );

    worker
        .submit(PipelineJob {
            pipeline_run_id: run_id.clone(),
            session_id,
            session_external_id: SESSION_ID.into(),
            source: "claude_code".into(),
            session_title: Some("V4.2 Session Index".into()),
            project_name: Some("aiks".into()),
        })
        .unwrap();

    assert_eq!(wait_for_terminal(&db, &run_id).await, "RAW_ONLY");

    let searchable: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM session_search_fts
             WHERE session_search_fts MATCH 'kubernetes' AND session_id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(searchable, 1, "AI-disabled sessions must remain searchable");

    let state: String = db
        .conn()
        .query_row(
            "SELECT status FROM session_index_state WHERE session_id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, "ready");
}

struct FailingEmbedding;

#[async_trait]
impl EmbeddingProvider for FailingEmbedding {
    fn enabled(&self) -> bool {
        true
    }

    fn model_name(&self) -> &str {
        "v42-failing-embedding"
    }

    fn dimensions(&self) -> Option<usize> {
        Some(3)
    }

    async fn embed(&self, _texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        anyhow::bail!("forced embedding failure")
    }
}

// Contract: semantic indexing may degrade, but lexical search must remain ready.
#[tokio::test]
async fn embedding_failure_degrades_to_lexical_session_index() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap());
    let session_id = insert_session(&db);
    let service = SessionIndexService::new(db.clone(), Arc::new(FailingEmbedding));

    let result = service
        .index_session(SessionIndexInput {
            session_id,
            external_id: SESSION_ID.into(),
            source: "claude_code".into(),
            title: Some("V4.2 Session Index".into()),
            normalized_text: SEARCH_TEXT.into(),
        })
        .await
        .expect("embedding failure must not discard the lexical index");

    assert_eq!(result.embedded_count, 0);

    let searchable: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM session_search_fts
             WHERE session_search_fts MATCH 'kubernetes' AND session_id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(searchable, 1);

    let (status, last_error): (String, Option<String>) = db
        .conn()
        .query_row(
            "SELECT status, last_error FROM session_index_state WHERE session_id = ?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "ready");
    assert!(last_error
        .as_deref()
        .is_some_and(|error| error.contains("forced embedding failure")));
}
