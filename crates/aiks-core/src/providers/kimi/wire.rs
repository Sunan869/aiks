//! Replay the recorded Kimi context vocabulary, not the viewer's token-stat summaries.
//! Format references: MoonshotAI/kimi-code contextOps.ts and compactionHandoff.ts
//! at 6a214b85e53e58a9ef6480f27bcb7b0103c0e34e (see THIRD_PARTY_NOTICES).
use std::collections::HashSet;
use std::path::Path;
use anyhow::{bail, ensure, Result};
use serde_json::{json, Value};
use crate::model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession};
use crate::providers::{local_io::ScopedReader, message_parts::*};

fn origin(m: &NormalizedMessage) -> Option<&str> { m.metadata.get("origin").and_then(|v| v.get("kind")).and_then(Value::as_str) }
fn anchor(m: &NormalizedMessage) -> bool {
    if m.role != MessageRole::User { return false; }
    match origin(m) {
        None | Some("user") => true,
        Some("skill_activation" | "plugin_command") => m.metadata["origin"]["trigger"].as_str() == Some("user-slash"),
        _ => false,
    }
}
#[derive(Default)]
struct Fold {
    history: Vec<NormalizedMessage>,
    open: Option<usize>,
    pending: HashSet<String>,
    deferred: Vec<NormalizedMessage>,
    model: Option<String>,
}
impl Fold {
    fn close(&mut self) {
        if let Some(index) = self.open.take() {
            if let Some(m) = self.history.get_mut(index) {
                if !self.pending.is_empty() {
                    let mut ids: Vec<_> = self.pending.iter().cloned().collect(); ids.sort();
                    m.metadata.insert("interrupted_tool_calls".into(), json!(ids));
                }
                if m.blocks.is_empty() { self.history.remove(index); }
            }
        }
        self.pending.clear();
        self.history.append(&mut self.deferred);
    }
    fn undo(&mut self, count: usize) {
        if count == 0 { return; }
        let mut remaining = count;
        let mut cut = None;
        for i in (0..self.history.len()).rev() {
            let m = &self.history[i];
            if origin(m) == Some("compaction_summary") { return; }
            if anchor(m) {
                remaining -= 1;
                let mut start = i;
                while start > 0 && origin(&self.history[start - 1]) == Some("injection") && self.history[start - 1].metadata["origin"]["ownerPromptId"].as_str() == Some(&m.external_id) { start -= 1; }
                cut = Some(start);
                if remaining == 0 { break; }
            }
        }
        // Current official replay requires all requested anchors to be undoable.
        if remaining == 0 {
            if let Some(cut) = cut { self.history.truncate(cut); self.open = None; self.pending.clear(); self.deferred.clear(); }
        }
    }
    fn compact(&mut self, record: &Value, index: usize) -> Result<()> {
        let count = record.get("compactedCount").or_else(|| record.get("count")).and_then(Value::as_u64).ok_or_else(|| anyhow::anyhow!("Kimi compaction boundary missing"))? as usize;
        ensure!(count <= self.history.len(), "Kimi compaction boundary exceeds recorded history");
        let summary = record.get("contextSummary").or_else(|| record.get("summary")).ok_or_else(|| anyhow::anyhow!("Kimi compaction summary missing"))?;
        let mut m = if summary.is_object() { standard_message(summary, index) }
            else { message(format!("kimi-compaction-{index}"), MessageRole::User, blocks(summary)) };
        ensure!(!m.blocks.is_empty(), "Kimi compaction summary has no recorded content");
        m.created_at = record.get("time").and_then(timestamp);
        m.metadata.insert("origin".into(), json!({"kind":"compaction_summary"}));
        let kept = record.get("keptUserMessageCount").and_then(Value::as_u64);
        let legacy = record.get("legacyTail").and_then(Value::as_bool).unwrap_or(kept.is_none());
        if legacy {
            let tail = self.history.split_off(count);
            self.history = vec![m]; self.history.extend(tail);
        } else {
            let users: Vec<_> = self.history.iter().filter(|m| anchor(m)).cloned().collect();
            let kept = kept.ok_or_else(|| anyhow::anyhow!("Kimi kept prompt count missing"))? as usize;
            let head = record["keptHeadUserMessageCount"].as_u64().unwrap_or(0) as usize;
            ensure!(head <= kept && kept <= users.len(), "Kimi compaction prompt selection invalid");
            let mut selected = users[..head].to_vec();
            selected.extend_from_slice(&users[users.len() - (kept - head)..]);
            // Preserve actual recorded prompt text, not an invented tokenizer truncation.
            // This is an archival view, explicitly distinct from the model's token-budget copy.
            for user in &mut selected { user.metadata.insert("compaction_original_user_text".into(), Value::Bool(true)); }
            selected.push(m); self.history = selected;
        }
        self.open = None; self.pending.clear(); self.deferred.clear();
        Ok(())
    }
}
pub(crate) fn replay(io: &ScopedReader, relative: &Path, session: &mut NormalizedSession) -> Result<()> {
    let mut f = Fold::default();
    let report = io.for_each_jsonl(relative, |index, record| {
        let kind = record["type"].as_str().ok_or_else(|| anyhow::anyhow!("Kimi wire event type missing"))?;
        let stamp = record.get("time").and_then(timestamp);
        match kind {
            "profile.bind" | "llm.request" => { if let Some(model) = string(&record, "modelAlias").or_else(|| string(&record, "model")) { f.model = Some(model); } }
            "context.clear" => { f.history.clear(); f.open = None; f.pending.clear(); f.deferred.clear(); }
            "context.undo" => f.undo(record["count"].as_u64().unwrap_or(1) as usize),
            "context.apply_compaction" => f.compact(&record, index)?,
            "context.append_message" => {
                let body = record.get("message").ok_or_else(|| anyhow::anyhow!("Kimi append missing message"))?;
                let mut m = standard_message(body, index);
                m.created_at = stamp;
                if let Some(origin) = body.get("origin") { m.metadata.insert("origin".into(), origin.clone()); }
                if let Some(calls) = body.get("toolCalls").and_then(Value::as_array) { m.blocks.extend(calls.iter().flat_map(blocks)); }
                if f.pending.is_empty() { f.history.push(m); } else { f.deferred.push(m); }
            }
            "context.append_loop_event" => {
                let event = record.get("event").ok_or_else(|| anyhow::anyhow!("Kimi loop event missing"))?;
                match event["type"].as_str().unwrap_or("") {
                    "step.begin" => {
                        f.close();
                        let mut m = message(format!("kimi-step-{index}"), MessageRole::Assistant, Vec::new());
                        m.created_at = stamp; m.model = f.model.clone();
                        f.open = Some(f.history.len()); f.history.push(m);
                    }
                    "step.end" => f.close(),
                    "content.part" => {
                        let open = f.open.ok_or_else(|| anyhow::anyhow!("Kimi content outside recorded step"))?;
                        f.history[open].blocks.extend(blocks(&event["part"]));
                    }
                    "tool.call" => {
                        let id = string(event, "toolCallId").ok_or_else(|| anyhow::anyhow!("Kimi tool call ID missing"))?;
                        let open = f.open.ok_or_else(|| anyhow::anyhow!("Kimi tool call outside recorded step"))?;
                        ensure!(f.pending.insert(id.clone()), "duplicate pending Kimi tool call");
                        f.history[open].blocks.push(ContentBlock::ToolCall { id: Some(id), name: string(event, "name").unwrap_or_else(|| "unknown".into()), input: event.get("args").cloned().unwrap_or(Value::Null) });
                    }
                    "tool.result" => {
                        let id = string(event, "toolCallId").ok_or_else(|| anyhow::anyhow!("Kimi tool result ID missing"))?;
                        let known = f.pending.remove(&id);
                        let result = &event["result"];
                        let mut m = message(format!("kimi-result-{index}"), MessageRole::Tool, vec![ContentBlock::ToolResult { id: Some(id), content: content_text(&result["output"]), is_error: result["isError"].as_bool().unwrap_or(false) }]);
                        m.created_at = stamp;
                        if !known { m.metadata.insert("orphan_tool_result".into(), Value::Bool(true)); }
                        f.history.push(m);
                        if f.pending.is_empty() { f.history.append(&mut f.deferred); }
                    }
                    _ => bail!("unsupported Kimi context loop event; previous import retained"),
                }
            }
            _ if kind.starts_with("context.") => bail!("unsupported Kimi context mutation; previous import retained"),
            _ => {} // Operational records do not form user/assistant turns.
        }
        Ok(())
    })?;
    ensure!(report.complete, "Kimi wire read incomplete; retry later");
    if f.open.is_some() { session.metadata.insert("open_step".into(), Value::Bool(true)); }
    f.close(); session.messages = f.history;
    Ok(())
}
