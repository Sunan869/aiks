pub(crate) mod wire;
use std::path::Path;
use anyhow::{ensure, Result};
use serde_json::Value;
use crate::model::{NormalizedSession, SourceKind};
use super::{local_io::ScopedReader, message_parts::*};

pub(crate) fn read(io: &ScopedReader, relative: &Path, metadata_only: bool) -> Result<Vec<NormalizedSession>> {
    let path = io.checked_path(relative)?;
    let modern = relative.ends_with("agents/main/wire.jsonl");
    let dir = if modern { relative.parent().and_then(Path::parent).and_then(Path::parent) } else { relative.parent() }.ok_or_else(|| anyhow::anyhow!("Kimi session directory missing"))?;
    let mut meta = Value::Null;
    let state = dir.join("state.json");
    if io.exists(&state)? { meta = io.read_json(&state)?; }
    let upstream = string(&meta, "id").or_else(|| dir.file_name().and_then(|n| n.to_str()).map(str::to_owned)).ok_or_else(|| anyhow::anyhow!("Kimi session ID missing"))?;
    let mut s = session(SourceKind::KimiCode, scoped_id(&io.checked_path(dir)?, &upstream), &path, if modern { "kimi-code-wire-v2" } else { "kimi-legacy" });
    s.metadata.insert("upstream_session_id".into(), upstream.into());
    s.title = string(&meta, "title");
    s.project_path = string(&meta, "cwd").or_else(|| string(&meta, "work_dir"));
    s.started_at = meta.get("createdAt").and_then(timestamp);
    s.updated_at = meta.get("updatedAt").and_then(timestamp);
    if !metadata_only {
        if modern { wire::replay(io, relative, &mut s)?; }
        else {
            let report = io.for_each_jsonl(relative, |index, item| {
                let Some(role) = item.get("role").and_then(Value::as_str) else {
                    ensure!(item.get("_type").is_some() || item.get("type").is_some(), "unknown Kimi context record");
                    return Ok(());
                };
                if role.starts_with('_') { return Ok(()); }
                let m = standard_message(&item, index);
                if !m.blocks.is_empty() { s.messages.push(m); }
                Ok(())
            })?;
            ensure!(report.complete, "Kimi context read incomplete; retry later");
        }
    }
    complete(&mut s);
    Ok(vec![s])
}
