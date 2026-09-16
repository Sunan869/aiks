/// Vector + FTS5 Hybrid Search
///
/// Text search is always the baseline. Optional vector search can improve
/// ranking, but failures are reported as degradations instead of being silently
/// converted into an empty vector result.
use tracing::info;

use crate::pipeline::embedding_client::{cosine_sim, EmbeddingClient, EmbeddingConfig};
use crate::pipeline::knowledge_repo::KnowledgeRepo;
use crate::storage::StateDb;

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

pub async fn hybrid_search(
    db: &StateDb,
    query: &str,
    limit: usize,
    embedding_config: Option<&EmbeddingConfig>,
) -> anyhow::Result<Vec<SearchHit>> {
    Ok(search_with_status(db, query, limit, embedding_config)
        .await?
        .hits)
}

pub async fn search_with_status(
    db: &StateDb,
    query: &str,
    limit: usize,
    embedding_config: Option<&EmbeddingConfig>,
) -> anyhow::Result<SearchOutcome> {
    let (fts_results, fts_degradation) = fts_search(db, query, limit.saturating_mul(2))?;
    let mut degradations = Vec::new();
    if let Some(degradation) = fts_degradation {
        degradations.push(degradation);
    }

    let mut preferred_seen = std::collections::HashSet::new();
    let preferred_knowledge_ids: Vec<String> = fts_results
        .iter()
        .filter_map(|hit| {
            if preferred_seen.insert(hit.knowledge_id.clone()) {
                Some(hit.knowledge_id.clone())
            } else {
                None
            }
        })
        .take(VECTOR_CANDIDATE_CAP)
        .collect();

    let vector_results = if let Some(cfg) = embedding_config {
        if cfg.enabled && !cfg.base_url.trim().is_empty() {
            match vector_search(
                db,
                cfg,
                query,
                limit.saturating_mul(2),
                &preferred_knowledge_ids,
            )
            .await
            {
                Ok(results) => results,
                Err(e) => {
                    tracing::warn!(error = %e, "[SEARCH] Vector search unavailable; returning text results");
                    degradations.push(SearchDegradation {
                        kind: SearchDegradationKind::VectorUnavailable,
                        message: format!("Vector search unavailable: {}", e),
                    });
                    vec![]
                }
            }
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let mut merged: std::collections::HashMap<String, SearchHit> = std::collections::HashMap::new();
    for hit in fts_results {
        merged.entry(hit.knowledge_id.clone()).or_insert(hit);
    }
    for hit in vector_results {
        merged
            .entry(hit.knowledge_id.clone())
            .and_modify(|existing| {
                existing.score = 0.35 * existing.score + 0.65 * hit.score;
                existing.match_type = "hybrid".to_string();
            })
            .or_insert(hit);
    }

    let mut hits: Vec<SearchHit> = merged.into_values().collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);

    info!(
        query,
        results = hits.len(),
        degraded = !degradations.is_empty(),
        "[SEARCH] Complete"
    );
    Ok(SearchOutcome { hits, degradations })
}

fn literal_fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

fn literal_like_pattern(query: &str) -> String {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{}%", escaped)
}

fn like_search(db: &StateDb, query: &str, limit: usize) -> anyhow::Result<Vec<SearchHit>> {
    let conn = db.conn();
    let pattern = literal_like_pattern(query);
    let mut stmt = conn.prepare(
        "SELECT id, summary FROM knowledge_item
         WHERE status = 'active'
           AND (title LIKE ?1 ESCAPE '\\'
             OR summary LIKE ?1 ESCAPE '\\'
             OR content LIKE ?1 ESCAPE '\\')
         ORDER BY updated_at DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![pattern, limit as i64], |row| {
            Ok(SearchHit {
                knowledge_id: row.get(0)?,
                chunk_id: String::new(),
                chunk_text: row.get(1)?,
                score: 0.4,
                match_type: "like".to_string(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn fts_search(
    db: &StateDb,
    query: &str,
    limit: usize,
) -> anyhow::Result<(Vec<SearchHit>, Option<SearchDegradation>)> {
    let trimmed = query.trim();
    if trimmed.is_empty() || limit == 0 {
        return Ok((vec![], None));
    }

    let conn = db.conn();
    let fts_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='knowledge_fts'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;

    if !fts_exists {
        drop(conn);
        let hits = like_search(db, trimmed, limit)?;
        return Ok((
            hits,
            Some(SearchDegradation {
                kind: SearchDegradationKind::FtsFallback,
                message: "FTS index unavailable; used LIKE fallback".to_string(),
            }),
        ));
    }

    let Some(fts_query) = literal_fts_query(trimmed) else {
        return Ok((vec![], None));
    };

    let fts_result: anyhow::Result<Vec<SearchHit>> = (|| {
        let mut stmt = conn.prepare(
            "SELECT knowledge_fts.knowledge_id, knowledge_fts.title, knowledge_fts.summary
             FROM knowledge_fts
             JOIN knowledge_item ki ON ki.id = knowledge_fts.knowledge_id
             WHERE knowledge_fts MATCH ?1 AND ki.status = 'active'
             ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![fts_query, limit as i64], |row| {
                Ok(SearchHit {
                    knowledge_id: row.get(0)?,
                    chunk_id: String::new(),
                    chunk_text: row.get::<_, String>(2)?,
                    score: 0.5,
                    match_type: "fts".to_string(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })();

    match fts_result {
        Ok(rows) => Ok((rows, None)),
        Err(e) => {
            tracing::warn!(error = %e, query = trimmed, "[SEARCH] FTS failed; degrading to LIKE");
            drop(conn);
            let hits = like_search(db, trimmed, limit)?;
            Ok((
                hits,
                Some(SearchDegradation {
                    kind: SearchDegradationKind::FtsFallback,
                    message: format!("FTS search failed; used LIKE fallback: {}", e),
                }),
            ))
        }
    }
}

async fn vector_search(
    db: &StateDb,
    cfg: &EmbeddingConfig,
    query: &str,
    limit: usize,
    preferred_knowledge_ids: &[String],
) -> anyhow::Result<Vec<SearchHit>> {
    let client = EmbeddingClient::new(cfg.clone())?;
    let embeddings = client.embed_batch(vec![query.to_string()]).await?;
    let query_vec = embeddings.into_iter().next().unwrap_or_default();
    if query_vec.is_empty() {
        return Ok(vec![]);
    }

    let knowledge_repo = KnowledgeRepo::new(db);
    let mut stored = knowledge_repo.load_embedding_candidates(
        &cfg.model,
        preferred_knowledge_ids,
        VECTOR_CANDIDATE_CAP,
    )?;

    if !preferred_knowledge_ids.is_empty() && stored.len() < VECTOR_CANDIDATE_CAP {
        let recent =
            knowledge_repo.load_embedding_candidates(&cfg.model, &[], VECTOR_CANDIDATE_CAP)?;
        let mut chunk_ids: std::collections::HashSet<String> =
            stored.iter().map(|row| row.chunk_id.clone()).collect();
        for row in recent {
            if chunk_ids.insert(row.chunk_id.clone()) {
                stored.push(row);
                if stored.len() >= VECTOR_CANDIDATE_CAP {
                    break;
                }
            }
        }
    }

    let mut hits: Vec<SearchHit> = stored
        .into_iter()
        .map(|row| SearchHit {
            score: cosine_sim(&query_vec, &row.vector),
            knowledge_id: row.knowledge_id,
            chunk_id: row.chunk_id,
            chunk_text: row.chunk_text,
            match_type: "vector".to_string(),
        })
        .collect();

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);
    Ok(hits)
}
