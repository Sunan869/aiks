use super::{local_io::ScopedReader, message_parts::*};
use crate::model::{ContentBlock, MessageRole, NormalizedSession, SourceKind};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

pub(crate) fn read(
    io: &ScopedReader,
    relative: &Path,
    metadata_only: bool,
) -> Result<Vec<NormalizedSession>> {
    let upstream = relative
        .components()
        .nth(1)
        .and_then(|c| c.as_os_str().to_str())
        .context("Antigravity conversation ID missing")?;
    let path = io.checked_path(relative)?;
    let mut s = session(
        SourceKind::Antigravity,
        scoped_id(io.root(), upstream),
        &path,
        "antigravity-cli-steps",
    );
    s.metadata
        .insert("upstream_session_id".into(), upstream.into());
    let mut seen = HashSet::new();
    let mut recognized = 0;
    let report = io.for_each_jsonl(relative, |line, event| {
        let typ = event["type"]
            .as_str()
            .context("Antigravity step type missing")?;
        let source = event["source"]
            .as_str()
            .context("Antigravity step source missing")?;
        let id = event["step_index"]
            .as_u64()
            .map(|i| format!("antigravity-step-{i}"))
            .unwrap_or_else(|| format!("antigravity-line-{line}"));
        ensure!(
            seen.insert(id.clone()),
            "duplicate Antigravity step identity; ambiguous snapshot"
        );
        if let Some(id) = string(&event, "conversationId") {
            ensure!(id == upstream, "Antigravity mixed conversation identities");
        }
        if let Some(cwd) = string(&event, "cwd").or_else(|| string(&event, "workspace")) {
            s.project_path = Some(cwd);
        }
        if matches!(
            typ,
            "USER_INPUT" | "PLANNER_RESPONSE" | "CONVERSATION_HISTORY"
        ) {
            recognized += 1;
        }
        let role = match (source, typ) {
            ("USER_EXPLICIT", "USER_INPUT") => MessageRole::User,
            ("MODEL", "PLANNER_RESPONSE") => MessageRole::Assistant,
            ("SYSTEM", _) => MessageRole::System,
            _ => MessageRole::Unknown,
        };
        if metadata_only && s.title.is_some() {
            return Ok(());
        }
        let parts = if role == MessageRole::Unknown {
            vec![ContentBlock::Unknown { raw: event.clone() }]
        } else if let Some(content) = event.get("content") {
            blocks(content)
        } else {
            Vec::new()
        };
        if !parts.is_empty() {
            let mut m = message(id, role, parts);
            m.created_at = event.get("created_at").and_then(timestamp);
            m.metadata.insert("event_type".into(), typ.into());
            s.messages.push(m);
            if metadata_only {
                complete(&mut s);
                s.messages.clear();
            }
        }
        Ok(())
    })?;
    ensure!(
        report.complete,
        "Antigravity transcript incomplete; retry later"
    );
    ensure!(
        recognized > 0,
        "unsupported Antigravity transcript schema; no verified message steps"
    );
    complete(&mut s);
    Ok(vec![s])
}
