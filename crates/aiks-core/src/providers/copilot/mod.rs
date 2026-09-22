pub(crate) mod vscode;
use crate::model::{ContentBlock, MessageRole, NormalizedSession, SourceKind};
use crate::providers::{local_io::ScopedReader, message_parts::*};
use anyhow::{ensure, Result};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

pub(crate) fn read(
    io: &ScopedReader,
    relative: &Path,
    metadata_only: bool,
) -> Result<Vec<NormalizedSession>> {
    if relative.starts_with("workspaceStorage") {
        return vscode::read(io, relative, metadata_only);
    }
    let dir = relative
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Copilot session directory missing"))?;
    let upstream = dir
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| anyhow::anyhow!("Copilot session ID missing"))?;
    let mut s = session(
        SourceKind::GithubCopilot,
        scoped_id(&io.checked_path(dir)?, upstream),
        &io.checked_path(relative)?,
        "copilot-events",
    );
    s.metadata
        .insert("upstream_session_id".into(), upstream.into());
    let mut entrypoint = "copilot-cli";
    let metadata = dir.join("workspace.yaml");
    if io.exists(&metadata)? {
        let raw = io.read_text(&metadata)?;
        ensure!(
            raw.len() <= 1024 * 1024,
            "Copilot workspace metadata byte budget exceeded"
        );
        for line in raw.lines() {
            if let Some(value) = line.strip_prefix("client_name:") {
                let name = value.trim().trim_matches(['\'', '"']);
                entrypoint = match name {
                    "github/autopilot" => "copilot-desktop",
                    "github/cli" => "copilot-cli",
                    _ => "copilot-unknown-client",
                };
            }
        }
    }
    s.metadata.insert("entrypoint".into(), entrypoint.into());
    let mut calls = HashSet::new();
    let report = io.for_each_jsonl(relative, |index, event| {
        let kind = event["type"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Copilot event type missing"))?;
        let data = &event["data"];
        let stamp = event.get("timestamp").and_then(timestamp);
        if let Some(stamp) = stamp {
            s.started_at = Some(s.started_at.map_or(stamp, |old| old.min(stamp)));
            s.updated_at = Some(s.updated_at.map_or(stamp, |old| old.max(stamp)));
        }
        match kind {
            "session.start" | "session.resume" => {
                if let Some(cwd) = string(&data["context"], "cwd") {
                    s.project_path = Some(cwd);
                }
            }
            "session.title_changed" => s.title = string(data, "title"),
            "session.model_change" => s.model = string(data, "newModel"),
            _ => {}
        }
        if metadata_only {
            return Ok(());
        }
        let (role, parts) = match kind {
            "user.message" => (MessageRole::User, blocks(&data["content"])),
            "system.message" => (MessageRole::System, blocks(&data["content"])),
            "assistant.message" => {
                let mut parts = blocks(&data["content"]);
                if let Some(requests) = data.get("toolRequests").and_then(Value::as_array) {
                    for call in requests {
                        let id = string(call, "toolCallId");
                        if let Some(id) = &id {
                            calls.insert(id.clone());
                        }
                        let raw = call.get("arguments").cloned().unwrap_or(Value::Null);
                        let input = raw
                            .as_str()
                            .and_then(|s| serde_json::from_str(s).ok())
                            .unwrap_or(raw);
                        parts.push(ContentBlock::ToolCall {
                            id,
                            name: string(call, "name").unwrap_or_else(|| "unknown".into()),
                            input,
                        });
                    }
                }
                (MessageRole::Assistant, parts)
            }
            "tool.execution_complete" => {
                let result = data.get("result").unwrap_or(&Value::Null);
                let content = result.get("content").unwrap_or(result);
                let id = string(data, "toolCallId");
                (
                    MessageRole::Tool,
                    vec![ContentBlock::ToolResult {
                        id,
                        content: content_text(content),
                        is_error: data["success"].as_bool() == Some(false)
                            || data.get("error").is_some_and(|e| !e.is_null()),
                    }],
                )
            }
            "tool.execution_start" => return Ok(()),
            _ => return Ok(()),
        };
        if !parts.is_empty() {
            let mut m = message(
                string(&event, "id").unwrap_or_else(|| format!("copilot-event-{index}")),
                role,
                parts,
            );
            m.created_at = stamp;
            m.model = string(data, "model");
            if role == MessageRole::Tool {
                let known = string(data, "toolCallId").is_some_and(|id| calls.remove(&id));
                if !known {
                    m.metadata
                        .insert("orphan_tool_result".into(), Value::Bool(true));
                }
            }
            s.messages.push(m);
        }
        Ok(())
    })?;
    ensure!(report.complete, "Copilot events incomplete; retry later");
    complete(&mut s);
    Ok(vec![s])
}
