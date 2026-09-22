use super::{local_io::ScopedReader, message_parts::*};
use crate::model::{NormalizedSession, SourceKind};
use anyhow::{ensure, Result};
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) fn read(
    io: &ScopedReader,
    relative: &Path,
    metadata_only: bool,
) -> Result<Vec<NormalizedSession>> {
    let path = io.checked_path(relative)?;
    let mut sessions = BTreeMap::<String, NormalizedSession>::new();
    let report = io.for_each_jsonl(relative, |index, event| {
        let id = string(&event, "sessionId")
            .ok_or_else(|| anyhow::anyhow!("Qwen record missing sessionId"))?;
        let s = sessions
            .entry(id.clone())
            .or_insert_with(|| session(SourceKind::QwenCode, id, &path, "qwen-jsonl"));
        if let Some(cwd) = string(&event, "cwd") {
            s.project_path = Some(cwd);
        }
        if let Some(title) = string(&event, "title") {
            s.title = Some(title);
        }
        if let Some(model) = string(&event, "model") {
            s.model = Some(model);
        }
        if let Some(stamp) = event.get("timestamp").and_then(timestamp) {
            s.started_at = Some(s.started_at.map_or(stamp, |old| old.min(stamp)));
            s.updated_at = Some(s.updated_at.map_or(stamp, |old| old.max(stamp)));
        }
        let Some(body) = event.get("message") else {
            return Ok(());
        };
        if !metadata_only || s.title.is_none() {
            let mut m = standard_message(body, index);
            m.external_id = string(&event, "uuid").unwrap_or_else(|| format!("qwen-line-{index}"));
            m.parent_id = string(&event, "parentUuid");
            m.created_at = event.get("timestamp").and_then(timestamp);
            m.model = string(&event, "model");
            if !m.blocks.is_empty() {
                s.messages.push(m);
                if metadata_only {
                    complete(s);
                    s.messages.clear();
                }
            }
        }
        Ok(())
    })?;
    ensure!(
        report.complete,
        "Qwen transcript is incomplete; previous import must be retained"
    );
    ensure!(
        !sessions.is_empty(),
        "Qwen transcript has no session identity"
    );
    Ok(sessions
        .into_values()
        .map(|mut s| {
            complete(&mut s);
            s
        })
        .collect())
}
