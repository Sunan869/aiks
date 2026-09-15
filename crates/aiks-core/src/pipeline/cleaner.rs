/// Session Cleaner — removes noisy messages before LLM processing
///
/// Removes:
/// - Empty messages
/// - Duplicate consecutive tool outputs
/// - Excessively long stdout blobs
/// - Repetitive step start/finish messages
///
/// Preserves:
/// - Error messages
/// - Commands
/// - Paths and versions
/// - Code blocks
/// - Final results
use crate::model::{ContentBlock, NormalizedMessage};

/// Result of cleaning a session
pub struct CleanResult {
    pub original_count: usize,
    pub cleaned_count: usize,
    pub removed_count: usize,
    pub messages: Vec<NormalizedMessage>,
    pub removed_indices: Vec<usize>,
}

/// Clean a list of messages for LLM processing
pub fn clean_messages(messages: Vec<NormalizedMessage>) -> CleanResult {
    let original_count = messages.len();
    let mut cleaned = Vec::with_capacity(messages.len());
    let mut removed_indices = Vec::new();

    for (idx, msg) in messages.into_iter().enumerate() {
        if should_remove(&msg) {
            removed_indices.push(idx);
        } else {
            cleaned.push(truncate_tool_results(msg));
        }
    }

    let cleaned_count = cleaned.len();
    let removed_count = removed_indices.len();

    CleanResult {
        original_count,
        cleaned_count,
        removed_count,
        messages: cleaned,
        removed_indices,
    }
}

/// Check if a message should be removed
fn should_remove(msg: &NormalizedMessage) -> bool {
    // Empty messages
    if msg.blocks.is_empty() {
        return true;
    }

    // Check if all blocks are empty/whitespace
    let has_content = msg.blocks.iter().any(|b| match b {
        ContentBlock::Text { text } => !text.trim().is_empty(),
        ContentBlock::Thinking { text } => !text.trim().is_empty(),
        ContentBlock::ToolCall { .. } => true,
        ContentBlock::ToolResult { content, .. } => !content.trim().is_empty(),
        _ => true,
    });

    if !has_content {
        return true;
    }

    // Remove pure step start/finish messages (brief assistant messages about progress)
    if is_step_noise(msg) {
        return true;
    }

    false
}

/// Detect step noise messages (brief progress announcements without real content)
fn is_step_noise(msg: &NormalizedMessage) -> bool {
    use crate::model::MessageRole;

    // Only filter assistant messages
    if msg.role != MessageRole::Assistant {
        return false;
    }

    // Only if there's a single text block
    if msg.blocks.len() != 1 {
        return false;
    }

    if let ContentBlock::Text { text } = &msg.blocks[0] {
        let trimmed = text.trim();
        // Very short messages that look like step announcements
        if trimmed.len() < 50 {
            let lower = trimmed.to_lowercase();
            if lower.starts_with("let me")
                || lower.starts_with("i'll ")
                || lower.starts_with("now ")
            {
                return true;
            }
        }
    }

    false
}

/// Truncate excessively long tool results to avoid token waste
fn truncate_tool_results(mut msg: NormalizedMessage) -> NormalizedMessage {
    const MAX_TOOL_RESULT_CHARS: usize = 8000;

    msg.blocks = msg
        .blocks
        .into_iter()
        .map(|block| {
            match block {
                ContentBlock::ToolResult {
                    id,
                    content,
                    is_error,
                } => {
                    // R11: cut on character boundaries — byte slicing panics on CJK.
                    if content.chars().count() > MAX_TOOL_RESULT_CHARS && !is_error {
                        let truncated = format!(
                            "{}\n\n[... {} chars truncated ...]",
                            crate::util::truncate_chars(&content, MAX_TOOL_RESULT_CHARS),
                            content.chars().count() - MAX_TOOL_RESULT_CHARS
                        );
                        ContentBlock::ToolResult {
                            id,
                            content: truncated,
                            is_error,
                        }
                    } else {
                        ContentBlock::ToolResult {
                            id,
                            content,
                            is_error,
                        }
                    }
                }
                other => other,
            }
        })
        .collect();

    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentBlock, MessageRole, NormalizedMessage};

    fn make_message(role: MessageRole, text: &str) -> NormalizedMessage {
        NormalizedMessage {
            external_id: "test".to_string(),
            parent_id: None,
            role,
            created_at: None,
            model: None,
            blocks: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            usage: None,
            metadata: Default::default(),
        }
    }

    #[test]
    fn clean_removes_empty() {
        let messages = vec![
            make_message(MessageRole::User, "hello"),
            NormalizedMessage {
                external_id: "empty".to_string(),
                parent_id: None,
                role: MessageRole::Assistant,
                created_at: None,
                model: None,
                blocks: vec![],
                usage: None,
                metadata: Default::default(),
            },
            make_message(MessageRole::Assistant, "world"),
        ];

        let result = clean_messages(messages);
        assert_eq!(result.original_count, 3);
        assert_eq!(result.cleaned_count, 2);
        assert_eq!(result.removed_count, 1);
    }

    #[test]
    fn clean_preserves_error_tool_result() {
        let messages = vec![NormalizedMessage {
            external_id: "tool".to_string(),
            parent_id: None,
            role: MessageRole::Tool,
            created_at: None,
            model: None,
            blocks: vec![ContentBlock::ToolResult {
                id: Some("t1".to_string()),
                content: "error: compilation failed".to_string(),
                is_error: true,
            }],
            usage: None,
            metadata: Default::default(),
        }];

        let result = clean_messages(messages);
        assert_eq!(result.cleaned_count, 1);
    }
}
