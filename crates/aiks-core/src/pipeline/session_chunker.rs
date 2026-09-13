/// V3 Session Chunker — splits sessions for LLM processing
///
/// Unlike V2 chunker (for extraction rendering), this chunker:
/// - Tracks message indices for pipeline stage tracking
/// - Estimates token count (rough: chars / 3.5)
/// - Writes chunks to the session_chunk table

use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

use crate::model::NormalizedMessage;
use crate::pipeline::cleaner::CleanResult;
use crate::storage::StateDb;

const CHARS_PER_TOKEN: f64 = 3.5;
const TARGET_TOKENS: usize = 20_000;
const MAX_MESSAGES: usize = 40;

pub struct SessionChunk {
    pub id: String,
    pub session_id: i64,
    pub chunk_index: i32,
    pub message_start: i32,
    pub message_end: i32,
    pub token_count: i32,
    pub content: String,
}

pub struct ChunkResult {
    pub chunks: Vec<SessionChunk>,
}

/// Estimate token count from character count
fn estimate_tokens(text: &str) -> usize {
    (text.len() as f64 / CHARS_PER_TOKEN) as usize
}

/// Render a message slice to text for AI consumption
fn render_messages(messages: &[&NormalizedMessage]) -> String {
    use crate::model::{ContentBlock, MessageRole};
    use crate::util::{truncate_chars};

    const MAX_TOOL_CALL_CHARS: usize = 500;
    const MAX_TOOL_RESULT_CHARS: usize = 2000;

    let mut parts = Vec::new();
    for msg in messages {
        if msg.role == MessageRole::System { continue; }
        let role = match msg.role {
            MessageRole::User => "用户",
            MessageRole::Assistant => "助手",
            MessageRole::Tool => "工具",
            _ => continue,
        };
        let mut content = Vec::new();
        for block in &msg.blocks {
            match block {
                ContentBlock::Text { text } if !text.is_empty() => content.push(text.clone()),
                ContentBlock::ToolCall { name, input, .. } => {
                    let inp = serde_json::to_string(input).unwrap_or_default();
                    let inp_trunc = truncate_chars(&inp, MAX_TOOL_CALL_CHARS);
                    content.push(format!("[{}] {}", name, inp_trunc));
                }
                ContentBlock::ToolResult { content: c, is_error, .. } => {
                    let prefix = if *is_error { "[错误]" } else { "[结果]" };
                    let trunc = truncate_chars(c.as_str(), MAX_TOOL_RESULT_CHARS);
                    content.push(format!("{} {}", prefix, trunc));
                }
                _ => {}
            }
        }
        if !content.is_empty() {
            parts.push(format!("**{}**: {}", role, content.join(" | ")));
        }
    }
    parts.join("\n")
}

/// Split cleaned messages into chunks for LLM processing
pub fn chunk_for_llm(
    session_id: i64,
    messages: &[NormalizedMessage],
) -> ChunkResult {
    let mut chunks = Vec::new();
    let mut chunk_start = 0;
    let mut chunk_index = 0;

    while chunk_start < messages.len() {
        let mut chunk_end = chunk_start + 1;
        let mut token_acc = 0usize;

        // Expand chunk until token limit or message limit
        while chunk_end < messages.len()
            && (chunk_end - chunk_start) < MAX_MESSAGES
        {
            let msg_text = render_messages(&[&messages[chunk_end]]);
            token_acc += estimate_tokens(&msg_text);
            if token_acc > TARGET_TOKENS {
                break;
            }
            chunk_end += 1;
        }

        let slice: Vec<&NormalizedMessage> = messages[chunk_start..chunk_end].iter().collect();
        let raw_content = render_messages(&slice);

        // B23: If a single chunk exceeds the token limit, truncate to keep it manageable.
        // Use TARGET_TOKENS as the limit to leave headroom for the truncation message overhead.
        const MAX_CHUNK_TOKENS: usize = TARGET_TOKENS; // 20k tokens; test requires <= 25k
        let content = if estimate_tokens(&raw_content) > MAX_CHUNK_TOKENS {
            use crate::util::truncate_chars;
            // Leave room for "...[内容超长已截断]..." footer (~8 tokens)
            let max_chars = ((MAX_CHUNK_TOKENS - 50) as f64 * CHARS_PER_TOKEN) as usize;
            let head = truncate_chars(&raw_content, max_chars);
            format!("{}\n...[内容超长已截断]...", head)
        } else {
            raw_content
        };

        let token_count = estimate_tokens(&content);

        chunks.push(SessionChunk {
            id: Uuid::new_v4().to_string(),
            session_id,
            chunk_index,
            message_start: chunk_start as i32,
            message_end: (chunk_end - 1) as i32,
            token_count: token_count as i32,
            content,
        });

        chunk_start = chunk_end;
        chunk_index += 1;
    }

    ChunkResult { chunks }
}

/// Persist chunks to DB
pub fn save_chunks(db: &StateDb, chunks: &[SessionChunk]) -> anyhow::Result<()> {
    let conn = db.conn();
    let now = Utc::now().to_rfc3339();

    // Delete old chunks for this session
    if let Some(first) = chunks.first() {
        conn.execute(
            "DELETE FROM session_chunk WHERE session_id = ?1",
            params![first.session_id],
        )?;
    }

    for chunk in chunks {
        use sha2::{Sha256, Digest};
        let hash = hex::encode(Sha256::digest(chunk.content.as_bytes()));
        conn.execute(
            "INSERT OR REPLACE INTO session_chunk
             (id, session_id, chunk_index, message_start, message_end, token_count, content, content_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                chunk.id, chunk.session_id, chunk.chunk_index,
                chunk.message_start, chunk.message_end,
                chunk.token_count, chunk.content, hash, now
            ],
        )?;
    }
    Ok(())
}

/// Load chunks from DB for a session
pub fn load_chunks(db: &StateDb, session_id: i64) -> anyhow::Result<Vec<(i32, String)>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT chunk_index, content FROM session_chunk WHERE session_id = ?1 ORDER BY chunk_index"
    )?;
    let result: Vec<(i32, String)> = stmt
        .query_map(params![session_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentBlock, MessageRole, NormalizedMessage};
    use std::collections::HashMap;

    fn make_msg(i: usize) -> NormalizedMessage {
        NormalizedMessage {
            external_id: format!("m{}", i),
            parent_id: None,
            role: if i % 2 == 0 { MessageRole::User } else { MessageRole::Assistant },
            created_at: None,
            model: None,
            blocks: vec![ContentBlock::Text { text: format!("Message content {}", "x".repeat(100)) }],
            usage: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn chunking_splits_large_sessions() {
        let messages: Vec<_> = (0..100).map(make_msg).collect();
        let result = chunk_for_llm(1, &messages);
        assert!(result.chunks.len() > 1, "100 messages should produce multiple chunks");
        // Each chunk should have at most MAX_MESSAGES messages
        for chunk in &result.chunks {
            let span = chunk.message_end - chunk.message_start + 1;
            assert!(span <= MAX_MESSAGES as i32 + 1);
        }
    }

    #[test]
    fn chunking_small_session_is_one_chunk() {
        let messages: Vec<_> = (0..5).map(make_msg).collect();
        let result = chunk_for_llm(1, &messages);
        assert_eq!(result.chunks.len(), 1);
    }
}
