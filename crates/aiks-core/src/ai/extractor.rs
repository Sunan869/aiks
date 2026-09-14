/// Knowledge Extractor — converts NormalizedSession to KnowledgeDocument.
///
/// For short sessions: single-pass extraction.
/// For long sessions: chunk → chunk summaries → final extraction (Map-Reduce).
use tracing::{debug, info, warn};

use crate::ai::{
    AiClient,
    chunker::{chunk_messages, render_chunk, render_session_for_ai},
    config::AiModelConfig,
    prompts::{
        make_chunk_prompt, make_extraction_prompt, make_final_extraction_prompt,
        SYSTEM_PROMPT,
    },
    schema::KnowledgeDocument,
};
use crate::model::NormalizedSession;
use crate::util::SecretSanitizer;

pub const EXTRACTOR_VERSION: &str = "extractor-v1";

pub struct KnowledgeExtractor {
    client: AiClient,
    sanitizer: SecretSanitizer,
}

impl KnowledgeExtractor {
    pub fn new(config: AiModelConfig) -> anyhow::Result<Self> {
        let client = AiClient::new(config)?;
        Ok(Self {
            client,
            sanitizer: SecretSanitizer::new(),
        })
    }

    pub fn config(&self) -> &AiModelConfig {
        self.client.config()
    }

    /// Extract knowledge from a session.
    /// Automatically chunks long sessions (spec §15).
    /// Always sanitizes secrets before sending to AI (spec §39).
    pub async fn extract(&self, session: &NormalizedSession) -> anyhow::Result<KnowledgeDocument> {
        let include_thinking = false; // spec §19: thinking OFF by default
        let chunk_size = self.client.config().chunk_size_messages;

        let non_system_msgs: Vec<_> = session.messages.iter()
            .filter(|m| m.role != crate::model::MessageRole::System)
            .collect();

        let doc = if non_system_msgs.len() <= chunk_size {
            // Short session: single pass
            self.extract_single(session, include_thinking).await?
        } else {
            // Long session: chunk + reduce
            self.extract_chunked(session, include_thinking).await?
        };

        Ok(doc)
    }

    async fn extract_single(
        &self,
        session: &NormalizedSession,
        include_thinking: bool,
    ) -> anyhow::Result<KnowledgeDocument> {
        let raw_text = render_session_for_ai(session, include_thinking);
        let sanitized = self.sanitizer.sanitize(&raw_text);
        let user_prompt = make_extraction_prompt(&sanitized);

        let response = self.client.chat(SYSTEM_PROMPT, &user_prompt).await?;
        self.parse_response(&response).await
    }

    async fn extract_chunked(
        &self,
        session: &NormalizedSession,
        include_thinking: bool,
    ) -> anyhow::Result<KnowledgeDocument> {
        let chunk_size = self.client.config().chunk_size_messages;
        let chunks = chunk_messages(&session.messages, chunk_size);
        let total = chunks.len();
        info!(total_chunks = total, session_id = %session.external_session_id, "Chunked extraction");

        let mut chunk_summaries = Vec::new();

        for (i, chunk) in chunks.iter().enumerate() {
            debug!(chunk = i, total, "Processing chunk");
            let chunk_text = render_chunk(chunk, include_thinking);
            let sanitized = self.sanitizer.sanitize(&chunk_text);
            let prompt = make_chunk_prompt(&sanitized, i, total);

            match self.client.chat(SYSTEM_PROMPT, &prompt).await {
                Ok(resp) => {
                    chunk_summaries.push(resp);
                }
                Err(e) => {
                    warn!(chunk = i, error = %e, "Chunk extraction failed, using fallback");
                    chunk_summaries.push(format!("第{}部分处理失败: {}", i + 1, e));
                }
            }
        }

        // Final extraction from chunk summaries
        let title = session.title.as_deref().unwrap_or("未知会话");
        let project = session.project_path.as_deref();
        let final_prompt = make_final_extraction_prompt(title, project, &chunk_summaries);

        let response = self.client.chat(SYSTEM_PROMPT, &final_prompt).await?;
        self.parse_response(&response).await
    }

    /// Parse the AI response JSON, with one retry on parse failure (spec §80).
    async fn parse_response(&self, response: &str) -> anyhow::Result<KnowledgeDocument> {
        let clean = clean_json_response(response);
        match serde_json::from_str::<KnowledgeDocument>(&clean) {
            Ok(doc) => Ok(doc),
            Err(first_err) => {
                warn!(error = %first_err, "JSON parse failed, retrying with fix prompt");
                let fix_prompt = format!(
                    "请仅修复以下 JSON，使其满足 Schema，不要添加额外内容：\n{}",
                    &response[..response.len().min(2000)]
                );
                let retry_resp = self.client.chat(SYSTEM_PROMPT, &fix_prompt).await?;
                let clean2 = clean_json_response(&retry_resp);
                serde_json::from_str::<KnowledgeDocument>(&clean2)
                    .map_err(|e| anyhow::anyhow!("JSON parse failed after retry: {} (original: {})", e, first_err))
            }
        }
    }
}

/// Strip markdown code fences from JSON response
fn clean_json_response(s: &str) -> String {
    let s = s.trim();
    // Remove ```json ... ``` or ``` ... ```
    let s = if s.starts_with("```") {
        let after = s.trim_start_matches('`').trim_start_matches("json").trim_start_matches('\n');
        if let Some(end) = after.rfind("```") {
            &after[..end]
        } else {
            after
        }
    } else {
        s
    };
    // Find JSON object boundaries
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        s[start..=end].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_json_removes_fences() {
        let raw = "```json\n{\"worth_extracting\":true}\n```";
        assert_eq!(clean_json_response(raw), "{\"worth_extracting\":true}");
    }

    #[test]
    fn clean_json_keeps_plain_json() {
        let raw = "  {\"worth_extracting\":false}  ";
        assert_eq!(clean_json_response(raw), "{\"worth_extracting\":false}");
    }
}
