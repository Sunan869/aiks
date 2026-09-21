//! Cline-family readers: conversation files are authoritative, indexes only metadata.
use super::{local_io::ScopedReader, message_parts::*};
use crate::model::{ContentBlock, MessageRole, NormalizedSession, SourceKind};
use anyhow::{ensure, Result};
use rusqlite::OptionalExtension;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(crate) fn extension(source: SourceKind) -> &'static str {
    match source {
        SourceKind::Cline => "saoudrizwan.claude-dev",
        SourceKind::RooCode => "rooveterinaryinc.roo-cline",
        SourceKind::KiloCode => "kilocode.kilo-code",
        _ => "",
    }
}
pub(crate) fn bases(io: &ScopedReader, source: SourceKind) -> Result<Vec<PathBuf>> {
    if io.exists(Path::new("tasks"))? {
        return Ok(vec![PathBuf::new()]);
    }
    let base = PathBuf::from("globalStorage").join(extension(source));
    if io.exists(&base)? {
        return Ok(vec![base]);
    }
    Ok(Vec::new())
}
fn entries(value: &Value) -> Option<&Vec<Value>> {
    value
        .as_array()
        .or_else(|| value.get("entries").and_then(Value::as_array))
        .or_else(|| value.get("taskHistory").and_then(Value::as_array))
}
fn task_metadata(
    io: &ScopedReader,
    base: &Path,
    id: &str,
    source: SourceKind,
) -> Result<Option<Value>> {
    for file in [
        base.join("state/taskHistory.json"),
        base.join("tasks/_index.json"),
    ] {
        if io.exists(&file)? {
            let value = io.read_json(&file)?;
            if let Some(item) = entries(&value)
                .and_then(|items| items.iter().find(|item| item["id"].as_str() == Some(id)))
            {
                return Ok(Some(item.clone()));
            }
        }
    }
    // An explicit extension root does not authorize its sibling editor database.
    if base.starts_with("globalStorage") && io.exists(Path::new("globalStorage/state.vscdb"))? {
        let conn = io.open_readonly(Path::new("globalStorage/state.vscdb"))?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM ItemTable WHERE key=?1 AND length(value)<=8388608",
                [extension(source)],
                |row| {
                    row.get::<_, String>(0).or_else(|_| {
                        row.get::<_, Vec<u8>>(0).and_then(|b| {
                            String::from_utf8(b).map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    0,
                                    rusqlite::types::Type::Blob,
                                    Box::new(e),
                                )
                            })
                        })
                    })
                },
            )
            .optional()?;
        if let Some(raw) = raw {
            let value: Value = serde_json::from_str(&raw)
                .map_err(|_| anyhow::anyhow!("invalid extension task index"))?;
            if let Some(item) = entries(&value)
                .and_then(|items| items.iter().find(|item| item["id"].as_str() == Some(id)))
            {
                return Ok(Some(item.clone()));
            }
        }
    }
    let file = base.join("tasks").join(id).join("task_metadata.json");
    if io.exists(&file)? {
        return Ok(Some(io.read_json(&file)?));
    }
    Ok(None)
}
pub(crate) fn read(
    io: &ScopedReader,
    relative: &Path,
    source: SourceKind,
    metadata_only: bool,
) -> Result<Vec<NormalizedSession>> {
    let task_dir = relative
        .parent()
        .ok_or_else(|| anyhow::anyhow!("task directory missing"))?;
    let id = task_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("task ID missing"))?;
    let base = task_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or(Path::new(""));
    let path = io.checked_path(relative)?;
    let mut s = session(
        source,
        scoped_id(&io.checked_path(task_dir)?, id),
        &path,
        "cline-family-task",
    );
    s.metadata.insert("upstream_session_id".into(), id.into());
    if let Some(meta) = task_metadata(io, base, id, source)? {
        s.title = string(&meta, "task").or_else(|| string(&meta, "title"));
        s.project_path = string(&meta, "cwdOnTaskInitialization")
            .or_else(|| string(&meta, "workspace"))
            .or_else(|| string(&meta, "cwd"));
        s.started_at = meta.get("ts").and_then(timestamp);
    }
    s.updated_at = std::fs::metadata(&path)?.modified().ok().map(Into::into);
    if metadata_only {
        complete(&mut s);
        return Ok(vec![s]);
    }
    let value = io.read_json(relative)?;
    let history = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("task history must be an array"))?;
    let api_history =
        relative.file_name().and_then(|n| n.to_str()) == Some("api_conversation_history.json");
    for (index, item) in history.iter().enumerate() {
        if api_history {
            ensure!(
                item.get("role").and_then(Value::as_str).is_some(),
                "task message role missing"
            );
            let mut msg = standard_message(item, index);
            if msg.blocks.is_empty() {
                msg.blocks.push(ContentBlock::Unknown { raw: item.clone() });
            }
            s.messages.push(msg);
            continue;
        }
        ensure!(
            item["partial"].as_bool() != Some(true),
            "task UI reply is still partial; retry later"
        );
        let kind = item["type"].as_str().unwrap_or("");
        let tag = item.get(kind).and_then(Value::as_str).unwrap_or("");
        let text = item["text"].as_str().unwrap_or("");
        let (role, parts) = match (kind, tag) {
            ("say", "user_feedback") => (MessageRole::User, blocks(&item["text"])),
            ("say", "text" | "completion_result") if index == 0 => {
                (MessageRole::User, blocks(&item["text"]))
            }
            ("say", "text" | "completion_result") | ("ask", "followup" | "completion_result") => {
                (MessageRole::Assistant, blocks(&item["text"]))
            }
            ("say", "reasoning") => (
                MessageRole::Assistant,
                vec![ContentBlock::Thinking {
                    text: item["reasoning"].as_str().unwrap_or(text).into(),
                }],
            ),
            (
                "say",
                "api_req_started" | "api_req_finished" | "api_req_retried" | "deleted_api_reqs",
            ) => continue,
            _ => (
                MessageRole::Unknown,
                vec![ContentBlock::Unknown { raw: item.clone() }],
            ),
        };
        if !parts.is_empty() {
            let mut msg = message(format!("task-ui-{index}"), role, parts);
            msg.created_at = item.get("ts").and_then(timestamp);
            s.messages.push(msg);
        }
    }
    complete(&mut s);
    Ok(vec![s])
}
