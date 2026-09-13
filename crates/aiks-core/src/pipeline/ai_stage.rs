/// V3 AI Extraction Stage
///
/// Calls the AI model on session chunks and produces 0~N KnowledgeItems.
/// Handles Map-Reduce for long sessions.
use std::time::Instant;

use tracing::{info, warn};

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
            parse_v3_result(&response)
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
            match self.client.chat(SYSTEM_PROMPT_V3, &prompt).await {
                Ok(resp) => chunk_summaries.push(resp),
                Err(e) => {
                    warn!(chunk = idx, error = %e, "[AI] Chunk summary failed");
                    chunk_summaries.push(format!("第{}部分摘要失败: {}", idx + 1, e));
                }
            }
        }

        let title = session_title.unwrap_or("未知会话");
        let final_prompt = make_v3_final_prompt(title, project_name, &chunk_summaries);
        let response = self.client.chat(SYSTEM_PROMPT_V3, &final_prompt).await?;
        Ok(parse_v3_result(&response))
    }
}

fn parse_v3_result(response: &str) -> V3ExtractionResult {
    let clean = clean_json(response);
    match serde_json::from_str::<V3ExtractionResult>(&clean) {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "[AI] Failed to parse V3 response, returning skip");
            V3ExtractionResult::skip("解析失败")
        }
    }
}

fn clean_json(s: &str) -> String {
    let s = s.trim();
    let s = if s.starts_with("```") {
        let after = s.trim_start_matches('`').trim_start_matches("json").trim_start_matches('\n');
        if let Some(end) = after.rfind("```") { &after[..end] } else { after }
    } else { s };
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        s[start..=end].to_string()
    } else { s.to_string() }
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
        let result = parse_v3_result(json);
        assert!(result.worth_extracting);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].title, "Test Item");
    }

    #[test]
    fn parse_skip_response() {
        let json = r#"{"session_summary":"trivial","knowledge_score":0.3,"worth_extracting":false,"items":[]}"#;
        let result = parse_v3_result(json);
        assert!(!result.worth_extracting);
        assert!(result.items.is_empty());
    }

    #[test]
    fn parse_json_with_fences() {
        let json = "```json\n{\"session_summary\":\"x\",\"knowledge_score\":0.5,\"worth_extracting\":false,\"items\":[]}\n```";
        let result = parse_v3_result(json);
        assert!(!result.worth_extracting);
    }
}
