/// V3 AI Extraction Stage
///
/// Calls the AI model on session chunks and produces 0~N KnowledgeItems.
/// Handles Map-Reduce for long sessions.
use std::time::Instant;

use tracing::info;

use crate::ai::{
    AiClient,
    config::AiModelConfig,
    prompts_v3::{make_v3_chunk_prompt, make_v3_extraction_prompt, make_v3_final_prompt, SYSTEM_PROMPT_V3},
    schema_v3::{V3ExtractionResult, V3KnowledgeItem},
};
use crate::pipeline::knowledge_repo::KnowledgeRepo;
use crate::pipeline::repo::PipelineRepo;
use crate::pipeline::session_chunker::load_chunks;
use crate::storage::StateDb;
use crate::util::SecretSanitizer;

pub struct AiStage {
    client: AiClient,
    sanitizer: SecretSanitizer,
    config: AiModelConfig,
}

impl AiStage {
    pub fn new(config: AiModelConfig) -> anyhow::Result<Self> {
        let client = AiClient::new(config.clone())?;
        Ok(Self {
            client,
            sanitizer: SecretSanitizer::new(),
            config,
        })
    }

    /// Run AI extraction on a session's LLM chunks.
    /// Returns the number of knowledge items created.
    pub async fn run(
        &self,
        db: &StateDb,
        pipeline_run_id: &str,
        session_id: i64,
        session_title: Option<&str>,
        project_name: Option<&str>,
    ) -> anyhow::Result<usize> {
        let pipeline_repo = PipelineRepo::new(db);
        let knowledge_repo = KnowledgeRepo::new(db);

        // Load chunks from DB
        let chunks = load_chunks(db, session_id)?;

        if chunks.is_empty() {
            anyhow::bail!("No chunks found for session {}", session_id);
        }

        let t0 = Instant::now();
        info!(session_id, chunks = chunks.len(), "[AI] Starting extraction");

        let result = if chunks.len() == 1 {
            // Single chunk: direct extraction
            let sanitized = self.sanitizer.sanitize(&chunks[0].1);
            let prompt = make_v3_extraction_prompt(&sanitized);
            let response = self.client.chat(SYSTEM_PROMPT_V3, &prompt).await?;
            // R10: parse errors propagate — model failure / protocol breakage
            // must surface as a stage error, never as a silent "skip".
            parse_v3_result_typed(&response)?
        } else {
            // Multiple chunks: Map-Reduce
            self.map_reduce(session_title, project_name, &chunks).await?
        };

        let latency_ms = t0.elapsed().as_millis() as i64;

        // Log AI request
        log_ai_request(db, pipeline_run_id, "AI_EXTRACT", &self.config.model, &self.config.base_url, chunks.len(), result.items.len(), latency_ms, true);

        if !result.worth_extracting || result.items.is_empty() {
            info!(
                session_id,
                score = result.knowledge_score,
                "[AI] Session not worth extracting or produced 0 items"
            );
            pipeline_repo.record_stage(
                pipeline_run_id, "AI_EXTRACTED", "SUCCESS",
                Some(chunks.len() as i32), Some(0), Some(latency_ms),
                Some(&serde_json::json!({"score": result.knowledge_score, "items": 0})),
                None,
            )?;
            return Ok(0);
        }

        let item_count = result.items.len();
        info!(session_id, items = item_count, latency_ms, "[AI] Extraction complete");

        // Save knowledge items
        knowledge_repo.save_items(session_id, project_name, &result)?;

        pipeline_repo.record_stage(
            pipeline_run_id, "AI_EXTRACTED", "SUCCESS",
            Some(chunks.len() as i32), Some(item_count as i32), Some(latency_ms),
            Some(&serde_json::json!({"score": result.knowledge_score, "items": item_count})),
            None,
        )?;

        Ok(item_count)
    }

    async fn map_reduce(
        &self,
        session_title: Option<&str>,
        project_name: Option<&str>,
        chunks: &[(i32, String)],
    ) -> anyhow::Result<V3ExtractionResult> {
        let total = chunks.len();
        let mut chunk_summaries = Vec::new();

        for (idx, (_, text)) in chunks.iter().enumerate() {
            let sanitized = self.sanitizer.sanitize(text);
            let prompt = make_v3_chunk_prompt(&sanitized, idx, total);
            // R10: a failed chunk means the map-reduce input is incomplete —
            // propagate the error instead of fabricating a degraded summary.
            let resp = self.client.chat(SYSTEM_PROMPT_V3, &prompt).await?;
            chunk_summaries.push(resp);
        }

        let title = session_title.unwrap_or("未知会话");
        let final_prompt = make_v3_final_prompt(title, project_name, &chunk_summaries);
        let response = self.client.chat(SYSTEM_PROMPT_V3, &final_prompt).await?;
        // R10: final parse errors propagate as real failures.
        parse_v3_result_typed(&response)
    }
}

/// B17/R10/R11: Parse V3 result — returns Ok(result) or Err if JSON is invalid/malformed.
/// This separates "no knowledge" (valid skip) from "AI output broken" (error).
/// The error preview is cut on character boundaries (UTF-8 safe).
/// Public for testing.
pub fn parse_v3_result_typed(response: &str) -> anyhow::Result<V3ExtractionResult> {
    let clean = clean_json(response);
    if clean.is_empty() || (!clean.starts_with('{')) {
        anyhow::bail!(
            "AI response is not a JSON object: {:?}",
            crate::util::safe_preview(response, 100)
        );
    }
    match serde_json::from_str::<V3ExtractionResult>(&clean) {
        Ok(r) => Ok(r),
        Err(e) => {
            // Qwen-style models often emit JS-style objects (unquoted keys,
            // trailing commas). Attempt a lenient repair before failing.
            let repaired = repair_json(&clean);
            if repaired != clean {
                if let Ok(r) = serde_json::from_str::<V3ExtractionResult>(&repaired) {
                    return Ok(r);
                }
            }
            Err(anyhow::anyhow!(
                "AI JSON parse failed: {} (response length: {})",
                e,
                response.len()
            ))
        }
    }
}

fn clean_json(s: &str) -> String {
    // Qwen3-style thinking models may wrap answers in <think>...</think>.
    let s = strip_think_blocks(s);
    let s = s.trim();
    let s = if s.starts_with("```") {
        let after = s.trim_start_matches('`').trim_start_matches("json").trim_start_matches('\n');
        if let Some(end) = after.rfind("```") { &after[..end] } else { after }
    } else { s };
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        s[start..=end].to_string()
    } else { s.to_string() }
}

fn strip_think_blocks(s: &str) -> String {
    let mut out = s.to_string();
    loop {
        let Some(start) = out.find("<think>") else { break };
        if let Some(end_rel) = out[start..].find("</think>") {
            let end = start + end_rel + "</think>".len();
            out = format!("{}{}", &out[..start], &out[end..]);
        } else {
            // Unclosed think block: drop everything up to it and keep the tail.
            out = out[start + "<think>".len()..].to_string();
            break;
        }
    }
    out
}

/// Best-effort repair of near-JSON emitted by small local models:
/// - quote unquoted object keys: `{foo: 1}` → `{"foo": 1}`
/// - remove trailing commas: `[1, 2,]` → `[1, 2]`
/// - convert single-quoted strings when the text has no double quotes at all
fn repair_json(s: &str) -> String {
    use regex::Regex;
    let mut out = s.to_string();

    // Only convert single quotes when double quotes are absent (ambiguous otherwise).
    if !out.contains('"') && out.contains('\'') {
        out = out.replace('\'', "\"");
    }

    if let Ok(re) = Regex::new(r#"([\{,]\s*)([A-Za-z_][A-Za-z0-9_\-]*)\s*:"#) {
        out = re.replace_all(&out, "$1\"$2\":").to_string();
    }

    if let Ok(re) = Regex::new(r#",\s*([\}\]])"#) {
        out = re.replace_all(&out, "$1").to_string();
    }

    out
}

fn log_ai_request(
    db: &StateDb,
    pipeline_run_id: &str,
    stage: &str,
    model: &str,
    endpoint: &str,
    input_count: usize,
    output_count: usize,
    latency_ms: i64,
    success: bool,
) {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let status = if success { "SUCCESS" } else { "FAILED" };
    let _ = db.conn().execute(
        "INSERT INTO ai_request_log (id, pipeline_run_id, stage, model, endpoint, input_tokens, output_tokens, latency_ms, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            id, pipeline_run_id, stage, model, endpoint,
            input_count as i64, output_count as i64, latency_ms, status, now
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_v3_response() {
        let json = r#"{
            "session_summary": "Test session",
            "knowledge_score": 0.85,
            "worth_extracting": true,
            "items": [
                {
                    "title": "Test Item",
                    "category": "troubleshooting",
                    "summary": "Test summary",
                    "content": "Test content",
                    "problem": null,
                    "root_causes": ["cause1"],
                    "solutions": ["solution1"],
                    "key_commands": null,
                    "key_files": null,
                    "decisions": null,
                    "tags": ["rust", "test"],
                    "confidence": 0.9
                }
            ]
        }"#;
        let result = parse_v3_result_typed(json).unwrap();
        assert!(result.worth_extracting);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].title, "Test Item");
    }

    #[test]
    fn parse_skip_response() {
        let json = r#"{"session_summary":"trivial","knowledge_score":0.3,"worth_extracting":false,"items":[]}"#;
        let result = parse_v3_result_typed(json).unwrap();
        assert!(!result.worth_extracting);
        assert!(result.items.is_empty());
    }

    #[test]
    fn parse_json_with_fences() {
        let json = "```json\n{\"session_summary\":\"x\",\"knowledge_score\":0.5,\"worth_extracting\":false,\"items\":[]}\n```";
        let result = parse_v3_result_typed(json).unwrap();
        assert!(!result.worth_extracting);
    }

    /// R11: non-JSON CJK responses must produce an error, not panic.
    #[test]
    fn parse_cjk_garbage_is_error_not_panic() {
        let garbage = "中".repeat(40);
        assert!(parse_v3_result_typed(&garbage).is_err());
    }

    /// Qwen-style unquoted keys must be repaired, not failed.
    #[test]
    fn parse_repairs_unquoted_keys() {
        let json = r#"{session_summary:"x", knowledge_score:0.5, worth_extracting:false, items:[]}"#;
        let result = parse_v3_result_typed(json).unwrap();
        assert!(!result.worth_extracting);
    }

    /// Trailing commas and <think> wrappers must not break parsing.
    #[test]
    fn parse_repairs_trailing_comma_and_think_block() {
        let json = "<think>让我想想……这个会话价值不高。</think>\n\
            {\"session_summary\":\"x\",\"knowledge_score\":0.4,\"worth_extracting\":false,\"items\":[,]}";
        let result = parse_v3_result_typed(json).unwrap();
        assert!(!result.worth_extracting);
    }

    /// Single-quoted JSON (no double quotes anywhere) is repaired.
    #[test]
    fn parse_repairs_single_quotes() {
        let json = "{'session_summary':'x','knowledge_score':0.4,'worth_extracting':false,'items':[]}";
        let result = parse_v3_result_typed(json).unwrap();
        assert!(!result.worth_extracting);
    }
}
