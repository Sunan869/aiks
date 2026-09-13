/// Vector + FTS5 Hybrid Search
///
/// In-process cosine similarity search over BLOB-stored embeddings.
/// Falls back to FTS5 when embeddings are not available.
use tracing::info;

use crate::pipeline::embedding_client::{cosine_sim, EmbeddingClient, EmbeddingConfig};
use crate::pipeline::knowledge_repo::{EmbeddingRow, KnowledgeRepo};
use crate::storage::StateDb;

#[derive(Debug)]
pub struct SearchHit {
    pub knowledge_id: String,
    pub chunk_id: String,
    pub chunk_text: String,
    pub score: f32,
    pub match_type: String,
}

/// Hybrid search: FTS5 keyword + vector similarity (when available)
pub async fn hybrid_search(
    db: &StateDb,
    query: &str,
    limit: usize,
    embedding_config: Option<&EmbeddingConfig>,
) -> anyhow::Result<Vec<SearchHit>> {
    let fts_results = fts_search(db, query, limit * 2)?;
    let vector_results = if let Some(cfg) = embedding_config {
        if cfg.enabled && !cfg.base_url.is_empty() {
            vector_search(db, cfg, query, limit * 2).await.unwrap_or_default()
        } else { vec![] }
    } else { vec![] };

    // Merge: deduplicate by knowledge_id, taking the higher score
    let mut merged: std::collections::HashMap<String, SearchHit> = std::collections::HashMap::new();

    for hit in fts_results {
        merged.entry(hit.knowledge_id.clone()).or_insert(hit);
    }
    for hit in vector_results {
        merged.entry(hit.knowledge_id.clone())
            .and_modify(|existing| {
                let combined = 0.35 * existing.score + 0.65 * hit.score;
                existing.score = combined;
                existing.match_type = "hybrid".to_string();
            })
            .or_insert(hit);
    }

    let mut results: Vec<SearchHit> = merged.into_values().collect();
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);

    info!(query, results = results.len(), "[SEARCH] Complete");
    Ok(results)
}

/// FTS5 full-text search
fn fts_search(db: &StateDb, query: &str, limit: usize) -> anyhow::Result<Vec<SearchHit>> {
    let conn = db.conn();

    let fts_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='knowledge_fts'",
        [], |r| r.get::<_, i64>(0),
    ).unwrap_or(0) > 0;

    if !fts_exists {
        return Ok(vec![]);
    }

    // Try FTS5 MATCH first, fall back to LIKE
    let try_fts = conn.prepare(
        "SELECT knowledge_id, title, summary FROM knowledge_fts WHERE knowledge_fts MATCH ?1 ORDER BY rank LIMIT ?2"
    );

    let hits: Vec<SearchHit> = match try_fts {
        Ok(mut stmt) => {
            let mapped = stmt
                .query_map(rusqlite::params![query, limit as i64], |row| {
                    Ok(SearchHit {
                        knowledge_id: row.get(0)?,
                        chunk_id: String::new(),
                        chunk_text: row.get::<_, String>(2)?,
                        score: 0.5,
                        match_type: "fts".to_string(),
                    })
                })?
                .filter_map(|r| r.ok());
            let rows: Vec<SearchHit> = mapped.collect();
            rows
        }
        Err(_) => {
            // Fallback: LIKE search on knowledge_item
            let pattern = format!("%{}%", query);
            let mut stmt2 = conn.prepare(
                "SELECT id, summary FROM knowledge_item WHERE title LIKE ?1 OR summary LIKE ?1 ORDER BY updated_at DESC LIMIT ?2"
            )?;
            let mapped2 = stmt2.query_map(rusqlite::params![pattern, limit as i64], |row| {
                Ok(SearchHit {
                    knowledge_id: row.get(0)?,
                    chunk_id: String::new(),
                    chunk_text: row.get(1)?,
                    score: 0.4,
                    match_type: "like".to_string(),
                })
            })?;
            let rows: Vec<SearchHit> = mapped2.filter_map(|r| r.ok()).collect();
            rows
        }
    };

    Ok(hits)
}

/// Vector similarity search (in-process cosine sim)
async fn vector_search(
    db: &StateDb,
    cfg: &EmbeddingConfig,
    query: &str,
    limit: usize,
) -> anyhow::Result<Vec<SearchHit>> {
    // Embed the query
    let client = EmbeddingClient::new(cfg.clone())?;
    let embeddings = client.embed_batch(vec![query.to_string()]).await?;
    let query_vec = embeddings.into_iter().next().unwrap_or_default();
    if query_vec.is_empty() { return Ok(vec![]); }

    // Load all stored embeddings
    let knowledge_repo = KnowledgeRepo::new(db);
    let stored = knowledge_repo.load_all_embeddings(&cfg.model)?;

    // Compute cosine similarity
    let mut hits: Vec<SearchHit> = stored
        .into_iter()
        .map(|row| {
            let score = cosine_sim(&query_vec, &row.vector);
            SearchHit {
                knowledge_id: row.knowledge_id,
                chunk_id: row.chunk_id,
                chunk_text: row.chunk_text,
                score,
                match_type: "vector".to_string(),
            }
        })
        .collect();

    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit);
    Ok(hits)
}
