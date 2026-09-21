use super::{local_io::ScopedReader, message_parts::*};
use crate::model::{ContentBlock, MessageRole, NormalizedSession, SourceKind};
use anyhow::{ensure, Result};
use std::collections::HashMap;
use std::path::Path;

const HEADER: &str = "# aider chat started at ";
fn flush(s: &mut NormalizedSession, role: MessageRole, text: &mut String) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        let id = format!("aider-message-{}", s.messages.len());
        let part = if role == MessageRole::Tool {
            ContentBlock::ToolResult {
                id: None,
                content: trimmed.into(),
                is_error: false,
            }
        } else {
            ContentBlock::Text {
                text: trimmed.into(),
            }
        };
        s.messages.push(message(id, role, vec![part]));
    }
    text.clear();
}
fn fence(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let c = trimmed.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let count = trimmed.chars().take_while(|v| *v == c).count();
    (count >= 3).then_some((c, count))
}
pub(crate) fn read(
    io: &ScopedReader,
    relative: &Path,
    metadata_only: bool,
) -> Result<Vec<NormalizedSession>> {
    let path = io.checked_path(relative)?;
    let text = io.read_text(relative)?;
    let mut output = Vec::new();
    let mut current: Option<NormalizedSession> = None;
    let mut occurrences = HashMap::<String, usize>::new();
    let mut open_fence: Option<(char, usize)> = None;
    let mut role = MessageRole::Unknown;
    let mut body = String::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let mark = fence(line);
        let outside = open_fence.is_none();
        if outside {
            if let Some(raw_time) = line.strip_prefix(HEADER) {
                if chrono::NaiveDateTime::parse_from_str(raw_time.trim(), "%Y-%m-%d %H:%M:%S")
                    .is_ok()
                {
                    if let Some(mut s) = current.take() {
                        flush(&mut s, role, &mut body);
                        complete(&mut s);
                        output.push(s);
                    }
                    let occurrence = occurrences.entry(raw_time.into()).or_default();
                    let id = scoped_id(
                        &path,
                        &format!("{}:{}:{}", raw_time.len(), raw_time, occurrence),
                    );
                    *occurrence += 1;
                    let mut s = session(SourceKind::Aider, id, &path, "aider-markdown");
                    s.metadata
                        .insert("local_session_time".into(), raw_time.into());
                    s.project_path = path.parent().map(|p| p.to_string_lossy().into_owned());
                    current = Some(s);
                    role = MessageRole::Unknown;
                    continue;
                }
            }
        }
        let Some(s) = current.as_mut() else {
            continue;
        };
        if outside {
            if let Some(prompt) = line.strip_prefix("#### ") {
                if role != MessageRole::User {
                    flush(s, role, &mut body);
                } else if !body.is_empty() {
                    body.push('\n');
                }
                role = MessageRole::User;
                body.push_str(prompt);
                continue;
            }
            if let Some(tool) = line.strip_prefix("> ") {
                if role != MessageRole::Tool {
                    flush(s, role, &mut body);
                }
                role = MessageRole::Tool;
                body.push_str(tool);
                body.push('\n');
                continue;
            }
            if role != MessageRole::Assistant {
                flush(s, role, &mut body);
                role = MessageRole::Assistant;
            }
        }
        body.push_str(line);
        body.push('\n');
        if let Some((c, count)) = mark {
            match open_fence {
                None => open_fence = Some((c, count)),
                Some((old, width))
                    if old == c && count >= width && line.trim().chars().all(|x| x == c) =>
                {
                    open_fence = None
                }
                _ => {}
            }
        }
    }
    if let Some(mut s) = current {
        flush(&mut s, role, &mut body);
        complete(&mut s);
        output.push(s);
    }
    ensure!(
        !output.is_empty(),
        "Aider history has no recognized session header"
    );
    if metadata_only {
        for s in &mut output {
            s.messages.clear();
        }
    }
    Ok(output)
}
