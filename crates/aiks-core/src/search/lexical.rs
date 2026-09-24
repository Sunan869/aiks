//! Indexed lexical candidates. FTS is the driving table, never the inner side
//! of an unindexed session-id join. Substring fallback remains available for
//! CJK/technical queries and damaged or missing FTS indexes.

use std::time::Instant;

use rusqlite::{params_from_iter, types::Value, Connection};

use super::{lexical_score, RankedCandidate, ScopedFilter, SearchCorpus, UnifiedSearchHit};
use crate::storage::StateDb;

const CANDIDATE_CAP: usize = 1024;

pub(super) fn recall(
    db: &StateDb,
    query: &str,
    terms: &[String],
    filter: &ScopedFilter,
    corpus: SearchCorpus,
    requested_limit: usize,
) -> (Vec<RankedCandidate>, Vec<String>) {
    let total_started = Instant::now();
    let mut warnings = Vec::new();
    let expression = fts_expression(query, terms);
    let conn = db.conn();

    let fts_started = Instant::now();
    let mut candidates = match indexed(&conn, &expression, filter, corpus) {
        Ok(rows) => rows,
        Err(error) => {
            warnings.push(format!(
                "{corpus:?} FTS unavailable; using text fallback: {error}"
            ));
            Vec::new()
        }
    };
    let fts_candidates = candidates.len();
    tracing::info!(
        corpus = ?corpus,
        fts_ms = fts_started.elapsed().as_millis() as u64,
        fts_candidates,
        "[SEARCH_TIMING] lexical fts"
    );

    // CJK queries used to force a full substring scan even when FTS had
    // already produced far more candidates than the UI could display. Keep
    // substring as a recall safety net when FTS is insufficient, while still
    // forcing it for technical punctuation that unicode61 does not preserve.
    let needs_substring = should_run_substring(query, candidates.len(), requested_limit);
    tracing::info!(
        corpus = ?corpus,
        needs_substring,
        query_terms = terms.len(),
        "[SEARCH_TIMING] lexical fallback decision"
    );
    if needs_substring {
        let substring_started = Instant::now();
        match substring(&conn, query, terms, filter, corpus) {
            Ok(rows) => {
                let substring_candidates = rows.len();
                candidates.extend(rows);
                tracing::info!(
                    corpus = ?corpus,
                    substring_ms = substring_started.elapsed().as_millis() as u64,
                    substring_candidates,
                    "[SEARCH_TIMING] lexical substring"
                );
            }
            Err(error) => {
                tracing::info!(
                    corpus = ?corpus,
                    substring_ms = substring_started.elapsed().as_millis() as u64,
                    substring_candidates = 0usize,
                    failed = true,
                    "[SEARCH_TIMING] lexical substring"
                );
                warnings.push(format!("{corpus:?} text fallback unavailable: {error}"));
                // A missing session transcript index must not erase knowledge
                // results. Session metadata is still safely searchable.
                if corpus == SearchCorpus::Session {
                    let metadata_started = Instant::now();
                    match session_metadata(&conn, query, terms, filter) {
                        Ok(rows) => {
                            let metadata_candidates = rows.len();
                            candidates.extend(rows);
                            tracing::info!(
                                metadata_ms = metadata_started.elapsed().as_millis() as u64,
                                metadata_candidates,
                                "[SEARCH_TIMING] session metadata fallback"
                            );
                        }
                        Err(error) => {
                            tracing::info!(
                                metadata_ms = metadata_started.elapsed().as_millis() as u64,
                                metadata_candidates = 0usize,
                                failed = true,
                                "[SEARCH_TIMING] session metadata fallback"
                            );
                            warnings.push(format!("Session metadata unavailable: {error}"))
                        }
                    }
                }
            }
        }
    }
    tracing::info!(
        corpus = ?corpus,
        lexical_corpus_ms = total_started.elapsed().as_millis() as u64,
        candidate_count = candidates.len(),
        warning_count = warnings.len(),
        "[SEARCH_TIMING] lexical corpus complete"
    );
    (candidates, warnings)
}

fn should_run_substring(query: &str, fts_candidates: usize, requested_limit: usize) -> bool {
    let enough_for_request = fts_candidates >= requested_limit.clamp(1, CANDIDATE_CAP);
    let has_technical_syntax = query.chars().any(|ch| "_./:+#-".contains(ch));
    !enough_for_request || has_technical_syntax
}

fn fts_expression(query: &str, terms: &[String]) -> String {
    // Single CJK characters create extremely broad prefix matches in FTS5
    // (especially for full-session documents). When the analyzer has already
    // produced a multi-character CJK term, keep that more selective term for
    // FTS and leave exhaustive substring coverage to the fallback path.
    let has_multi_cjk = terms
        .iter()
        .any(|term| term.chars().count() >= 2 && term.chars().all(super::is_cjk));
    let filtered: Vec<&str> = terms
        .iter()
        .map(String::as_str)
        .filter(|term| {
            !(has_multi_cjk && term.chars().count() == 1 && term.chars().all(super::is_cjk))
        })
        .collect();
    let values: Vec<&str> = if filtered.is_empty() {
        vec![query]
    } else {
        filtered
    };
    values
        .into_iter()
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn filter_values(filter: &ScopedFilter) -> (&str, &str) {
    (
        filter.project.as_deref().unwrap_or("").trim(),
        filter.source.as_deref().unwrap_or("").trim(),
    )
}

fn indexed(
    conn: &Connection,
    expression: &str,
    filter: &ScopedFilter,
    corpus: SearchCorpus,
) -> anyhow::Result<Vec<RankedCandidate>> {
    let sql = match corpus {
        SearchCorpus::Knowledge => {
            "SELECT ki.id, ki.title,
                    snippet(knowledge_fts, -1, '', '', '…', 48),
                    ki.siyuan_doc_id, bm25(knowledge_fts, 0, 4, 2, 1, 1)
             FROM knowledge_fts
             CROSS JOIN knowledge_item ki ON ki.id = knowledge_fts.knowledge_id
             LEFT JOIN source_session ss ON ss.id = ki.source_session_id
             WHERE knowledge_fts MATCH ?1 AND ki.status = 'active'
               AND (?2 = '' OR ki.project_name = ?2)
               AND (?3 = '' OR ss.source = ?3)
             ORDER BY rank LIMIT ?4"
        }
        SearchCorpus::Session => {
            // session_search_fts currently stores one potentially very large
            // transcript per session. FTS5 snippet() has to inspect that large
            // text for every ranked candidate and dominated real-device search
            // latency. Return a cheap bounded preview here; CJK/technical
            // queries still run the substring fallback which produces a
            // match-centered snippet.
            "SELECT CAST(ss.id AS TEXT), COALESCE(ss.title, ''),
                    substr(session_search_fts.content, 1, 220),
                    ss.siyuan_doc_id, bm25(session_search_fts)
             FROM session_search_fts
             CROSS JOIN source_session ss ON ss.id = session_search_fts.session_id
             WHERE session_search_fts MATCH ?1 AND ss.is_missing = 0
               AND (?2 = '' OR ss.project_name = ?2)
               AND (?3 = '' OR ss.source = ?3)
             ORDER BY rank LIMIT ?4"
        }
    };
    let (project, source) = filter_values(filter);
    let scope = filter.predicate(corpus, 5)?;
    let sql = sql.replace(
        "ORDER BY rank",
        &format!("AND {} ORDER BY rank", scope.clause),
    );
    let mut values = vec![
        Value::Text(expression.to_owned()),
        Value::Text(project.to_owned()),
        Value::Text(source.to_owned()),
        Value::Integer(CANDIDATE_CAP as i64),
    ];
    values.extend(scope.values);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(values.iter()), |row| {
        Ok(candidate(
            corpus,
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            -row.get::<_, f64>(4)? as f32,
        ))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn substring(
    conn: &Connection,
    query: &str,
    terms: &[String],
    filter: &ScopedFilter,
    corpus: SearchCorpus,
) -> anyhow::Result<Vec<RankedCandidate>> {
    let (select, haystack) = match corpus {
        SearchCorpus::Knowledge => (
            "SELECT ki.id, ki.title, ki.summary, ki.tags,
                    substr(ki.content, MAX(1, instr(lower(ki.content), ?3) - 50), 240),
                    ki.siyuan_doc_id
             FROM knowledge_item ki
             LEFT JOIN source_session ss ON ss.id = ki.source_session_id
             WHERE ki.status = 'active'
               AND (?1 = '' OR ki.project_name = ?1)
               AND (?2 = '' OR ss.source = ?2)",
            "lower(ki.title || ' ' || ki.summary || ' ' || ki.content || ' ' || ki.tags)",
        ),
        SearchCorpus::Session => (
            "SELECT CAST(ss.id AS TEXT), COALESCE(ss.title, ''),
                    COALESCE(ss.project_name, ''), ss.source,
                    substr(sf.content, MAX(1, instr(lower(sf.content), ?3) - 50), 240),
                    ss.siyuan_doc_id
             FROM session_search_fts sf
             CROSS JOIN source_session ss ON ss.id = sf.session_id
             WHERE ss.is_missing = 0
               AND (?1 = '' OR ss.project_name = ?1)
               AND (?2 = '' OR ss.source = ?2)",
            "lower(COALESCE(ss.title, '') || ' ' || COALESCE(ss.project_name, '') || ' ' || ss.source || ' ' || sf.content)",
        ),
    };
    text_rows(conn, query, terms, filter, corpus, select, haystack)
}

fn session_metadata(
    conn: &Connection,
    query: &str,
    terms: &[String],
    filter: &ScopedFilter,
) -> anyhow::Result<Vec<RankedCandidate>> {
    text_rows(
        conn,
        query,
        terms,
        filter,
        SearchCorpus::Session,
        "SELECT CAST(ss.id AS TEXT), COALESCE(ss.title, ''),
                COALESCE(ss.project_name, ''), ss.source, '', ss.siyuan_doc_id
         FROM source_session ss WHERE ss.is_missing = 0
           AND (?1 = '' OR ss.project_name = ?1)
           AND (?2 = '' OR ss.source = ?2)",
        "lower(COALESCE(ss.title, '') || ' ' || COALESCE(ss.project_name, '') || ' ' || ss.source)",
    )
}

fn text_rows(
    conn: &Connection,
    query: &str,
    terms: &[String],
    filter: &ScopedFilter,
    corpus: SearchCorpus,
    select: &str,
    haystack: &str,
) -> anyhow::Result<Vec<RankedCandidate>> {
    let (project, source) = filter_values(filter);
    let mut values = vec![
        Value::Text(project.to_string()),
        Value::Text(source.to_string()),
        Value::Text(query.to_lowercase()),
    ];
    values.extend(terms.iter().cloned().map(Value::Text));
    let predicates = (3..=values.len())
        .map(|index| format!("instr({haystack}, ?{index}) > 0"))
        .collect::<Vec<_>>()
        .join(" OR ");
    // LIMIT is a compile-time bound; every user-controlled value is bound.
    let scope = filter.predicate(corpus, values.len() + 1)?;
    let sql = format!(
        "{select} AND ({predicates}) AND {} LIMIT {CANDIDATE_CAP}",
        scope.clause
    );
    values.extend(scope.values);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(values.iter()), |row| {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let secondary: String = row.get(2)?;
        let metadata: String = row.get(3)?;
        let content: String = row.get(4)?;
        let score = lexical_score(query, terms, &title, &secondary, &content, &metadata);
        Ok(candidate(
            corpus,
            id,
            title,
            super::make_snippet(&secondary, &content, terms),
            row.get(5)?,
            score.max(0.001),
        ))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn candidate(
    corpus: SearchCorpus,
    id: String,
    title: String,
    snippet: String,
    siyuan_doc_id: Option<String>,
    raw_score: f32,
) -> RankedCandidate {
    RankedCandidate {
        raw_score,
        hit: UnifiedSearchHit {
            corpus,
            title: if title.is_empty() {
                format!("AI Session {id}")
            } else {
                title
            },
            entity_id: id,
            chunk_id: None,
            snippet,
            score: 0.0,
            match_types: vec!["lexical".to_string()],
            siyuan_doc_id,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substring_is_skipped_when_fts_already_satisfies_plain_cjk_request() {
        assert!(!should_run_substring("磁盘", 30, 30));
        assert!(!should_run_substring("磁盘", 117, 30));
        assert!(should_run_substring("磁盘", 29, 30));
    }

    #[test]
    fn technical_syntax_keeps_substring_safety_net() {
        assert!(should_run_substring("/var/lib/kubelet/pods", 200, 30));
        assert!(should_run_substring("qwen3.8:27b", 200, 30));
    }

    #[test]
    fn cjk_fts_prefers_multi_character_terms_over_single_character_prefixes() {
        let terms = vec!["磁".to_string(), "盘".to_string(), "磁盘".to_string()];
        let expression = fts_expression("磁盘", &terms);
        assert_eq!(expression, "\"磁盘\"*");
    }

    #[test]
    fn single_cjk_character_search_remains_supported() {
        let terms = vec!["盘".to_string()];
        let expression = fts_expression("盘", &terms);
        assert_eq!(expression, "\"盘\"*");
    }

    #[test]
    fn technical_terms_are_preserved_beside_cjk_bigrams() {
        let terms = vec![
            "磁".to_string(),
            "盘".to_string(),
            "磁盘".to_string(),
            "kubernetes".to_string(),
        ];
        let expression = fts_expression("磁盘 kubernetes", &terms);
        assert!(expression.contains("\"磁盘\"*"));
        assert!(expression.contains("\"kubernetes\"*"));
        assert!(!expression.contains("\"磁\"*"));
        assert!(!expression.contains("\"盘\"*"));
    }
}
