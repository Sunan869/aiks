use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rusqlite::{params_from_iter, types::Value};
use serde::{Deserialize, Serialize};

use crate::indexing::EmbeddingProvider;
use crate::pipeline::embedding_client::{cosine_sim_with_left_norm, l2_norm};
use crate::storage::StateDb;

mod lexical;
mod scope;
use scope::ScopedFilter;
pub mod query_cache;

const RRF_K: f32 = 60.0;
const SEMANTIC_CANDIDATE_CAP: usize = 4096;
const MAX_QUERY_TERMS: usize = 48;
const QUERY_EMBEDDING_BUDGET: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchCorpus {
    Knowledge,
    Session,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UnifiedSearchFilter {
    #[serde(default)]
    pub corpora: Vec<SearchCorpus>,
    pub project: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnifiedSearchHit {
    pub corpus: SearchCorpus,
    pub entity_id: String,
    pub chunk_id: Option<String>,
    pub title: String,
    pub snippet: String,
    pub score: f32,
    pub match_types: Vec<String>,
    pub siyuan_doc_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnifiedSearchOutcome {
    pub hits: Vec<UnifiedSearchHit>,
    pub degraded: bool,
    pub warnings: Vec<String>,
}

enum SearchDb<'a> {
    Owned(Arc<StateDb>),
    Borrowed(&'a StateDb),
}

impl std::ops::Deref for SearchDb<'_> {
    type Target = StateDb;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(db) => db.as_ref(),
            Self::Borrowed(db) => db,
        }
    }
}

pub struct UnifiedSearchService<'a> {
    db: SearchDb<'a>,
    embeddings: Arc<dyn EmbeddingProvider>,
    context: Option<crate::service::RequestContext>,
    auth_now: u64,
}

impl UnifiedSearchService<'static> {
    pub(crate) fn scoped_for(
        db: Arc<StateDb>,
        embeddings: Arc<dyn EmbeddingProvider>,
        context: crate::service::RequestContext,
        auth_now: u64,
    ) -> Self {
        Self {
            db: SearchDb::Owned(db),
            embeddings,
            context: Some(context),
            auth_now,
        }
    }

    pub fn new(db: Arc<StateDb>, embeddings: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            db: SearchDb::Owned(db),
            embeddings,
            context: None,
            auth_now: 0,
        }
    }
}

impl<'a> UnifiedSearchService<'a> {
    pub fn borrowed(db: &'a StateDb, embeddings: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            db: SearchDb::Borrowed(db),
            embeddings,
            context: None,
            auth_now: 0,
        }
    }

    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        filter: UnifiedSearchFilter,
    ) -> anyhow::Result<UnifiedSearchOutcome> {
        self.search_with_progress(query, limit, filter, |_| {})
            .await
    }

    /// Publish local recall before waiting for query embeddings. Existing
    /// callers still use search(), with exactly the same final fusion path.
    pub async fn search_with_progress<F>(
        &self,
        query: &str,
        limit: usize,
        filter: UnifiedSearchFilter,
        mut on_lexical: F,
    ) -> anyhow::Result<UnifiedSearchOutcome>
    where
        F: FnMut(&UnifiedSearchOutcome) + Send,
    {
        let filter = ScopedFilter {
            filter,
            context: self.context.clone(),
            auth_now: self.auth_now,
        };
        let started = Instant::now();
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Ok(UnifiedSearchOutcome {
                hits: Vec::new(),
                degraded: false,
                warnings: Vec::new(),
            });
        }
        anyhow::ensure!(query.chars().count() <= 4096, "Search query is too long");
        let corpora = normalized_corpora(&filter.corpora);
        let terms = analyze_query(query);
        tracing::info!(
            query_chars = query.chars().count(),
            query_terms = terms.len(),
            corpus_count = corpora.len(),
            semantic_enabled = self.embeddings.enabled(),
            limit,
            "[SEARCH_TIMING] search start"
        );
        let (lexical, mut warnings) = match &self.db {
            SearchDb::Owned(db) => {
                let db = db.clone();
                let query = query.to_string();
                let filter = filter.clone();
                let corpora = corpora.clone();
                tokio::task::spawn_blocking(move || {
                    recall_lexical(&db, &query, &terms, &filter, &corpora, limit)
                })
                .await?
            }
            SearchDb::Borrowed(db) => recall_lexical(db, query, &terms, &filter, &corpora, limit),
        };
        let lexical_ms = started.elapsed().as_millis() as u64;
        on_lexical(&UnifiedSearchOutcome {
            hits: fuse_rrf(lexical.clone(), Vec::new(), limit),
            degraded: !warnings.is_empty(),
            warnings: warnings.clone(),
        });
        // Give IPC and cancellation a scheduling point even on a cache hit.
        tokio::task::yield_now().await;

        let semantic = if !self.embeddings.enabled() {
            warnings
                .push("Semantic search is disabled; returning lexical results only".to_string());
            Vec::new()
        } else {
            match self.semantic_recall(query, &filter, &corpora).await {
                Ok(rows) => rows,
                Err(error) => {
                    warnings.push(format!(
                        "Semantic search unavailable; returning lexical results only: {error}"
                    ));
                    Vec::new()
                }
            }
        };
        let fusion_started = Instant::now();
        let hits = fuse_rrf(lexical, semantic, limit);
        tracing::info!(
            lexical_ms,
            fusion_ms = fusion_started.elapsed().as_millis() as u64,
            total_ms = started.elapsed().as_millis() as u64,
            hit_count = hits.len(),
            degraded = !warnings.is_empty(),
            "[SEARCH_TIMING] search complete"
        );
        Ok(UnifiedSearchOutcome {
            hits,
            degraded: !warnings.is_empty(),
            warnings,
        })
    }

    async fn semantic_recall(
        &self,
        query: &str,
        filter: &ScopedFilter,
        corpora: &HashSet<SearchCorpus>,
    ) -> anyhow::Result<Vec<RankedCandidate>> {
        let started = Instant::now();
        // This deadline is query-only: indexing/rebuild jobs keep their own
        // timeout and retry policy. Dropping this future cancels local HTTP I/O.
        let result = tokio::time::timeout(
            QUERY_EMBEDDING_BUDGET,
            self.embeddings.embed(vec![query.to_string()]),
        )
        .await;
        tracing::info!(
            embedding_ms = started.elapsed().as_millis() as u64,
            timed_out = result.is_err(),
            "[SEARCH_TIMING] query embedding"
        );
        let mut vectors =
            result.map_err(|_| anyhow::anyhow!("query embedding timed out after 8 seconds"))??;
        let query_vector = vectors
            .pop()
            .ok_or_else(|| anyhow::anyhow!("embedding provider returned no query vector"))?;
        anyhow::ensure!(
            !query_vector.is_empty(),
            "embedding provider returned an empty query vector"
        );
        anyhow::ensure!(
            query_vector.iter().all(|v| v.is_finite()),
            "invalid query vector"
        );

        let started = Instant::now();
        let result = match &self.db {
            SearchDb::Owned(db) => {
                let service = UnifiedSearchService::new(db.clone(), self.embeddings.clone());
                let filter = filter.clone();
                let corpora = corpora.clone();
                tokio::task::spawn_blocking(move || {
                    service.semantic_candidates(&query_vector, &filter, &corpora)
                })
                .await?
            }
            SearchDb::Borrowed(_) => self.semantic_candidates(&query_vector, filter, corpora),
        };
        tracing::info!(
            vector_ms = started.elapsed().as_millis() as u64,
            "[SEARCH_TIMING] bounded vector recall"
        );
        result
    }

    fn semantic_candidates(
        &self,
        query_vector: &[f32],
        filter: &ScopedFilter,
        corpora: &HashSet<SearchCorpus>,
    ) -> anyhow::Result<Vec<RankedCandidate>> {
        let model = self.embeddings.model_name();
        let query_norm = l2_norm(query_vector);
        anyhow::ensure!(query_norm > 0.0, "query embedding has zero norm");
        let mut candidates = Vec::new();
        if corpora.contains(&SearchCorpus::Knowledge) {
            candidates.extend(self.semantic_knowledge(query_vector, query_norm, model, filter)?);
        }
        if corpora.contains(&SearchCorpus::Session) {
            candidates.extend(self.semantic_sessions(query_vector, query_norm, model, filter)?);
        }
        candidates.sort_by(|a, b| {
            b.raw_score
                .partial_cmp(&a.raw_score)
                .unwrap_or(Ordering::Equal)
        });
        Ok(deduplicate_ranked(candidates))
    }

    fn semantic_knowledge(
        &self,
        query_vector: &[f32],
        query_norm: f32,
        model: &str,
        filter: &ScopedFilter,
    ) -> anyhow::Result<Vec<RankedCandidate>> {
        let conn = self.db.conn();
        let scope = filter.predicate(SearchCorpus::Knowledge, 5)?;
        let sql = format!(
            "SELECT ki.id, ki.title, ki.summary, ki.project_name, ki.siyuan_doc_id,
                    ss.source, kc.id, kc.text, er.vector
             FROM embedding_record er
             JOIN knowledge_chunk kc ON kc.id = er.chunk_id
             JOIN knowledge_item ki ON ki.id = kc.knowledge_id
             LEFT JOIN source_session ss ON ss.id = ki.source_session_id
             WHERE er.model = ?1 AND ki.status = 'active' AND er.vector IS NOT NULL
               AND (?3 = '' OR ki.project_name = ?3)
               AND (?4 = '' OR ss.source = ?4)
             AND {} LIMIT ?2",
            scope.clause,
        );
        let mut values = vec![
            Value::Text(model.to_owned()),
            Value::Integer(SEMANTIC_CANDIDATE_CAP as i64),
            Value::Text(filter.project.as_deref().unwrap_or("").trim().to_owned()),
            Value::Text(filter.source.as_deref().unwrap_or("").trim().to_owned()),
        ];
        values.extend(scope.values);
        let mut stmt = conn.prepare(&sql)?;
        let db_started = Instant::now();
        let rows = stmt.query_map(params_from_iter(values.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Vec<u8>>(8)?,
            ))
        })?;
        let rows = rows.collect::<Result<Vec<_>, _>>()?;
        let vector_bytes: usize = rows.iter().map(|row| row.6.len()).sum();
        let vector_db_ms = db_started.elapsed().as_millis() as u64;
        let vector_rows = rows.len();
        let score_started = Instant::now();
        let mut out = Vec::new();
        for (id, title, summary, siyuan_doc_id, chunk_id, text, bytes) in rows {
            let vector = decode_vector(&bytes)?;
            if vector.len() != query_vector.len() {
                continue;
            }
            out.push(RankedCandidate {
                raw_score: cosine_sim_with_left_norm(query_vector, query_norm, &vector),
                hit: UnifiedSearchHit {
                    corpus: SearchCorpus::Knowledge,
                    entity_id: id,
                    chunk_id: Some(chunk_id),
                    title,
                    snippet: if text.trim().is_empty() {
                        summary
                    } else {
                        truncate_chars(&text, 220)
                    },
                    score: 0.0,
                    match_types: vec!["semantic".to_string()],
                    siyuan_doc_id,
                },
            });
        }
        tracing::info!(
            corpus = "knowledge",
            vector_db_ms,
            vector_score_ms = score_started.elapsed().as_millis() as u64,
            vector_rows,
            vector_bytes,
            scored_rows = out.len(),
            "[SEARCH_TIMING] vector corpus"
        );
        Ok(out)
    }

    fn semantic_sessions(
        &self,
        query_vector: &[f32],
        query_norm: f32,
        model: &str,
        filter: &ScopedFilter,
    ) -> anyhow::Result<Vec<RankedCandidate>> {
        let conn = self.db.conn();
        let scope = filter.predicate(SearchCorpus::Session, 5)?;
        let sql = format!(
            "SELECT ss.id, COALESCE(ss.title, ''), ss.siyuan_doc_id,
                    sc.id, sc.text, er.vector
             FROM session_embedding_record er
             JOIN session_search_chunk sc ON sc.id = er.chunk_id
             JOIN source_session ss ON ss.id = sc.session_id
             JOIN session_index_state st ON st.session_id = ss.id
             WHERE er.model = ?1 AND st.status = 'ready' AND ss.is_missing = 0
               AND (?3 = '' OR ss.project_name = ?3)
               AND (?4 = '' OR ss.source = ?4)
             AND {} LIMIT ?2",
            scope.clause,
        );
        let mut values = vec![
            Value::Text(model.to_owned()),
            Value::Integer(SEMANTIC_CANDIDATE_CAP as i64),
            Value::Text(filter.project.as_deref().unwrap_or("").trim().to_owned()),
            Value::Text(filter.source.as_deref().unwrap_or("").trim().to_owned()),
        ];
        values.extend(scope.values);
        let mut stmt = conn.prepare(&sql)?;
        let db_started = Instant::now();
        let rows = stmt.query_map(params_from_iter(values.iter()), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        })?;
        let rows = rows.collect::<Result<Vec<_>, _>>()?;
        let vector_bytes: usize = rows.iter().map(|row| row.5.len()).sum();
        let vector_db_ms = db_started.elapsed().as_millis() as u64;
        let vector_rows = rows.len();
        let score_started = Instant::now();
        let mut out = Vec::new();
        for (session_id, title, siyuan_doc_id, chunk_id, text, bytes) in rows {
            let vector = decode_vector(&bytes)?;
            if vector.len() != query_vector.len() {
                continue;
            }
            out.push(RankedCandidate {
                raw_score: cosine_sim_with_left_norm(query_vector, query_norm, &vector),
                hit: UnifiedSearchHit {
                    corpus: SearchCorpus::Session,
                    entity_id: session_id.to_string(),
                    chunk_id: Some(chunk_id),
                    title: if title.is_empty() {
                        format!("AI Session {session_id}")
                    } else {
                        title
                    },
                    snippet: truncate_chars(&text, 220),
                    score: 0.0,
                    match_types: vec!["semantic".to_string()],
                    siyuan_doc_id,
                },
            });
        }
        tracing::info!(
            corpus = "session",
            vector_db_ms,
            vector_score_ms = score_started.elapsed().as_millis() as u64,
            vector_rows,
            vector_bytes,
            scored_rows = out.len(),
            "[SEARCH_TIMING] vector corpus"
        );
        Ok(out)
    }
}

fn recall_lexical(
    db: &StateDb,
    query: &str,
    terms: &[String],
    filter: &ScopedFilter,
    corpora: &HashSet<SearchCorpus>,
    requested_limit: usize,
) -> (Vec<RankedCandidate>, Vec<String>) {
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    for corpus in [SearchCorpus::Knowledge, SearchCorpus::Session] {
        if corpora.contains(&corpus) {
            let (rows, problems) =
                lexical::recall(db, query, terms, filter, corpus, requested_limit);
            candidates.extend(rows);
            warnings.extend(problems);
        }
    }
    candidates.sort_by(|a, b| {
        b.raw_score
            .partial_cmp(&a.raw_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.hit.title.cmp(&b.hit.title))
    });
    (deduplicate_ranked(candidates), warnings)
}

#[derive(Debug, Clone)]
struct RankedCandidate {
    raw_score: f32,
    hit: UnifiedSearchHit,
}

fn normalized_corpora(requested: &[SearchCorpus]) -> HashSet<SearchCorpus> {
    if requested.is_empty() {
        [SearchCorpus::Knowledge, SearchCorpus::Session]
            .into_iter()
            .collect()
    } else {
        requested.iter().copied().collect()
    }
}

pub fn analyze_query(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();
    let mut technical = String::new();
    let mut cjk_run = Vec::new();
    let flush_technical =
        |buffer: &mut String, terms: &mut Vec<String>, seen: &mut HashSet<String>| {
            let value = buffer.trim_matches(|ch: char| ch == '.' || ch == ',' || ch == ';');
            if value.len() >= 2 {
                let normalized = value.to_ascii_lowercase();
                if seen.insert(normalized.clone()) {
                    terms.push(normalized);
                }
            }
            buffer.clear();
        };
    let flush_cjk = |run: &mut Vec<char>, terms: &mut Vec<String>, seen: &mut HashSet<String>| {
        if run.is_empty() {
            return;
        }
        for ch in run.iter() {
            let token = ch.to_string();
            if seen.insert(token.clone()) {
                terms.push(token);
            }
        }
        for pair in run.windows(2) {
            let token: String = pair.iter().collect();
            if seen.insert(token.clone()) {
                terms.push(token);
            }
        }
        run.clear();
    };
    for ch in query.trim().chars() {
        if is_cjk(ch) {
            flush_technical(&mut technical, &mut terms, &mut seen);
            cjk_run.push(ch);
        } else {
            flush_cjk(&mut cjk_run, &mut terms, &mut seen);
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':' | '+' | '#') {
                technical.push(ch);
            } else {
                flush_technical(&mut technical, &mut terms, &mut seen);
            }
        }
        if terms.len() >= MAX_QUERY_TERMS {
            break;
        }
    }
    flush_technical(&mut technical, &mut terms, &mut seen);
    flush_cjk(&mut cjk_run, &mut terms, &mut seen);
    terms.truncate(MAX_QUERY_TERMS);
    terms
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x3040..=0x30FF | 0xAC00..=0xD7AF)
}

fn lexical_score(
    query: &str,
    terms: &[String],
    title: &str,
    secondary: &str,
    content: &str,
    metadata: &str,
) -> f32 {
    let query_lower = query.to_lowercase();
    let fields = [
        (title.to_lowercase(), 12.0, 4.0),
        (secondary.to_lowercase(), 8.0, 2.0),
        (content.to_lowercase(), 6.0, 1.0),
        (metadata.to_lowercase(), 5.0, 1.0),
    ];
    let mut score = 0.0;
    for (text, full_weight, term_weight) in fields {
        if text.contains(&query_lower) {
            score += full_weight;
        }
        for term in terms {
            if text.contains(term) {
                score += term_weight;
            }
        }
    }
    score
}

fn make_snippet(secondary: &str, content: &str, terms: &[String]) -> String {
    let preferred = if !secondary.trim().is_empty() {
        secondary
    } else {
        content
    };
    if let Some(position) = first_term_position(preferred, terms) {
        let chars: Vec<char> = preferred.chars().collect();
        let start = position.saturating_sub(50).min(chars.len());
        let end = (position + 170).min(chars.len());
        return chars[start..end].iter().collect();
    }
    truncate_chars(preferred, 220)
}

fn first_term_position(text: &str, terms: &[String]) -> Option<usize> {
    let lower = text.to_lowercase();
    let mut best = None;
    for term in terms {
        if let Some(byte_pos) = lower.find(term) {
            let char_pos = lower[..byte_pos].chars().count();
            best = Some(best.map_or(char_pos, |current: usize| current.min(char_pos)));
        }
    }
    best
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn deduplicate_ranked(candidates: Vec<RankedCandidate>) -> Vec<RankedCandidate> {
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert((candidate.hit.corpus, candidate.hit.entity_id.clone())))
        .collect()
}

fn fuse_rrf(
    lexical: Vec<RankedCandidate>,
    semantic: Vec<RankedCandidate>,
    limit: usize,
) -> Vec<UnifiedSearchHit> {
    let mut fused: HashMap<(SearchCorpus, String), UnifiedSearchHit> = HashMap::new();
    add_ranked_list(&mut fused, lexical, "lexical");
    add_ranked_list(&mut fused, semantic, "semantic");
    let mut hits: Vec<_> = fused.into_values().collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.title.cmp(&b.title))
    });
    hits.truncate(limit);
    hits
}

fn add_ranked_list(
    fused: &mut HashMap<(SearchCorpus, String), UnifiedSearchHit>,
    ranked: Vec<RankedCandidate>,
    match_type: &str,
) {
    for (index, candidate) in ranked.into_iter().enumerate() {
        let contribution = 1.0 / (RRF_K + (index + 1) as f32);
        let key = (candidate.hit.corpus, candidate.hit.entity_id.clone());
        match fused.entry(key) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                let mut hit = candidate.hit;
                hit.score = contribution;
                if !hit.match_types.iter().any(|value| value == match_type) {
                    hit.match_types.push(match_type.to_string());
                }
                entry.insert(hit);
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                let existing = entry.get_mut();
                existing.score += contribution;
                if !existing.match_types.iter().any(|value| value == match_type) {
                    existing.match_types.push(match_type.to_string());
                }
                if existing.chunk_id.is_none() && candidate.hit.chunk_id.is_some() {
                    existing.chunk_id = candidate.hit.chunk_id;
                    existing.snippet = candidate.hit.snippet;
                }
            }
        }
    }
}

fn decode_vector(bytes: &[u8]) -> anyhow::Result<Vec<f32>> {
    if !bytes.len().is_multiple_of(4) {
        anyhow::bail!("invalid f32 vector byte length: {}", bytes.len());
    }
    let (chunks, _) = bytes.as_chunks::<4>();
    Ok(chunks
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(corpus: SearchCorpus, id: &str, title: &str) -> RankedCandidate {
        RankedCandidate {
            raw_score: 1.0,
            hit: UnifiedSearchHit {
                corpus,
                entity_id: id.to_string(),
                chunk_id: None,
                title: title.to_string(),
                snippet: String::new(),
                score: 0.0,
                match_types: Vec::new(),
                siyuan_doc_id: None,
            },
        }
    }

    #[test]
    fn analyzer_preserves_technical_tokens_and_builds_cjk_terms() {
        let terms = analyze_query("如何解决kubernetes节点磁盘空间不足 42804 timestamptz /var/lib/kubelet/pods qwen3.8:27b");
        for term in [
            "kubernetes",
            "42804",
            "timestamptz",
            "/var/lib/kubelet/pods",
            "qwen3.8:27b",
            "节点",
            "磁盘",
        ] {
            assert!(terms.contains(&term.to_string()));
        }
    }

    #[test]
    fn rrf_rewards_entities_recalled_by_both_channels() {
        let lexical = vec![
            candidate(SearchCorpus::Knowledge, "shared", "shared"),
            candidate(SearchCorpus::Knowledge, "lexical", "lexical"),
        ];
        let semantic = vec![
            candidate(SearchCorpus::Knowledge, "shared", "shared"),
            candidate(SearchCorpus::Session, "semantic", "semantic"),
        ];
        let hits = fuse_rrf(lexical, semantic, 10);
        assert_eq!(hits[0].entity_id, "shared");
        assert!(hits[0].match_types.contains(&"lexical".to_string()));
        assert!(hits[0].match_types.contains(&"semantic".to_string()));
    }

    #[test]
    fn vector_decoder_rejects_corrupt_blob() {
        assert!(decode_vector(&[1, 2, 3]).is_err());
        let bytes: Vec<u8> = [1.0_f32, 2.0_f32]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        assert_eq!(decode_vector(&bytes).unwrap(), vec![1.0, 2.0]);
    }
}
