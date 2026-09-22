use crate::model::{ContentBlock, MessageRole, NormalizedSession, SourceKind};
use crate::providers::{local_io::ScopedReader, message_parts::*};
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::path::Path;

fn target<'a>(value: &'a mut Value, path: &[Value], growth: &mut usize) -> Result<&'a mut Value> {
    ensure!(path.len() <= 64, "VS Code patch path depth exceeded");
    let mut value = value;
    for segment in path {
        if value.is_null() {
            *value = if segment.is_string() {
                json!({})
            } else {
                json!([])
            };
        }
        match segment {
            Value::String(key) => {
                ensure!(key.len() <= 1024, "VS Code patch key too long");
                value = value
                    .as_object_mut()
                    .context("VS Code patch expects object")?
                    .entry(key.clone())
                    .or_insert(Value::Null);
            }
            Value::Number(n) => {
                let index = n.as_u64().context("VS Code patch index invalid")? as usize;
                ensure!(index < 100_000, "VS Code patch index exceeds budget");
                let array = value
                    .as_array_mut()
                    .context("VS Code patch expects array")?;
                if array.len() <= index {
                    *growth += index + 1 - array.len();
                    ensure!(*growth <= 100_000, "VS Code patch growth budget exceeded");
                    array.resize(index + 1, Value::Null);
                }
                value = &mut array[index];
            }
            _ => bail!("unsupported VS Code patch segment"),
        }
    }
    Ok(value)
}
fn state(io: &ScopedReader, relative: &Path, metadata_only: bool) -> Result<(Value, bool)> {
    if relative.extension().and_then(|v| v.to_str()) == Some("json") {
        return Ok((io.read_json(relative)?, true));
    }
    let mut snapshot = None;
    let mut growth = 0_usize;
    let report = io.for_each_jsonl(relative, |_, event| {
        let kind = event["kind"]
            .as_u64()
            .context("VS Code patch kind missing")?;
        if kind == 0 {
            snapshot = Some(event.get("v").context("VS Code snapshot missing")?.clone());
            return Ok(());
        }
        let state = snapshot
            .as_mut()
            .context("VS Code log requires initial snapshot")?;
        let path = event["k"]
            .as_array()
            .context("VS Code patch path missing")?;
        match kind {
            1 => {
                *target(state, path, &mut growth)? =
                    event.get("v").context("VS Code set value missing")?.clone();
            }
            2 => {
                let value = target(state, path, &mut growth)?;
                if value.is_null() {
                    *value = json!([]);
                }
                let array = value
                    .as_array_mut()
                    .context("VS Code append target is not array")?;

                // Current VS Code mutation logs may emit a Push entry with only
                // `i`, meaning "truncate from this index", and no appended values.
                if let Some(index) = event.get("i").and_then(Value::as_u64) {
                    let index = usize::try_from(index)
                        .context("VS Code append index is too large")?;
                    ensure!(index < 100_000, "VS Code append index exceeds budget");
                    if index < array.len() {
                        array.truncate(index);
                    } else if index > array.len() {
                        growth += index - array.len();
                        ensure!(growth <= 100_000, "VS Code patch growth budget exceeded");
                        array.resize(index, Value::Null);
                    }
                }

                if let Some(items) = event.get("v") {
                    let items = items
                        .as_array()
                        .context("VS Code append values must be an array")?;
                    growth += items.len();
                    ensure!(growth <= 100_000, "VS Code patch growth budget exceeded");
                    array.extend(items.iter().cloned());
                }
            }
            3 => {
                let (last, parents) = path
                    .split_last()
                    .context("VS Code cannot delete root snapshot")?;
                let parent = target(state, parents, &mut growth)?;
                match (parent, last) {
                    (Value::Object(map), Value::String(key)) => {
                        map.remove(key);
                    }
                    (Value::Array(array), Value::Number(n)) => {
                        let i = n.as_u64().context("invalid deletion index")? as usize;
                        if i < array.len() {
                            array.remove(i);
                        }
                    }
                    _ => bail!("invalid VS Code deletion target"),
                }
            }
            _ => bail!("unsupported VS Code patch; previous import retained"),
        }
        Ok(())
    })?;
    let snapshot = snapshot.context("VS Code snapshot missing")?;
    let safe_live_tail = metadata_only
        && report.partial_tail
        && report.malformed_lines == 0
        && !report.source_changed;
    Ok((snapshot, report.complete || safe_live_tail))
}
pub(crate) fn read(
    io: &ScopedReader,
    relative: &Path,
    metadata_only: bool,
) -> Result<Vec<NormalizedSession>> {
    let (value, complete_read) = state(io, relative, metadata_only)?;
    let upstream = string(&value, "sessionId").context("VS Code session ID missing")?;
    let workspace = relative
        .parent()
        .and_then(Path::parent)
        .context("VS Code workspace directory missing")?;
    let mut s = session(
        SourceKind::GithubCopilot,
        scoped_id(&io.checked_path(workspace)?, &upstream),
        &io.checked_path(relative)?,
        "copilot-vscode-chat",
    );
    s.metadata
        .insert("upstream_session_id".into(), upstream.into());
    s.metadata
        .insert("entrypoint".into(), "copilot-vscode".into());
    s.title = string(&value, "customTitle").or_else(|| string(&value, "title"));
    s.started_at = value.get("creationDate").and_then(timestamp);
    let folder = workspace.join("workspace.json");
    if io.exists(&folder)? {
        s.project_path = string(&io.read_json(&folder)?, "folder")
            .map(|s| s.strip_prefix("file://").unwrap_or(&s).into());
    }
    let requests = value["requests"]
        .as_array()
        .context("VS Code requests array missing")?;
    if !metadata_only {
        for (index, req) in requests.iter().enumerate() {
            let user = &req["message"];
            let parts = if let Some(text) = string(user, "text") {
                vec![ContentBlock::Text { text }]
            } else {
                user.get("parts")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .map(|p| {
                                if p["kind"].as_str() == Some("text") {
                                    ContentBlock::Text {
                                        text: p["text"].as_str().unwrap_or("").into(),
                                    }
                                } else {
                                    ContentBlock::Unknown { raw: p.clone() }
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };
            if !parts.is_empty() {
                let mut m = message(
                    string(req, "requestId").unwrap_or_else(|| format!("vscode-request-{index}")),
                    MessageRole::User,
                    parts,
                );
                m.created_at = req.get("timestamp").and_then(timestamp);
                s.messages.push(m);
            }
            let Some(response) = req.get("response").and_then(Value::as_array) else {
                continue;
            };
            let mut parts = Vec::new();
            for part in response {
                match part["kind"].as_str() {
                    None if part["value"].is_string() => parts.push(ContentBlock::Text {
                        text: part["value"].as_str().unwrap().into(),
                    }),
                    Some("markdownContent" | "progressTaskSerialized")
                        if part["content"]["value"].is_string() =>
                    {
                        parts.push(ContentBlock::Text {
                            text: part["content"]["value"].as_str().unwrap().into(),
                        })
                    }
                    Some("thinking") if part["value"].is_string() => {
                        parts.push(ContentBlock::Thinking {
                            text: part["value"].as_str().unwrap().into(),
                        })
                    }
                    Some("toolInvocationSerialized") => {
                        let id = string(part, "toolCallId");
                        parts.push(ContentBlock::ToolCall {
                            id: id.clone(),
                            name: string(part, "toolId").unwrap_or_else(|| "unknown".into()),
                            input: json!({"invocationMessage":part["invocationMessage"]}),
                        });
                        if part["isComplete"].as_bool() == Some(true) {
                            if let Some(text) = string(&part["pastTenseMessage"], "value") {
                                parts.push(ContentBlock::ToolResult {
                                    id,
                                    content: text,
                                    is_error: part["isError"].as_bool().unwrap_or(false),
                                });
                            }
                        }
                    }
                    _ => parts.push(ContentBlock::Unknown { raw: part.clone() }),
                }
            }
            if !parts.is_empty() {
                let mut m = message(
                    string(req, "responseId").unwrap_or_else(|| format!("vscode-response-{index}")),
                    MessageRole::Assistant,
                    parts,
                );
                m.model = string(req, "modelId");
                s.messages.push(m);
            }
        }
    }
    complete(&mut s);
    Ok((vec![s], complete_read))
}
