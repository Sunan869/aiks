//! Opt-in, anonymized live-model quality/latency benchmark for S2.1.
//!
//! By default CI only tests the scoring/report code; it never calls a model.
//! AIKS_BENCH_BASE_URL=http://127.0.0.1:11434/v1 AIKS_BENCH_MODEL=qwen3 \
//!   cargo test -p aiks-core --test extraction_live_benchmark -- --ignored --nocapture
//!
//! Optional: AIKS_BENCH_API_KEY, AIKS_BENCH_DISABLE_THINKING=1,
//! AIKS_BENCH_REPEATS=3 (1..=5), AIKS_BENCH_REPORT_PATH=path-to-new.json.
//! The transcripts are synthetic. Reports never persist prompts, responses,
//! endpoint URLs, credentials, or provider files.
//! This tests model extraction output, not the end-to-end durable pipeline.

use aiks_core::ai::prompts_v3::{make_v3_extraction_prompt, SYSTEM_PROMPT_V3};
use aiks_core::ai::{AiClient, AiModelConfig, V3ExtractionResult};
use aiks_core::pipeline::ai_stage::parse_v3_result_typed;
use serde::Serialize;
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

struct Case {
    id: &'static str,
    transcript: &'static str,
    expected_terms: &'static [&'static str],
    worth_extracting: bool,
}

fn cases() -> [Case; 5] {
    [
        Case {
            id: "short_timeout_fix",
            transcript: "User: Our HTTP client hit a timeout after 5 seconds.\nAssistant: Set a finite retry limit of 2 and apply bounded backoff. Tests now pass.",
            expected_terms: &["timeout", "retry"],
            worth_extracting: true,
        },
        Case {
            id: "long_chunk_cache",
            transcript: "User: A large Codex session has multiple unrelated problems.\nAssistant: Extract knowledge independently from each chunk. Cache results using content and model fingerprint, then merge without discarding middle-chunk evidence. On rescan, reuse unchanged chunks.",
            expected_terms: &["chunk", "cache"],
            worth_extracting: true,
        },
        Case {
            id: "repeated_tool_noise",
            transcript: "Tool: progress 10%\nTool: progress 20%\nTool: progress 30%\nTool: progress 40%\nTool: progress 50%\nTool: progress 60%\nTool: progress 70%\nTool: progress 80%\nTool: progress 90%\nTool: progress 100%\nUser: Thanks.",
            expected_terms: &[],
            worth_extracting: false,
        },
        Case {
            id: "decision_reversal",
            transcript: "User: We originally selected an online database.\nAssistant: Because offline use is required, switch to SQLite. Record a migration rollback procedure before applying changes. Verification failed once, so rollback was used.",
            expected_terms: &["SQLite", "rollback"],
            worth_extracting: true,
        },
        Case {
            id: "mixed_zh_en",
            transcript: "用户：SQLite 导入中文文本出现乱码。\n助手：查明旧文件不是 UTF-8；统一使用 UTF-8 编码重新导入，确认中文字段完整。",
            expected_terms: &["SQLite", "UTF-8"],
            worth_extracting: true,
        },
    ]
}

fn percentile(values: &[u64], percentage: usize) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = sorted
        .len()
        .saturating_mul(percentage.min(100))
        .div_ceil(100);
    sorted[rank.max(1) - 1]
}

/// Only aggregate anonymous case identifiers and measured numerical facts.
/// Never store a model response, source transcript, error text, URL or API key.
#[derive(Debug, Serialize)]
struct CaseObservation {
    case_id: &'static str,
    repetition: usize,
    request_ok: bool,
    strict_json_valid: bool,
    production_schema_valid: bool,
    skip_match: bool,
    term_hits: usize,
    term_total: usize,
    exact_duplicate_titles: usize,
    elapsed_ms: u64,
    reported_tokens: Option<u64>,
}

fn observation(
    case: &Case,
    repetition: usize,
    content: Option<&str>,
    elapsed_ms: u64,
    reported_tokens: Option<u64>,
) -> CaseObservation {
    let mut observed = CaseObservation {
        case_id: case.id,
        repetition,
        request_ok: content.is_some(),
        strict_json_valid: false,
        production_schema_valid: false,
        skip_match: false,
        term_hits: 0,
        term_total: case.expected_terms.len(),
        exact_duplicate_titles: 0,
        elapsed_ms,
        reported_tokens,
    };
    let Some(content) = content else {
        return observed;
    };
    observed.strict_json_valid = serde_json::from_str::<V3ExtractionResult>(content).is_ok();

    if let Ok(result) = parse_v3_result_typed(content) {
        observed.production_schema_valid = true;
        observed.skip_match = result.worth_extracting == case.worth_extracting
            && (case.worth_extracting == !result.items.is_empty());

        let mut unique = HashSet::new();
        let evidence = result
            .items
            .iter()
            .map(|item| {
                let identity = (
                    item.category.trim().to_lowercase(),
                    item.title.trim().to_lowercase(),
                );
                if !unique.insert(identity) {
                    observed.exact_duplicate_titles += 1;
                }
                format!(
                    "{} {} {} {}",
                    item.title,
                    item.summary,
                    item.content,
                    item.tags.join(" ")
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        observed.term_hits = case
            .expected_terms
            .iter()
            .filter(|term| evidence.contains(&term.to_lowercase()))
            .count();
    }
    observed
}

#[derive(Debug, Serialize)]
struct LiveReport {
    report_version: u32,
    model: String,
    case_count: usize,
    repeats: usize,
    requests: usize,
    request_errors: usize,
    strict_json_valid: usize,
    production_schema_valid: usize,
    skip_matches: usize,
    term_hits: usize,
    term_total: usize,
    exact_duplicate_titles: usize,
    latency_p50_ms: u64,
    latency_p95_ms: u64,
    reported_token_calls: usize,
    total_reported_tokens: Option<u64>,
    observations: Vec<CaseObservation>,
}

fn summarize(
    model: &str,
    case_count: usize,
    repeats: usize,
    rows: Vec<CaseObservation>,
) -> LiveReport {
    let latencies: Vec<u64> = rows.iter().map(|row| row.elapsed_ms).collect();
    let reported_token_calls = rows
        .iter()
        .filter(|row| row.reported_tokens.is_some())
        .count();
    let reported_tokens = rows
        .iter()
        .filter_map(|row| row.reported_tokens)
        .fold(0u64, u64::saturating_add);
    LiveReport {
        report_version: 1,
        model: model.to_string(),
        case_count,
        repeats,
        requests: rows.len(),
        request_errors: rows.iter().filter(|row| !row.request_ok).count(),
        strict_json_valid: rows.iter().filter(|row| row.strict_json_valid).count(),
        production_schema_valid: rows
            .iter()
            .filter(|row| row.production_schema_valid)
            .count(),
        skip_matches: rows.iter().filter(|row| row.skip_match).count(),
        term_hits: rows.iter().map(|row| row.term_hits).sum(),
        term_total: rows.iter().map(|row| row.term_total).sum(),
        exact_duplicate_titles: rows.iter().map(|row| row.exact_duplicate_titles).sum(),
        latency_p50_ms: percentile(&latencies, 50),
        latency_p95_ms: percentile(&latencies, 95),
        reported_token_calls,
        total_reported_tokens: (reported_token_calls > 0).then_some(reported_tokens),
        observations: rows,
    }
}

/// Never overwrite an earlier real-model benchmark. Use an explicitly selected
/// output path; no report is written to an AIKS/Provider directory by default.
fn save_report(report: &LiveReport, path: &Path) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(report)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn configured_repeats(value: Option<&str>) -> anyhow::Result<usize> {
    let n = value.unwrap_or("1").parse::<usize>()?;
    anyhow::ensure!((1..=5).contains(&n), "AIKS_BENCH_REPEATS must be 1..=5");
    Ok(n)
}

#[test]
fn live_benchmark_cases_are_distinct_and_have_safe_evidence() {
    let mut ids = HashSet::new();
    for case in cases() {
        assert!(ids.insert(case.id));
        assert!(!case.transcript.is_empty());
        assert_eq!(case.expected_terms.is_empty(), !case.worth_extracting);
        assert!(!case.transcript.contains("sk-"));
        assert!(!case.transcript.contains("Bearer "));
    }
    assert_eq!(ids.len(), 5);
    assert_eq!(percentile(&[], 95), 0);
    assert_eq!(percentile(&[10, 20, 30, 40, 50], 50), 30);
    assert_eq!(percentile(&[10, 20, 30, 40, 50], 95), 50);
    assert_eq!(configured_repeats(None).unwrap(), 1);
    assert_eq!(configured_repeats(Some("5")).unwrap(), 5);
    assert!(configured_repeats(Some("0")).is_err());
    assert!(configured_repeats(Some("6")).is_err());
    assert!(configured_repeats(Some("garbage")).is_err());
}

#[test]
fn production_gate_and_metrics_are_computed_from_actual_output_not_fixture_labels() {
    let noise = cases()
        .into_iter()
        .find(|case| !case.worth_extracting)
        .unwrap();
    let good_skip = serde_json::to_string(&V3ExtractionResult::skip("工具进度无知识")).unwrap();
    let valid = observation(&noise, 1, Some(&good_skip), 12, None);
    assert!(valid.production_schema_valid);
    assert!(valid.skip_match);
    assert!(valid.strict_json_valid);
    assert_eq!(valid.term_total, 0);

    let wrong = serde_json::json!({
        "session_summary": "wrong skip",
        "knowledge_score": 0.9,
        "worth_extracting": true,
        "items": []
    });
    let wrong = observation(&noise, 2, Some(&wrong.to_string()), 20, Some(19));
    assert!(wrong.strict_json_valid);
    assert!(!wrong.production_schema_valid);
    assert!(!wrong.skip_match);
    let invalid = observation(&noise, 3, Some("not json"), 30, None);
    assert!(!invalid.production_schema_valid);
    let missing = observation(&noise, 4, None, 45, None);
    assert!(!missing.request_ok);

    // Merely declaring "worth_extracting=true" with zero knowledge items
    // must not count as a successful positive quality outcome.
    let positive = &cases()[0];
    let empty_positive = serde_json::json!({
        "session_summary": "extracted",
        "knowledge_score": 0.8,
        "worth_extracting": true,
        "items": []
    });
    let empty_positive = observation(positive, 1, Some(&empty_positive.to_string()), 12, None);
    assert!(!empty_positive.production_schema_valid);
    assert!(!empty_positive.skip_match);
    assert_eq!(empty_positive.term_hits, 0);
    assert_eq!(empty_positive.term_total, 2);

    // A partial JSON prefix can contain complete usable-looking knowledge,
    // but that is not proof that the model finished emitting all items.
    // Production scoring must never count that as schema-valid or accurate.
    let truncated = r#"{"session_summary":"partial","knowledge_score":0.9,"worth_extracting":true,"items":[{"title":"timeout retry","category":"general","summary":"timeout","content":"retry","problem":null,"root_causes":[],"solutions":[],"key_commands":null,"key_files":null,"decisions":null,"tags":[],"confidence":0.8},"#;
    let partial = observation(positive, 2, Some(truncated), 23, Some(52));
    assert!(partial.request_ok);
    assert!(!partial.strict_json_valid);
    assert!(!partial.production_schema_valid);
    assert!(!partial.skip_match);
    assert_eq!(partial.term_hits, 0);
    assert_eq!(partial.term_total, 2);

    let report = summarize("local-demo", 1, 4, vec![valid, wrong, invalid, missing]);
    assert_eq!(report.requests, 4);
    assert_eq!(report.request_errors, 1);
    assert_eq!(report.strict_json_valid, 2);
    assert_eq!(report.production_schema_valid, 1);
    assert_eq!(report.skip_matches, 1);
    assert_eq!(report.latency_p50_ms, 20);
    assert_eq!(report.latency_p95_ms, 45);
    assert_eq!(report.reported_token_calls, 1);
    assert_eq!(report.total_reported_tokens, Some(19));
}

#[test]
fn reports_are_explicit_create_only_and_exclude_source_text_and_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("live-benchmark.json");
    let case = &cases()[0];
    let row = observation(case, 1, None, 31, None);
    let report = summarize("local-model", 1, 1, vec![row]);
    save_report(&report, &file).unwrap();
    assert!(save_report(&report, &file).is_err());
    let saved = std::fs::read_to_string(&file).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(parsed["model"], "local-model");
    assert_eq!(parsed["observations"][0]["case_id"], case.id);
    assert!(!saved.contains(case.transcript));
    assert!(!saved.contains("http://"));
    assert!(!saved.contains("api_key"));
    assert!(!saved.contains("Bearer "));
}

/// Explicitly ignored in CI. Real-model results must be collected/reviewed,
/// never reported as successful based on deterministic gold-output fixtures.
#[tokio::test]
#[ignore = "requires AIKS_BENCH_BASE_URL and AIKS_BENCH_MODEL"]
async fn real_model_quality_and_latency() {
    let mut config = AiModelConfig {
        enabled: true,
        base_url: std::env::var("AIKS_BENCH_BASE_URL").expect("set AIKS_BENCH_BASE_URL"),
        model: std::env::var("AIKS_BENCH_MODEL").expect("set AIKS_BENCH_MODEL"),
        ..AiModelConfig::default()
    };
    config.api_key = std::env::var("AIKS_BENCH_API_KEY").ok();
    config.disable_thinking = std::env::var("AIKS_BENCH_DISABLE_THINKING").as_deref() == Ok("1");
    let repeats = configured_repeats(std::env::var("AIKS_BENCH_REPEATS").ok().as_deref())
        .expect("AIKS_BENCH_REPEATS must be 1..=5");
    let model = config.model.clone();
    let client = AiClient::new(config).expect("initialize AI client");
    let all_cases = cases();
    let mut rows = Vec::new();

    for repetition in 1..=repeats {
        for case in &all_cases {
            let prompt = make_v3_extraction_prompt(case.transcript);
            let started = Instant::now();
            let response = client.chat_detailed(SYSTEM_PROMPT_V3, &prompt).await;
            let elapsed_ms = started.elapsed().as_millis() as u64;
            let row = match response {
                Ok(response) => {
                    let tokens = response.usage.and_then(|usage| usage.total_tokens);
                    observation(
                        case,
                        repetition,
                        Some(&response.content),
                        elapsed_ms,
                        tokens,
                    )
                }
                Err(_) => {
                    // Error details from model servers may contain sensitive URL
                    // headers. Only report a failure count and anonymous case ID.
                    observation(case, repetition, None, elapsed_ms, None)
                }
            };
            eprintln!(
                "AIKS_LIVE_CASE case={} repeat={} request_ok={} schema_valid={} elapsed_ms={}",
                row.case_id,
                row.repetition,
                row.request_ok,
                row.production_schema_valid,
                row.elapsed_ms
            );
            rows.push(row);
        }
    }
    let report = summarize(&model, all_cases.len(), repeats, rows);
    eprintln!(
        "AIKS_LIVE_EXTRACTION_BENCHMARK {}",
        serde_json::to_string(&report).expect("serialize benchmark report")
    );

    if let Ok(path) = std::env::var("AIKS_BENCH_REPORT_PATH") {
        save_report(&report, Path::new(&path)).expect("create new benchmark report");
    }
    assert_eq!(report.request_errors, 0, "live model requests failed");
    assert_eq!(
        report.production_schema_valid, report.requests,
        "model schema validation failed"
    );
}
