use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::indexing::EmbeddingProvider;
use crate::pipeline::embedding_client::cosine_sim;
use crate::storage::StateDb;

const RRF_K: f32 = 60.0;
const SEMANTIC_CANDIDATE_CAP: usize = 4096;
const MAX_QUERY_TERMS: usize = 48;

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
}

impl UnifiedSearchService<'static> {
    pub fn new(db: Arc<StateDb>, embeddings: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            db: SearchDb::Owned(db),
            embeddings,
        }
    }
}

impl<'a> UnifiedSearchService<'a> {
    pub fn borrowed(db: &'a StateDb, embeddings: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            db: SearchDb::Borrowed(db),
            embeddings,
        }
    }

    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        filter: UnifiedSearchFilter,
    ) -> anyhow::Result<UnifiedSearchOutcome> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Ok(UnifiedSearchOutcome {
                hits: Vec::new(),
                degraded: false,
                warnings: Vec::new(),
            });
        }

        let corpora = normalized_corpora(&filter.corpora);
        let terms = analyze_query(query);
        let mut warnings = Vec::new();

        let lexical = match self.lexical_recall(query, &terms, &filter, &corpora) {
            Ok(rows) => rows,
            Err(error) => {
                warnings.push(format!("Lexical search degraded: {error}"));
                Vec::new()
            }
        };

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

        let hits = fuse_rrf(lexical, semantic, limit);
        Ok(UnifiedSearchOutcome {
            hits,
            degraded: !warnings.is_empty(),
            warnings,
        })
    }

    fn lexical_recall(
        &self,
        query: &str,
        terms: &[String],
        filter: &UnifiedSearchFilter,
        corpora: &HashSet<SearchCorpus>,
    ) -> anyhow::Result<Vec<RankedCandidate>> {
        let mut candidates = Vec::new();
        if corpora.contains(&SearchCorpus::Knowledge) {
            candidates.extend(self.lexical_knowledge(query, terms, filter)?);
        }
        if corpora.contains(&SearchCorpus::Session) {
            candidates.extend(self.lexical_sessions(query, terms, filter)?);
        }

        candidates.sort_by(|a, b| {
            b.raw_score
                .partial_cmp(&a.raw_score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.hit.title.cmp(&b.hit.title))
        });
        Ok(deduplicate_ranked(candidates))
    }

    fn lexical_knowledge(
        &self,
        query: &str,
        terms: &[String],
        filter: &UnifiedSearchFilter,
    ) -> anyhow::Result<Vec<RankedCandidate>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT ki.id, ki.title, ki.summary, ki.content, ki.tags,
                    ki.project_name, ki.siyuan_doc_id, ss.source
             FROM knowledge_item ki
             LEFT JOIN source_session ss ON ss.id = ki.source_session_id
             WHERE ki.status = 'active'",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, title, summary, content, tags, project, siyuan_doc_id, source) = row?;
            if !matches_filter(project.as_deref(), source.as_deref(), filter) {
                continue;
            }
            let score = lexical_score(query, terms, &title, &summary, &content, &tags);
            if score <= 0.0 {
                continue;
            }
            out.push(RankedCandidate {
                raw_score: score,
                hit: UnifiedSearchHit {
                    corpus: SearchCorpus::Knowledge,
                    entity_id: id,
                    chunk_id: None,
                    title,
                    snippet: make_snippet(&summary, &content, terms),
                    score: 0.0,
                    match_types: vec!["lexical".to_string()],
                    siyuan_doc_id,
                },
            });
        }
        Ok(out)
    }

    fn lexical_sessions(
        &self,
        query: &str,
        terms: &[String],
        filter: &UnifiedSearchFilter,
    ) -> anyhow::Result<Vec<RankedCandidate>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT ss.id, COALESCE(ss.title, ''), COALESCE(ss.project_name, ''),
                    ss.source, ss.siyuan_doc_id, COALESCE(sf.content, '')
             FROM source_session ss
             LEFT JOIN session_search_fts sf ON sf.session_id = ss.id
             WHERE ss.is_missing = 0",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (session_id, title, project, source, siyuan_doc_id, content) = row?;
            if content.is_empty() {
                continue;
            }
            if !matches_filter(Some(project.as_str()), Some(source.as_str()), filter) {
                continue;
            }
            let score = lexical_score(query, terms, &title, &project, &content, &source);
            if score <= 0.0 {
                continue;
            }
            out.push(RankedCandidate {
                raw_score: score,
                hit: UnifiedSearchHit {
                    corpus: SearchCorpus::Session,
                    entity_id: session_id.to_string(),
                    chunk_id: None,
                    title: if title.is_empty() {
                        format!("AI Session {session_id}")
                    } else {
                        title
                    },
                    snippet: make_snippet("", &content, terms),
                    score: 0.0,
                    match_types: vec!["lexical".to_string()],
                    siyuan_doc_id,
                },
            });
        }
        Ok(out)
    }

    async fn semantic_recall(
        &self,
        query: &str,
        filter: &UnifiedSearchFilter,
        corpora: &HashSet<SearchCorpus>,
    ) -> anyhow::Result<Vec<RankedCandidate>> {
        let mut query_vectors = self.embeddings.embed(vec![query.to_string()]).await?;
        let query_vector = query_vectors
            .pop()
            .ok_or_else(|| anyhow::anyhow!("embedding provider returned no query vector"))?;
        if query_vector.is_empty() {
            anyhow::bail!("embedding provider returned an empty query vector");
        }

        let model = self.embeddings.model_name();
        let mut candidates = Vec::new();
        if corpora.contains(&SearchCorpus::Knowledge) {
            candidates.extend(self.semantic_knowledge(&query_vector, model, filter)?);
        }
        if corpora.contains(&SearchCorpus::Session) {
            candidates.extend(self.semantic_sessions(&query_vector, model, filter)?);
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
        model: &str,
        filter: &UnifiedSearchFilter,
    ) -> anyhow::Result<Vec<RankedCandidate>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT ki.id, ki.title, ki.summary, ki.project_name, ki.siyuan_doc_id,
                    ss.source, kc.id, kc.text, er.vector
             FROM embedding_record er
             JOIN knowledge_chunk kc ON kc.id = er.chunk_id
             JOIN knowledge_item ki ON ki.id = kc.knowledge_id
             LEFT JOIN source_session ss ON ss.id = ki.source_session_id
             WHERE er.model = ?1 AND ki.status = 'active' AND er.vector IS NOT NULL
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![model, SEMANTIC_CANDIDATE_CAP as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Vec<u8>>(8)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, title, summary, project, siyuan_doc_id, source, chunk_id, text, bytes) = row?;
            if !matches_filter(project.as_deref(), source.as_deref(), filter) {
                continue;
            }
            let vector = decode_vector(&bytes)?;
            if vector.len() != query_vector.len() {
                continue;
            }
            out.push(RankedCandidate {
                raw_score: cosine_sim(query_vector, &vector),
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
        Ok(out)
    }

    fn semantic_sessions(
        &self,
        query_vector: &[f32],
        model: &str,
        filter: &UnifiedSearchFilter,
    ) -> anyhow::Result<Vec<RankedCandidate>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT ss.id, COALESCE(ss.title, ''), COALESCE(ss.project_name, ''),
                    ss.source, ss.siyuan_doc_id, sc.id, sc.text, er.vector
             FROM session_embedding_record er
             JOIN session_search_chunk sc ON sc.id = er.chunk_id
             JOIN source_session ss ON ss.id = sc.session_id
             JOIN session_index_state st ON st.session_id = ss.id
             WHERE er.model = ?1 AND st.status = 'ready' AND ss.is_missing = 0
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![model, SEMANTIC_CANDIDATE_CAP as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Vec<u8>>(7)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (session_id, title, project, source, siyuan_doc_id, chunk_id, text, bytes) = row?;
            if !matches_filter(Some(project.as_str()), Some(source.as_str()), filter) {
                continue;
            }
            let vector = decode_vector(&bytes)?;
            if vector.len() != query_vector.len() {
                continue;
            }
            out.push(RankedCandidate {
                raw_score: cosine_sim(query_vector, &vector),
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
        Ok(out)
    }
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

fn matches_filter(
    project: Option<&str>,
    source: Option<&str>,
    filter: &UnifiedSearchFilter,
) -> bool {
    if let Some(expected) = filter
        .project
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if project.unwrap_or_default() != expected {
            return false;
        }
    }
    if let Some(expected) = filter
        .source
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if source.unwrap_or_default() != expected {
            return false;
        }
    }
    true
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
    matches!(
        ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x3040..=0x30FF | 0xAC00..=0xD7AF
    )
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
    let title_lower = title.to_lowercase();
    let secondary_lower = secondary.to_lowercase();
    let content_lower = content.to_lowercase();
    let metadata_lower = metadata.to_lowercase();

    let mut score = 0.0;
    if title_lower.contains(&query_lower) {
        score += 12.0;
    }
    if secondary_lower.contains(&query_lower) {
        score += 8.0;
    }
    if content_lower.contains(&query_lower) {
        score += 6.0;
    }
    if metadata_lower.contains(&query_lower) {
        score += 5.0;
    }

    for term in terms {
        if title_lower.contains(term) {
            score += 4.0;
        }
        if secondary_lower.contains(term) {
            score += 2.0;
        }
        if content_lower.contains(term) {
            score += 1.0;
        }
        if metadata_lower.contains(term) {
            score += 1.0;
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
        let start = position.saturating_sub(50);
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
    let mut out = Vec::new();
    for candidate in candidates {
        let key = (candidate.hit.corpus, candidate.hit.entity_id.clone());
        if seen.insert(key) {
            out.push(candidate);
        }
    }
    out
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
        assert!(terms.contains(&"kubernetes".to_string()));
        assert!(terms.contains(&"42804".to_string()));
        assert!(terms.contains(&"timestamptz".to_string()));
        assert!(terms.contains(&"/var/lib/kubelet/pods".to_string()));
        assert!(terms.contains(&"qwen3.8:27b".to_string()));
        assert!(terms.contains(&"节点".to_string()));
        assert!(terms.contains(&"磁盘".to_string()));
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
