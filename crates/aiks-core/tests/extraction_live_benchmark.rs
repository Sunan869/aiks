//! Opt-in, anonymized live-model quality/latency benchmark for S2.1.
//!
//! By default CI only validates the test cases below; it never calls a model.
//! To run against a user-selected OpenAI-compatible endpoint:
//!
//! AIKS_BENCH_BASE_URL=http://127.0.0.1:11434/v1 AIKS_BENCH_MODEL=qwen3 \
//!   cargo test -p aiks-core --test extraction_live_benchmark -- --ignored --nocapture
//!
//! Optional: AIKS_BENCH_API_KEY, AIKS_BENCH_DISABLE_THINKING=1.
//! These synthetic transcripts contain no real session data or credentials.
//! This tests model extraction output, not the end-to-end durable pipeline.

use aiks_core::ai::prompts_v3::{make_v3_extraction_prompt, SYSTEM_PROMPT_V3};
use aiks_core::ai::{AiClient, AiModelConfig, V3ExtractionResult};
use aiks_core::pipeline::ai_stage::parse_v3_result_typed;
use std::collections::HashSet;
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
    let rank = sorted.len().saturating_mul(percentage.min(100)).div_ceil(100);
    sorted[rank.max(1) - 1]
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
}

/// Explicitly ignored in CI: real-model results must be collected and reviewed,
/// never reported as successful based on the fixed gold-output fixture.
#[tokio::test]
#[ignore = "requires AIKS_BENCH_BASE_URL and AIKS_BENCH_MODEL"]
async fn real_model_quality_and_latency() {
    let mut config = AiModelConfig {
        enabled: true,
        base_url: std::env::var("AIKS_BENCH_BASE_URL")
            .expect("explicitly set AIKS_BENCH_BASE_URL"),
        model: std::env::var("AIKS_BENCH_MODEL")
            .expect("explicitly set AIKS_BENCH_MODEL"),
        ..AiModelConfig::default()
    };
    config.api_key = std::env::var("AIKS_BENCH_API_KEY").ok();
    config.disable_thinking =
        std::env::var("AIKS_BENCH_DISABLE_THINKING").as_deref() == Ok("1");
    let model = config.model.clone();
    let client = AiClient::new(config).expect("initialize AI client");

    let mut latencies_ms = Vec::new();
    let mut strict_json_valid = 0usize;
    let mut schema_valid = 0usize;
    let mut skip_matches = 0usize;
    let mut expected_term_hits = 0usize;
    let mut expected_term_total = 0usize;
    let mut reported_token_calls = 0usize;
    let mut total_reported_tokens = 0u64;
    for case in cases() {
        let prompt = make_v3_extraction_prompt(case.transcript);
        let started = Instant::now();
        let response = client
            .chat_detailed(SYSTEM_PROMPT_V3, &prompt)
            .await
            .expect("live model request failed");
        let elapsed = started.elapsed().as_millis() as u64;
        latencies_ms.push(elapsed);
        if let Some(tokens) = response.usage.and_then(|usage| usage.total_tokens) {
            reported_token_calls += 1;
            total_reported_tokens = total_reported_tokens.saturating_add(tokens);
        }
        if serde_json::from_str::<V3ExtractionResult>(&response.content).is_ok() {
            strict_json_valid += 1;
        }
        match parse_v3_result_typed(&response.content) {
            Ok(result) => {
                schema_valid += 1;
                if result.worth_extracting == case.worth_extracting {
                    skip_matches += 1;
                }
                let evidence = result
                    .items
                    .iter()
                    .map(|item| {
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
                for term in case.expected_terms {
                    expected_term_total += 1;
                    if evidence.contains(&term.to_lowercase()) {
                        expected_term_hits += 1;
                    }
                }
            }
            Err(error) => {
                expected_term_total += case.expected_terms.len();
                eprintln!("AIKS_LIVE_CASE_FAIL case={} error={}", case.id, error);
            }
        }
        eprintln!("AIKS_LIVE_CASE case={} elapsed_ms={}", case.id, elapsed);
    }
    eprintln!(
        "AIKS_LIVE_EXTRACTION_BENCHMARK {}",
        serde_json::json!({
            "model": model,
            "cases": latencies_ms.len(),
            "strict_json_valid": strict_json_valid,
            "production_schema_valid": schema_valid,
            "skip_matches": skip_matches,
            "term_hits": expected_term_hits,
            "term_total": expected_term_total,
            "latency_p50_ms": percentile(&latencies_ms, 50),
            "latency_p95_ms": percentile(&latencies_ms, 95),
            "reported_token_calls": reported_token_calls,
            "total_reported_tokens": if reported_token_calls > 0 {
                Some(total_reported_tokens)
            } else {
                None
            }
        })
    );
    assert_eq!(schema_valid, latencies_ms.len(), "model schema validation failed");
}
