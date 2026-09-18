/// Legacy knowledge-search compatibility surface.
///
/// V4.2 has exactly one search implementation: `UnifiedSearchService`.
/// This module keeps the pre-V4.2 return types/API for existing callers, but
/// performs no independent lexical/vector recall, fusion, or ranking.
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;

use crate::indexing::EmbeddingProvider;
use crate::pipeline::embedding_client::{EmbeddingClient, EmbeddingConfig};
use crate::search::{SearchCorpus, UnifiedSearchFilter, UnifiedSearchService};
use crate::storage::StateDb;

/// Kept for source compatibility with the legacy bounded-candidate regression
/// test. Candidate bounding for V4.2 search is owned by `UnifiedSearchService`.
pub const VECTOR_CANDIDATE_CAP: usize = 512;

#[derive(Debug)]
pub struct SearchHit {
    pub knowledge_id: String,
    pub chunk_id: String,
    pub chunk_text: String,
    pub score: f32,
    pub match_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDegradationKind {
    FtsFallback,
    VectorUnavailable,
}

#[derive(Debug, Clone)]
pub struct SearchDegradation {
    pub kind: SearchDegradationKind,
    pub message: String,
}

#[derive(Debug)]
pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    pub degradations: Vec<SearchDegradation>,
}

impl SearchOutcome {
    pub fn degraded(&self) -> bool {
        !self.degradations.is_empty()
    }
}

#[derive(Clone)]
struct LegacyEmbeddingProvider {
    config: Option<EmbeddingConfig>,
}

impl LegacyEmbeddingProvider {
    fn new(config: Option<&EmbeddingConfig>) -> Self {
        Self {
            config: config.cloned(),
        }
    }

    fn active_config(&self) -> Option<&EmbeddingConfig> {
        self.config
            .as_ref()
            .filter(|cfg| cfg.enabled && !cfg.base_url.trim().is_empty())
    }
}

#[async_trait]
impl EmbeddingProvider for LegacyEmbeddingProvider {
    fn enabled(&self) -> bool {
        self.active_config().is_some()
    }

    fn model_name(&self) -> &str {
        self.config
            .as_ref()
            .map(|cfg| cfg.model.as_str())
            .unwrap_or("")
    }

    fn dimensions(&self) -> Option<usize> {
        self.config.as_ref().and_then(|cfg| cfg.dimensions)
    }

    fn batch_size(&self) -> usize {
        self.config
            .as_ref()
            .map(|cfg| cfg.batch_size.max(1))
            .unwrap_or(16)
    }

    fn chunk_target_tokens(&self) -> usize {
        self.config
            .as_ref()
            .map(|cfg| cfg.chunk_target_tokens.max(1))
            .unwrap_or(800)
    }

    fn chunk_overlap_tokens(&self) -> usize {
        self.config
            .as_ref()
            .map(|cfg| cfg.chunk_overlap_tokens)
            .unwrap_or(120)
    }

    async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>> {
        let config = self
            .active_config()
            .cloned()
            .ok_or_else(|| anyhow!("embedding is disabled"))?;
        EmbeddingClient::new(config)?.embed_batch(texts).await
    }
}

/// Compatibility wrapper for existing callers that only need knowledge hits.
pub async fn hybrid_search(
    db: &Arc<StateDb>,
    query: &str,
    limit: usize,
    embedding_config: Option<&EmbeddingConfig>,
) -> anyhow::Result<Vec<SearchHit>> {
    Ok(search_with_status(db, query, limit, embedding_config)
        .await?
        .hits)
}

/// Compatibility wrapper over the V4.2 unified search service.
pub async fn search_with_status(
    db: &Arc<StateDb>,
    query: &str,
    limit: usize,
    embedding_config: Option<&EmbeddingConfig>,
) -> anyhow::Result<SearchOutcome> {
    let embedding_requested = embedding_config
        .map(|cfg| cfg.enabled && !cfg.base_url.trim().is_empty())
        .unwrap_or(false);
    let embeddings: Arc<dyn EmbeddingProvider> =
        Arc::new(LegacyEmbeddingProvider::new(embedding_config));

    let outcome = UnifiedSearchService::new(Arc::clone(db), embeddings)
        .search(
            query,
            limit,
            UnifiedSearchFilter {
                corpora: vec![SearchCorpus::Knowledge],
                project: None,
                source: None,
            },
        )
        .await?;

    let degradations = outcome
        .warnings
        .into_iter()
        .filter_map(|message| {
            if !embedding_requested && message.starts_with("Semantic search is disabled") {
                return None;
            }

            let kind = if message.starts_with("Semantic search") {
                SearchDegradationKind::VectorUnavailable
            } else {
                SearchDegradationKind::FtsFallback
            };
            Some(SearchDegradation { kind, message })
        })
        .collect();

    let hits = outcome
        .hits
        .into_iter()
        .map(|hit| {
            let lexical = hit.match_types.iter().any(|kind| kind == "lexical");
            let semantic = hit.match_types.iter().any(|kind| kind == "semantic");
            let match_type = match (lexical, semantic) {
                (true, true) => "hybrid",
                (false, true) => "vector",
                _ => "fts",
            }
            .to_string();

            SearchHit {
                knowledge_id: hit.entity_id,
                chunk_id: hit.chunk_id.unwrap_or_default(),
                chunk_text: hit.snippet,
                score: hit.score,
                match_type,
            }
        })
        .collect();

    Ok(SearchOutcome { hits, degradations })
}
