/// Session chunker for long sessions (spec §15-16).
///
/// Splits large sessions into chunks, extracts per-chunk summaries (Map),
/// then runs final extraction over all summaries (Reduce).
use crate::model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession};

const MAX_TOOL_RESULT_CHARS: usize = 100_000;
const TOOL_RESULT_HEAD: usize = 30_000;
const TOOL_RESULT_TAIL: usize = 30_000;

/// Render a NormalizedSession to compact text for AI consumption.
/// Applies truncation rules (spec §20).
pub fn render_session_for_ai(
    session: &NormalizedSession,
    include_thinking: bool,
) -> String {
    let mut parts = Vec::new();

    if let Some(title) = &session.title {
        parts.push(format!("# {}\n", title));
    }
    if let Some(project) = &session.project_path {
        parts.push(format!("项目路径: {}\n", project));
    }

    for msg in &session.messages {
        // Skip system messages (usually injected context)
        if msg.role == MessageRole::System {
            continue;
        }

        let role_label = match msg.role {
            MessageRole::User => "用户",
            MessageRole::Assistant => "助手",
            MessageRole::Tool => "工具",
            _ => continue,
        };

        let mut msg_parts = Vec::new();
        for block in &msg.blocks {
            match block {
                ContentBlock::Text { text } if !text.is_empty() => {
                    msg_parts.push(text.clone());
                }
                ContentBlock::Thinking { text } if include_thinking && !text.is_empty() => {
                    msg_parts.push(format!("[思考] {}", text));
                }
                ContentBlock::ToolCall { name, input, .. } => {
                    let input_str = if input.is_null() {
                        String::new()
                    } else {
                        serde_json::to_string(input).unwrap_or_default()
                    };
                    if !input_str.is_empty() && input_str != "null" {
                        msg_parts.push(format!("[工具调用] {}: {}", name, &input_str[..input_str.len().min(500)]));
                    } else {
                        msg_parts.push(format!("[工具调用] {}", name));
                    }
                }
                ContentBlock::ToolResult { content, is_error, .. } => {
                    let display = truncate_tool_result(content);
                    if *is_error {
                        msg_parts.push(format!("[工具错误] {}", display));
                    } else {
                        msg_parts.push(format!("[工具结果] {}", display));
                    }
                }
                _ => {}
            }
        }

        if !msg_parts.is_empty() {
            parts.push(format!("\n**{}**:\n{}", role_label, msg_parts.join("\n")));
        }
    }

    parts.join("\n")
}

fn truncate_tool_result(content: &str) -> String {
    if content.len() <= MAX_TOOL_RESULT_CHARS {
        return content.to_string();
    }
    let head = &content[..TOOL_RESULT_HEAD.min(content.len())];
    let tail_start = content.len().saturating_sub(TOOL_RESULT_TAIL);
    let tail = &content[tail_start..];
    format!("{}\n...[中间内容已截断]...\n{}", head, tail)
}

/// Split messages into chunks of max `chunk_size` messages.
pub fn chunk_messages(messages: &[NormalizedMessage], chunk_size: usize) -> Vec<Vec<&NormalizedMessage>> {
    messages
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .collect::<Vec<_>>()
        .chunks(chunk_size)
        .map(|c| c.to_vec())
        .collect()
}

/// Render a chunk of messages to text
pub fn render_chunk(messages: &[&NormalizedMessage], include_thinking: bool) -> String {
    let mut parts = Vec::new();
    for msg in messages {
        let role_label = match msg.role {
            MessageRole::User => "用户",
            MessageRole::Assistant => "助手",
            MessageRole::Tool => "工具",
            _ => continue,
        };
        let mut msg_parts = Vec::new();
        for block in &msg.blocks {
            match block {
                ContentBlock::Text { text } if !text.is_empty() => msg_parts.push(text.clone()),
                ContentBlock::Thinking { text } if include_thinking => msg_parts.push(format!("[思考] {}", text)),
                ContentBlock::ToolCall { name, input, .. } => {
                    let inp = serde_json::to_string(input).unwrap_or_default();
                    msg_parts.push(format!("[{}] {}", name, &inp[..inp.len().min(300)]));
                }
                ContentBlock::ToolResult { content, .. } => {
                    msg_parts.push(format!("[结果] {}", &truncate_tool_result(content)[..truncate_tool_result(content).len().min(2000)]));
                }
                _ => {}
            }
        }
        if !msg_parts.is_empty() {
            parts.push(format!("**{}**: {}", role_label, msg_parts.join(" | ")));
        }
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use std::collections::HashMap;

    #[test]
    fn chunk_messages_splits_correctly() {
        let msgs: Vec<NormalizedMessage> = (0..100)
            .map(|i| NormalizedMessage {
                external_id: format!("msg-{}", i),
                parent_id: None,
                role: MessageRole::User,
                created_at: None,
                model: None,
                blocks: vec![ContentBlock::Text { text: format!("msg {}", i) }],
                usage: None,
                metadata: HashMap::new(),
            })
            .collect();
        let chunks = chunk_messages(&msgs, 40);
        assert_eq!(chunks.len(), 3); // 40, 40, 20
        assert_eq!(chunks[0].len(), 40);
        assert_eq!(chunks[2].len(), 20);
    }

    #[test]
    fn tool_result_truncation() {
        let long = "x".repeat(200_000);
        let result = truncate_tool_result(&long);
        assert!(result.contains("中间内容已截断"));
        assert!(result.len() < long.len());
    }
}
