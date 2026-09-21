use std::path::Path;
use anyhow::{ensure, Result};
use crate::model::{ContentBlock, MessageRole, NormalizedSession, SourceKind};
use super::{local_io::ScopedReader, message_parts::*};

pub(crate) fn read(io: &ScopedReader, relative: &Path, metadata_only: bool) -> Result<Vec<NormalizedSession>> {
    let path = io.checked_path(relative)?;
    let upstream = relative.file_stem().and_then(|n| n.to_str()).ok_or_else(|| anyhow::anyhow!("Cursor Agent filename missing"))?;
    // File UUIDs are not guaranteed unique across project exports; namespace by transcript path.
    let mut s = session(SourceKind::CursorAgent, scoped_id(&path, upstream), &path, "cursor-agent-jsonl");
    s.metadata.insert("upstream_session_id".into(), upstream.into());
    if metadata_only {
        s.title = Some(upstream.into());
        s.updated_at = std::fs::metadata(&path)?.modified().ok().map(Into::into);
        return Ok(vec![s]);
    }
    let report = io.for_each_jsonl(relative, |index, event| {
        let body = event.get("message").unwrap_or(&event);
        let role = MessageRole::from_str(event.get("role").or_else(|| body.get("role")).and_then(|v| v.as_str()).unwrap_or("unknown"));
        let mut parts = blocks(&body["content"]);
        if role == MessageRole::User {
            for part in &mut parts { if let ContentBlock::Text { text } = part { *text = user_query(text); } }
        }
        if !parts.is_empty() {
            let mut m = message(format!("cursor-agent-line-{index}"), role, parts);
            m.parent_id = string(&event, "parentId");
            s.messages.push(m);
        }
        Ok(())
    })?;
    ensure!(report.complete, "Cursor Agent transcript incomplete; retaining previous import");
    ensure!(!s.messages.is_empty(), "Cursor Agent transcript contains no readable messages");
    complete(&mut s);
    Ok(vec![s])
}
