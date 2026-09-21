use std::path::Path;
use anyhow::{ensure, Result};
use serde_json::Value;
use crate::model::{ContentBlock, NormalizedSession, SourceKind};
use super::{local_io::ScopedReader, message_parts::*};

pub(crate) fn read(io: &ScopedReader, relative: &Path, metadata_only: bool) -> Result<Vec<NormalizedSession>> {
    let value = io.read_json(relative)?;
    let id = string(&value, "sessionId").ok_or_else(|| anyhow::anyhow!("Continue sessionId missing"))?;
    let history = value.get("history").and_then(Value::as_array).ok_or_else(|| anyhow::anyhow!("Continue history array missing"))?;
    let mut s = session(SourceKind::Continue, id, &io.checked_path(relative)?, "continue-json");
    s.title = string(&value, "title");
    s.project_path = string(&value, "workspaceDirectory");
    if !metadata_only {
        for (index, item) in history.iter().enumerate() {
            let body = item.get("message").ok_or_else(|| anyhow::anyhow!("Continue history entry missing message"))?;
            let mut m = standard_message(body, index);
            // Continue toolCallStates supplement assistant calls; contextItems are not messages.
            if let Some(states) = item.get("toolCallStates").and_then(Value::as_array) {
                for state in states {
                    if let Some(call) = state.get("toolCall") {
                        let id = string(call, "id");
                        let duplicate = m.blocks.iter().any(|b| matches!(b, ContentBlock::ToolCall { id: existing, .. } if id.is_some() && existing == &id));
                        if !duplicate { m.blocks.extend(blocks(call)); }
                    }
                }
            }
            ensure!(!m.blocks.is_empty(), "Continue message has no supported content; retaining previous import");
            s.messages.push(m);
        }
    }
    complete(&mut s);
    Ok(vec![s])
}
