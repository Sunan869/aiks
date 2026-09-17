from pathlib import Path


def replace_once(path: str, old: str, new: str, label: str) -> None:
    target = Path(path)
    text = target.read_text()
    if old not in text:
        raise SystemExit(f"missing replacement target: {label}")
    target.write_text(text.replace(old, new, 1))


session = "crates/aiks-core/src/indexing/session.rs"
replace_once(
    session,
    '''        let (vectors, actual_dimensions) = match embed_result {
            Ok(value) => value,
            Err(error) => {
                self.mark_failed(input.session_id, &content_hash, &error.to_string())?;
                return Err(error);
            }
        };

        self.finish_rebuild(
            input.session_id,
            &content_hash,
            &prepared,
            embedding_model.as_deref(),
            actual_dimensions,
            &vectors,
        )?;

        Ok(SessionIndexResult {
            content_hash,
            chunk_count: prepared.len(),
            embedded_count: vectors.len(),
            skipped: false,
        })''',
    '''        let (vectors, actual_dimensions, final_embedding_model, embedding_error) =
            match embed_result {
                Ok((vectors, dimensions)) => (
                    vectors,
                    dimensions,
                    embedding_model.as_deref(),
                    None,
                ),
                Err(error) => {
                    let message = error.to_string();
                    (Vec::new(), None, None, Some(message))
                }
            };

        self.finish_rebuild(
            input.session_id,
            &content_hash,
            &prepared,
            final_embedding_model,
            actual_dimensions,
            &vectors,
            embedding_error.as_deref(),
        )?;

        Ok(SessionIndexResult {
            content_hash,
            chunk_count: prepared.len(),
            embedded_count: vectors.len(),
            skipped: false,
        })''',
    "embedding degradation",
)
replace_once(
    session,
    '''        vectors: &[Vec<f32>],
    ) -> anyhow::Result<()> {''',
    '''        vectors: &[Vec<f32>],
        last_error: Option<&str>,
    ) -> anyhow::Result<()> {''',
    "finish_rebuild signature",
)
replace_once(
    session,
    '''                 SET status = 'ready', indexed_at = ?3, embedding_model = ?4,
                     embedding_dimensions = ?5, chunk_count = ?6, last_error = NULL
                 WHERE session_id = ?1 AND indexed_hash = ?2 AND status = 'indexing'",
                params![
                    session_id,
                    content_hash,
                    now,
                    embedding_model,
                    dimensions.map(|value| value as i64),
                    chunks.len() as i64
                ],''',
    '''                 SET status = 'ready', indexed_at = ?3, embedding_model = ?4,
                     embedding_dimensions = ?5, chunk_count = ?6, last_error = ?7
                 WHERE session_id = ?1 AND indexed_hash = ?2 AND status = 'indexing'",
                params![
                    session_id,
                    content_hash,
                    now,
                    embedding_model,
                    dimensions.map(|value| value as i64),
                    chunks.len() as i64,
                    last_error
                ],''',
    "finish_rebuild state",
)
replace_once(
    session,
    '''    fn mark_failed(&self, session_id: i64, content_hash: &str, error: &str) -> anyhow::Result<()> {
        let conn = self.db.conn();
        conn.execute(
            "UPDATE session_index_state
             SET status = 'failed', indexed_at = NULL, embedding_model = NULL,
                 embedding_dimensions = NULL, last_error = ?3
             WHERE session_id = ?1 AND indexed_hash = ?2",
            params![session_id, content_hash, error],
        )?;
        Ok(())
    }

''',
    '',
    "remove obsolete mark_failed",
)

worker = "crates/aiks-core/src/pipeline/worker.rs"
replace_once(
    worker,
    '''use crate::ai::config::AiModelConfig;
use crate::model::SourceKind;''',
    '''use crate::ai::{config::AiModelConfig, ModelService};
use crate::config::ContentConfig;
use crate::indexing::{SessionIndexInput, SessionIndexService};
use crate::model::SourceKind;''',
    "worker imports",
)
replace_once(
    worker,
    '''use crate::providers::{ProviderRegistry, SessionProvider, SessionSummary};
use crate::storage::StateDb;''',
    '''use crate::providers::{ProviderRegistry, SessionProvider, SessionSummary};
use crate::renderer::MarkdownRenderer;
use crate::storage::StateDb;''',
    "renderer import",
)
replace_once(
    worker,
    '''    const STAGES: [&str; 8] = [
        "PARSED",
        "CLEANED",
        "LLM_CHUNKED",
        "AI_EXTRACTED",
        "EMBED_CHUNKED",
        "EMBEDDED",
        "INDEXED",
        "DISCOVERED",
    ];''',
    '''    const STAGES: [&str; 9] = [
        "PARSED",
        "CLEANED",
        "SESSION_INDEX",
        "LLM_CHUNKED",
        "AI_EXTRACTED",
        "EMBED_CHUNKED",
        "EMBEDDED",
        "INDEXED",
        "DISCOVERED",
    ];''',
    "session index error stage",
)
replace_once(
    worker,
    '''async fn run_pipeline(
    db: &StateDb,
    registry: &ProviderRegistry,
    discovery_cache: &ProviderDiscoveryCache,
    ai_config: &AiModelConfig,
    _embedding_config: &EmbeddingConfig,
    job: &PipelineJob,
) -> anyhow::Result<()> {''',
    '''async fn run_pipeline(
    db: &Arc<StateDb>,
    registry: &ProviderRegistry,
    discovery_cache: &ProviderDiscoveryCache,
    ai_config: &AiModelConfig,
    embedding_config: &EmbeddingConfig,
    job: &PipelineJob,
) -> anyhow::Result<()> {''',
    "run_pipeline signature",
)
replace_once(
    worker,
    '''    if clean_result.cleaned_count == 0 {
        repo.mark_finished(run_id, "RAW_ONLY")?;
        return Ok(());
    }

    // ── Stage 3: LLM CHUNK ───────────────────────────────────────────────────''',
    '''    if clean_result.cleaned_count == 0 {
        repo.mark_finished(run_id, "RAW_ONLY")?;
        return Ok(());
    }

    // Session search is independent from AI extraction and from canonical
    // Knowledge indexing. Keep the cleaned session searchable even when AI is
    // disabled; semantic embedding failures degrade inside SessionIndexService.
    let mut searchable_session = session.clone();
    searchable_session.messages = clean_result.messages.clone();
    let normalized_text =
        MarkdownRenderer::new(ContentConfig::default(), true).render(&searchable_session);
    let models = Arc::new(ModelService::new(
        ai_config.clone(),
        embedding_config.clone(),
    )?);
    SessionIndexService::new(Arc::clone(db), models)
        .index_session(SessionIndexInput {
            session_id: job.session_id,
            external_id: job.session_external_id.clone(),
            source: job.source.clone(),
            title: job.session_title.clone().or_else(|| session.title.clone()),
            normalized_text,
        })
        .await
        .map_err(|error| anyhow::anyhow!("SESSION_INDEX: {error}"))?;

    // ── Stage 3: LLM CHUNK ───────────────────────────────────────────────────''',
    "automatic session indexing",
)
