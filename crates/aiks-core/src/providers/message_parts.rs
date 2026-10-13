//! Pure conversion helpers. No filesystem, network, or command execution.
use super::SessionSummary;
use crate::model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;

pub(crate) fn string(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
}
pub(crate) fn timestamp(v: &Value) -> Option<DateTime<Utc>> {
    match v {
        Value::String(s) => DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|t| t.with_timezone(&Utc)),
        Value::Number(n) => n.as_i64().and_then(DateTime::from_timestamp_millis),
        _ => None,
    }
}
pub(crate) fn content_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items)
            if items
                .iter()
                .all(|v| v.get("text").and_then(Value::as_str).is_some()) =>
        {
            items
                .iter()
                .filter_map(|v| v["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        }
        Value::Null => String::new(),
        v => v.to_string(),
    }
}
fn arguments(v: &Value) -> Value {
    v.as_str()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| v.clone())
}
pub(crate) fn blocks(v: &Value) -> Vec<ContentBlock> {
    if let Some(items) = v.as_array() {
        return items.iter().flat_map(blocks).collect();
    }
    if let Some(text) = v.as_str() {
        return vec![ContentBlock::Text { text: text.into() }];
    }
    if v.is_null() {
        return Vec::new();
    }
    if let Some(call) = v.get("functionCall") {
        return vec![ContentBlock::ToolCall {
            id: string(call, "id"),
            name: string(call, "name").unwrap_or_else(|| "unknown".into()),
            input: call.get("args").cloned().unwrap_or(Value::Null),
        }];
    }
    if let Some(result) = v.get("functionResponse") {
        return vec![ContentBlock::ToolResult {
            id: string(result, "id"),
            content: content_text(&result["response"]),
            is_error: result
                .get("response")
                .and_then(|r| r.get("error"))
                .is_some_and(|e| !e.is_null()),
        }];
    }
    let kind = v["type"].as_str().unwrap_or("");
    let block = match kind {
        "redacted" => return Vec::new(),
        "text" | "input_text" | "output_text" | "" if v["text"].is_string() => {
            let text = v["text"].as_str().unwrap().to_owned();
            if v["thought"].as_bool() == Some(true) {
                ContentBlock::Thinking { text }
            } else {
                ContentBlock::Text { text }
            }
        }
        "thinking" | "think" | "reasoning" => match v
            .get("thinking")
            .or_else(|| v.get("text"))
            .and_then(Value::as_str)
        {
            Some(text) => ContentBlock::Thinking { text: text.into() },
            None => ContentBlock::Unknown { raw: v.clone() },
        },
        "tool_use" | "tool_call" | "function" => {
            let call = v.get("function").unwrap_or(v);
            ContentBlock::ToolCall {
                id: string(v, "id"),
                name: string(call, "name").unwrap_or_else(|| "unknown".into()),
                input: arguments(
                    call.get("input")
                        .or_else(|| call.get("arguments"))
                        .unwrap_or(&Value::Null),
                ),
            }
        }
        "tool_result" => ContentBlock::ToolResult {
            id: string(v, "tool_use_id")
                .or_else(|| string(v, "tool_call_id"))
                .or_else(|| string(v, "id")),
            content: content_text(&v["content"]),
            is_error: v["is_error"]
                .as_bool()
                .or_else(|| v["isError"].as_bool())
                .unwrap_or(false),
        },
        "command_output" => ContentBlock::ToolResult {
            id: string(v, "id"),
            content: content_text(&v["output"]),
            is_error: false,
        },
        _ => ContentBlock::Unknown { raw: v.clone() },
    };
    vec![block]
}
pub(crate) fn message(
    id: String,
    role: MessageRole,
    parts: Vec<ContentBlock>,
) -> NormalizedMessage {
    let role = if !parts.is_empty()
        && parts
            .iter()
            .all(|b| matches!(b, ContentBlock::ToolResult { .. }))
    {
        MessageRole::Tool
    } else {
        role
    };
    NormalizedMessage {
        external_id: id,
        parent_id: None,
        role,
        created_at: None,
        model: None,
        blocks: parts,
        usage: None,
        metadata: HashMap::new(),
    }
}
pub(crate) fn standard_message(v: &Value, index: usize) -> NormalizedMessage {
    let role = MessageRole::from_str(v["role"].as_str().unwrap_or("unknown"));
    let mut parts = blocks(
        v.get("content")
            .or_else(|| v.get("parts"))
            .unwrap_or(&Value::Null),
    );
    if let Some(calls) = v.get("tool_calls").and_then(Value::as_array) {
        parts.extend(calls.iter().flat_map(blocks));
    }
    if role == MessageRole::Tool {
        parts = vec![ContentBlock::ToolResult {
            id: string(v, "tool_call_id").or_else(|| string(v, "toolCallId")),
            content: content_text(&v["content"]),
            is_error: v["is_error"].as_bool().unwrap_or(false),
        }];
    }
    let mut m = message(
        string(v, "id")
            .or_else(|| string(v, "uuid"))
            .unwrap_or_else(|| format!("message-{index}")),
        role,
        parts,
    );
    m.created_at = v.get("timestamp").and_then(timestamp);
    m.model = string(v, "model");
    m
}
pub(crate) fn session(
    source: SourceKind,
    id: String,
    path: &Path,
    layout: &str,
) -> NormalizedSession {
    let mut metadata = HashMap::new();
    metadata.insert("storage_layout".into(), Value::String(layout.into()));
    NormalizedSession {
        source,
        external_session_id: id,
        title: None,
        project_name: None,
        project_path: None,
        source_path: Some(path.into()),
        started_at: None,
        updated_at: None,
        model: None,
        messages: Vec::new(),
        usage: None,
        metadata,
    }
}
pub(crate) fn complete(s: &mut NormalizedSession) {
    if s.title.is_none() {
        s.title = s
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .flat_map(|m| &m.blocks)
            .find_map(|b| match b {
                ContentBlock::Text { text } if !text.trim().is_empty() => {
                    Some(text.trim().chars().take(120).collect())
                }
                _ => None,
            });
    }
    if s.project_name.is_none() {
        s.project_name = s
            .project_path
            .as_ref()
            .and_then(|p| p.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next())
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
    }
    if s.started_at.is_none() {
        s.started_at = s.messages.iter().filter_map(|m| m.created_at).min();
    }
    if s.updated_at.is_none() {
        s.updated_at = s.messages.iter().filter_map(|m| m.created_at).max();
    }
}
pub(crate) fn summary(s: &NormalizedSession) -> SessionSummary {
    SessionSummary {
        source: s.source,
        external_session_id: s.external_session_id.clone(),
        title: s.title.clone(),
        project_name: s.project_name.clone(),
        project_path: s.project_path.clone(),
        source_path: s.source_path.clone(),
        started_at: s.started_at,
        updated_at: s.updated_at,
        message_count: s.messages.len(),
    }
}
pub(crate) fn scoped_id(path: &Path, upstream: &str) -> String {
    let hash = hex::encode(Sha256::digest(path.to_string_lossy().as_bytes()));
    format!("{}:{upstream}", &hash[..32])
}
pub(crate) fn user_query(text: &str) -> String {
    if let Some((_, tail)) = text.split_once("<user_query>") {
        if let Some((body, _)) = tail.split_once("</user_query>") {
            return body.trim().into();
        }
    }
    text.into()
}
