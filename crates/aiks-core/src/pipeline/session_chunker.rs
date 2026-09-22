// CI lint baseline: pre-existing Clippy debt; remove allowances incrementally.
#![allow(clippy::manual_is_multiple_of)]

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
use crate::storage::StateDb;

const CHARS_PER_TOKEN: f64 = 3.5;
/// Token budget per chunk (ESTIMATED via chars/3.5).
///
/// 20_000 was too generous: the estimate undercounts code/symbol-heavy
/// content by ~1.6x (observed live: an estimated-20K chunk cost 31.7K real
/// Qwen tokens and could never fit a 32K context at any max_tokens).
/// 12_000 estimated keeps the worst case (~19K real tokens) well inside
/// the window after adding the prompt frame + 8K output budget.
const TARGET_TOKENS: usize = 12_000;
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

/// Estimate token count from character count.
/// R12: estimate from CHARACTERS (not bytes) so CJK text is budgeted
/// consistently with the character-based chunk splitting.
fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() as f64 / CHARS_PER_TOKEN).ceil() as usize
}

/// Render a message slice to text for AI consumption
fn render_messages(messages: &[&NormalizedMessage]) -> String {
    use crate::model::{ContentBlock, MessageRole};
    use crate::util::truncate_chars;

    const MAX_TOOL_CALL_CHARS: usize = 500;
    const MAX_TOOL_RESULT_CHARS: usize = 2000;

    let mut parts = Vec::new();
    for msg in messages {
        if msg.role == MessageRole::System {
            continue;
        }
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
                ContentBlock::ToolResult {
                    content: c,
                    is_error,
                    ..
                } => {
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

/// Push one chunk covering [start, end] with the given content.
fn push_chunk(
    chunks: &mut Vec<SessionChunk>,
    session_id: i64,
    chunk_index: i32,
    message_start: usize,
    message_end: usize,
    content: String,
) {
    let token_count = estimate_tokens(&content);
    chunks.push(SessionChunk {
        id: Uuid::new_v4().to_string(),
        session_id,
        chunk_index,
        message_start: message_start as i32,
        message_end: message_end as i32,
        token_count: token_count as i32,
        content,
    });
}

/// Split cleaned messages into chunks for LLM processing.
///
/// R12 fixes:
/// - The budget estimate and the splitting both operate on characters, so CJK
///   content no longer exceeds the token budget.
/// - The FIRST message of each chunk is counted in the accumulated budget.
/// - A single message larger than the whole budget is split into multiple
///   chunks covering its full text — nothing is truncated, no tail is lost.
pub fn chunk_for_llm(session_id: i64, messages: &[NormalizedMessage]) -> ChunkResult {
    let mut chunks: Vec<SessionChunk> = Vec::new();
    let mut chunk_index: i32 = 0;

    // Accumulator: (message_index, rendered_text)
    let mut current: Vec<(usize, String)> = Vec::new();
    let mut current_tokens = 0usize;

    let flush = |current: &mut Vec<(usize, String)>,
                 chunks: &mut Vec<SessionChunk>,
                 chunk_index: &mut i32| {
        if current.is_empty() {
            return;
        }
        let start = current[0].0;
        let end = current[current.len() - 1].0;
        let content = current
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        push_chunk(chunks, session_id, *chunk_index, start, end, content);
        *chunk_index += 1;
        current.clear();
    };

    for (i, msg) in messages.iter().enumerate() {
        let text = render_messages(&[msg]);
        if text.is_empty() {
            // System/empty messages carry nothing for the LLM — skip entirely.
            continue;
        }
        let tokens = estimate_tokens(&text);

        // Single message exceeding the whole budget: flush, then split the
        // message text into budget-sized pieces. All content is preserved.
        if tokens > TARGET_TOKENS {
            flush(&mut current, &mut chunks, &mut chunk_index);
            let total_chars = text.chars().count();
            let max_chars = ((TARGET_TOKENS as f64) * CHARS_PER_TOKEN) as usize;
            let mut char_pos = 0usize;
            while char_pos < total_chars {
                let end = (char_pos + max_chars).min(total_chars);
                let piece: String = text.chars().skip(char_pos).take(end - char_pos).collect();
                push_chunk(&mut chunks, session_id, chunk_index, i, i, piece);
                chunk_index += 1;
                char_pos = end;
            }
            continue;
        }

        // Accumulate into the current chunk; flush first when adding this
        // message would exceed the budget or the message-count limit.
        if !current.is_empty()
            && (current_tokens + tokens > TARGET_TOKENS || current.len() >= MAX_MESSAGES)
        {
            flush(&mut current, &mut chunks, &mut chunk_index);
            current_tokens = 0;
        }
        current_tokens += tokens;
        current.push((i, text));
    }
    flush(&mut current, &mut chunks, &mut chunk_index);

    ChunkResult { chunks }
}

/// Persist chunks to DB
pub fn save_chunks(db: &StateDb, chunks: &[SessionChunk]) -> anyhow::Result<()> {
    save_chunks_guarded(db, chunks, None)
}

pub fn save_chunks_guarded(
    db: &StateDb,
    chunks: &[SessionChunk],
    fence: Option<&crate::service::RevisionFence>,
) -> anyhow::Result<()> {
    let mut locked = db.conn();
    let conn = locked.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Some(fence) = fence {
        fence.check_in_tx(&conn)?;
        anyhow::ensure!(
            chunks
                .iter()
                .all(|chunk| chunk.session_id == fence.session_id),
            "Chunk belongs to another revision session"
        );
        conn.execute(
            "DELETE FROM session_chunk WHERE session_id=?1",
            [fence.session_id],
        )?;
    }
    let now = Utc::now().to_rfc3339();

    // Delete old chunks for this session
    if let Some(first) = chunks.first() {
        conn.execute(
            "DELETE FROM session_chunk WHERE session_id = ?1",
            params![first.session_id],
        )?;
    }

    for chunk in chunks {
        use sha2::{Digest, Sha256};
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
    conn.commit()?;
    Ok(())
}

/// Load chunks from DB for a session
pub fn load_chunks(db: &StateDb, session_id: i64) -> anyhow::Result<Vec<(i32, String)>> {
    load_chunks_guarded(db, session_id, None)
}

pub fn load_chunks_guarded(
    db: &StateDb,
    session_id: i64,
    fence: Option<&crate::service::RevisionFence>,
) -> anyhow::Result<Vec<(i32, String)>> {
    let mut locked = db.conn();
    let conn = locked.transaction()?;
    if let Some(fence) = fence {
        fence.check_session_in_tx(&conn, session_id)?;
    }
    let mut stmt = conn.prepare(
        "SELECT chunk_index, content FROM session_chunk WHERE session_id = ?1 ORDER BY chunk_index",
    )?;
    let result: Vec<(i32, String)> = stmt
        .query_map(params![session_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
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
            role: if i % 2 == 0 {
                MessageRole::User
            } else {
                MessageRole::Assistant
            },
            created_at: None,
            model: None,
            blocks: vec![ContentBlock::Text {
                text: format!("Message content {}", "x".repeat(100)),
            }],
            usage: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn chunking_splits_large_sessions() {
        let messages: Vec<_> = (0..100).map(make_msg).collect();
        let result = chunk_for_llm(1, &messages);
        assert!(
            result.chunks.len() > 1,
            "100 messages should produce multiple chunks"
        );
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

    /// R12: an oversized CJK message must be split within budget, with the
    /// tail preserved (no truncation loss).
    #[test]
    fn chunking_long_cjk_message_stays_in_budget_and_keeps_tail() {
        let messages = vec![NormalizedMessage {
            external_id: "m".to_string(),
            parent_id: None,
            role: MessageRole::User,
            created_at: None,
            model: None,
            blocks: vec![ContentBlock::Text {
                text: format!("{}AUDIT_TAIL", "中".repeat(80_000)),
            }],
            usage: None,
            metadata: HashMap::new(),
        }];
        let result = chunk_for_llm(1, &messages);
        assert!(
            result.chunks.len() > 1,
            "80k CJK chars must split into multiple chunks"
        );
        for chunk in &result.chunks {
            assert!(
                chunk.token_count <= TARGET_TOKENS as i32,
                "chunk token_count {} exceeds budget {}",
                chunk.token_count,
                TARGET_TOKENS
            );
        }
        let all = result
            .chunks
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("");
        assert!(
            all.contains("AUDIT_TAIL"),
            "tail marker must survive chunking"
        );
    }

    /// R12: chunks cover all messages — no message is dropped.
    #[test]
    fn chunking_covers_all_messages() {
        let messages: Vec<_> = (0..100).map(make_msg).collect();
        let result = chunk_for_llm(1, &messages);
        assert_eq!(result.chunks.first().unwrap().message_start, 0);
        assert_eq!(result.chunks.last().unwrap().message_end, 99);
        // Consecutive coverage
        for w in result.chunks.windows(2) {
            assert_eq!(w[0].message_end + 1, w[1].message_start);
        }
    }
}
