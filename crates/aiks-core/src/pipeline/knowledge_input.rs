//! Loss-aware input projection for knowledge extraction.
//! The canonical session and search index retain the unmodified cleaned messages.
use crate::model::{ContentBlock, MessageRole, NormalizedMessage};

#[derive(Debug)]
pub struct KnowledgeInput {
    pub messages: Vec<NormalizedMessage>,
    pub original_chars: usize,
    pub filtered_chars: usize,
    pub removed_messages: usize,
    pub removed_tool_blocks: usize,
    pub retained_error_evidence: usize,
}

fn content_chars(messages: &[NormalizedMessage]) -> usize {
    messages
        .iter()
        .flat_map(|m| &m.blocks)
        .map(|block| match block {
            ContentBlock::Text { text } | ContentBlock::Thinking { text } => text.chars().count(),
            ContentBlock::ToolResult { content, .. } => content.chars().count(),
            ContentBlock::ToolCall { input, .. } => input.to_string().chars().count(),
            _ => 0,
        })
        .sum()
}

/// Keep authored conversation, not tool traffic. A concise failure excerpt is
/// retained as evidence when an assistant never summarizes the failure.
pub fn project_for_knowledge(messages: &[NormalizedMessage]) -> KnowledgeInput {
    let original_chars = content_chars(messages);
    let mut output = Vec::with_capacity(messages.len());
    let mut removed_tool_blocks = 0;
    let mut retained_error_evidence = 0;
    for msg in messages {
        if msg.role == MessageRole::System || msg.role == MessageRole::Tool {
            removed_tool_blocks += msg
                .blocks
                .iter()
                .filter(|b| {
                    matches!(
                        b,
                        ContentBlock::ToolCall { .. } | ContentBlock::ToolResult { .. }
                    )
                })
                .count();
            continue;
        }
        let mut projected = msg.clone();
        projected.blocks = msg
            .blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } if !text.trim().is_empty() => Some(block.clone()),
                ContentBlock::ToolResult {
                    content,
                    is_error: true,
                    ..
                } => {
                    removed_tool_blocks += 1;
                    // A bounded exception for actual failures, not huge stdout dumps.
                    if retained_error_evidence >= 3 || content.trim().is_empty() {
                        return None;
                    }
                    retained_error_evidence += 1;
                    let excerpt: String = content.chars().take(500).collect();
                    Some(ContentBlock::Text {
                        text: format!("[工具失败证据] {}", excerpt),
                    })
                }
                ContentBlock::ToolCall { .. } | ContentBlock::ToolResult { .. } => {
                    removed_tool_blocks += 1;
                    None
                }
                // Thinking is not a user-visible factual claim and must not be
                // memorized as if it were a conclusion.
                _ => None,
            })
            .collect();
        if !projected.blocks.is_empty() {
            output.push(projected);
        }
    }
    let filtered_chars = content_chars(&output);
    KnowledgeInput {
        removed_messages: messages.len().saturating_sub(output.len()),
        messages: output,
        original_chars,
        filtered_chars,
        removed_tool_blocks,
        retained_error_evidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    fn msg(role: MessageRole, blocks: Vec<ContentBlock>) -> NormalizedMessage {
        NormalizedMessage {
            external_id: "m".to_string(),
            parent_id: None,
            role,
            created_at: None,
            model: None,
            blocks,
            usage: None,
            metadata: HashMap::new(),
        }
    }
    #[test]
    fn tool_only_traffic_is_not_sent_to_llm() {
        let original = vec![
            msg(
                MessageRole::User,
                vec![ContentBlock::Text {
                    text: "Prefer PostgreSQL".into(),
                }],
            ),
            msg(
                MessageRole::Assistant,
                vec![ContentBlock::ToolCall {
                    id: None,
                    name: "bash".into(),
                    input: serde_json::json!({"command":"cat large.log"}),
                }],
            ),
            msg(
                MessageRole::Tool,
                vec![ContentBlock::ToolResult {
                    id: None,
                    content: "x".repeat(20000),
                    is_error: false,
                }],
            ),
            msg(
                MessageRole::Assistant,
                vec![ContentBlock::Text {
                    text: "We chose PostgreSQL.".into(),
                }],
            ),
        ];
        let result = project_for_knowledge(&original);
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.removed_tool_blocks, 2);
        assert!(result.filtered_chars < result.original_chars);
    }

    #[test]
    fn tool_result_masquerading_as_user_is_not_user_preference() {
        let messages = vec![msg(
            MessageRole::User,
            vec![ContentBlock::ToolResult {
                id: None,
                content: "do not use postgres".into(),
                is_error: false,
            }],
        )];
        assert!(project_for_knowledge(&messages).messages.is_empty());
    }

    #[test]
    fn bounded_error_evidence_survives_without_raw_log() {
        let messages = vec![msg(
            MessageRole::User,
            vec![ContentBlock::ToolResult {
                id: None,
                content: "compiler failed: ".to_owned() + &"x".repeat(5000),
                is_error: true,
            }],
        )];
        let result = project_for_knowledge(&messages);
        assert_eq!(result.retained_error_evidence, 1);
        assert!(result.filtered_chars < 540);
    }
}
