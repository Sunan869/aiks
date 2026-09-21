use crate::model::{ContentBlock, MessageRole, NormalizedSession, SourceKind};
use crate::providers::{local_io::ScopedReader, message_parts::*};
use anyhow::{ensure, Context, Result};
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use std::path::Path;

fn row_text(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<String> {
    row.get::<_, String>(index).or_else(|_| {
        row.get::<_, Vec<u8>>(index).and_then(|b| {
            String::from_utf8(b).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Blob,
                    Box::new(e),
                )
            })
        })
    })
}
fn decode(raw: &str) -> Result<Value> {
    let mut value: Value =
        serde_json::from_str(raw).map_err(|_| anyhow::anyhow!("invalid Cursor JSON"))?;
    for _ in 0..2 {
        if let Some(nested) = value.as_str() {
            value = serde_json::from_str(nested)
                .map_err(|_| anyhow::anyhow!("invalid nested Cursor JSON"))?;
        } else {
            return Ok(value);
        }
    }
    ensure!(!value.is_string(), "Cursor JSON nesting limit exceeded");
    Ok(value)
}
fn has_table(conn: &Connection, table: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false))
}
fn exact(conn: &Connection, table: &str, key: &str) -> Result<Option<Value>> {
    ensure!(
        matches!(table, "cursorDiskKV" | "ItemTable"),
        "invalid Cursor table"
    );
    let query = format!("SELECT length(value), CASE WHEN length(value)<=8388608 THEN value ELSE NULL END FROM {table} WHERE key=?1");
    let row: Option<(u64, Option<String>)> = conn
        .query_row(&query, [key], |r| {
            Ok((
                r.get(0)?,
                if r.get::<_, Option<i64>>(0)?.is_some() && r.get::<_, u64>(0)? <= 8_388_608 {
                    Some(row_text(r, 1)?)
                } else {
                    None
                },
            ))
        })
        .optional()?;
    row.map(|(size, raw)| {
        ensure!(size <= 8_388_608, "Cursor record byte budget exceeded");
        decode(raw.as_deref().context("Cursor record missing")?)
    })
    .transpose()
}
fn bubble(v: &Value, typ: Option<u64>, index: usize) -> crate::model::NormalizedMessage {
    let role = match v["type"].as_u64().or(typ) {
        Some(1) => MessageRole::User,
        Some(2) => MessageRole::Assistant,
        _ => MessageRole::Unknown,
    };
    let mut parts = Vec::new();
    let text = string(v, "text");
    if let Some(tool) = v.get("toolFormerData").filter(|v| v.is_object()) {
        let id = string(tool, "toolCallId");
        let raw = tool.get("rawArgs").cloned().unwrap_or(Value::Null);
        let input = raw
            .as_str()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(raw);
        parts.push(ContentBlock::ToolCall {
            id: id.clone(),
            name: string(tool, "name").unwrap_or_else(|| "unknown".into()),
            input,
        });
        if let Some(text) = text {
            parts.push(ContentBlock::ToolResult {
                id,
                content: text,
                is_error: matches!(tool["status"].as_str(), Some("error" | "rejected")),
            });
        }
    } else if let Some(text) = text {
        parts.push(if v["isThought"].as_bool() == Some(true) {
            ContentBlock::Thinking { text }
        } else {
            ContentBlock::Text { text }
        });
    }
    if let Some(images) = v.get("images").and_then(Value::as_array) {
        for image in images {
            parts.push(ContentBlock::Unknown {
                raw: serde_json::json!({"type":"image_reference","reference":image}),
            });
        }
    }
    if parts.is_empty() {
        parts.push(ContentBlock::Unknown { raw: v.clone() });
    }
    let mut m = message(
        string(v, "bubbleId").unwrap_or_else(|| format!("cursor-bubble-{index}")),
        role,
        parts,
    );
    m.created_at = v
        .get("createdAt")
        .or_else(|| v.get("timestamp"))
        .and_then(timestamp);
    m.model = string(v, "model");
    m
}
fn composer(
    io: &ScopedReader,
    relative: &Path,
    conn: &Connection,
    upstream: &str,
    v: &Value,
    metadata_only: bool,
) -> Result<NormalizedSession> {
    let mut s = session(
        SourceKind::Cursor,
        scoped_id(io.root(), upstream),
        &io.checked_path(relative)?,
        "cursor-composer-sqlite",
    );
    s.metadata
        .insert("upstream_session_id".into(), upstream.into());
    if v["isArchived"].as_bool() == Some(true) {
        s.metadata.insert("archived".into(), Value::Bool(true));
    }
    s.title = string(v, "name");
    s.started_at = v.get("createdAt").and_then(timestamp);
    s.updated_at = v.get("lastUpdatedAt").and_then(timestamp);
    s.project_path = string(&v["workspaceIdentifier"]["uri"], "fsPath")
        .or_else(|| string(&v["workspaceIdentifier"]["uri"], "path"));
    let workspace = relative
        .parent()
        .unwrap_or(Path::new(""))
        .join("workspace.json");
    if s.project_path.is_none()
        && relative.starts_with("workspaceStorage")
        && io.exists(&workspace)?
    {
        s.project_path = string(&io.read_json(&workspace)?, "folder")
            .map(|s| s.strip_prefix("file://").unwrap_or(&s).to_owned());
    }
    if !metadata_only {
        if let Some(messages) = v.get("conversation").and_then(Value::as_array) {
            s.messages = messages
                .iter()
                .enumerate()
                .map(|(i, b)| bubble(b, None, i))
                .collect();
        } else {
            let headers = v
                .get("fullConversationHeadersOnly")
                .and_then(Value::as_array)
                .context("Cursor composer lacks readable conversation headers")?;
            let global;
            let connection = if has_table(conn, "cursorDiskKV")? {
                conn
            } else {
                global = io.open_readonly(Path::new("globalStorage/state.vscdb"))?;
                &global
            };
            for (i, header) in headers.iter().enumerate() {
                let bid = string(header, "bubbleId")
                    .context("Cursor conversation header lacks bubble ID")?;
                let b = exact(
                    connection,
                    "cursorDiskKV",
                    &format!("bubbleId:{upstream}:{bid}"),
                )?
                .context("Cursor referenced bubble missing; previous import retained")?;
                s.messages.push(bubble(&b, header["type"].as_u64(), i));
            }
        }
    }
    complete(&mut s);
    Ok(s)
}
/// The bool is false when any composer failed; valid neighbors remain available.
pub(crate) fn read(
    io: &ScopedReader,
    relative: &Path,
    metadata_only: bool,
    expected: Option<&str>,
) -> Result<(Vec<NormalizedSession>, bool)> {
    let conn = io.open_readonly(relative)?;
    let mut result = Vec::new();
    let mut complete = true;
    if has_table(&conn, "cursorDiskKV")? {
        let mut statement = conn.prepare("SELECT key, length(value), CASE WHEN length(value)<=8388608 THEN value ELSE NULL END FROM cursorDiskKV WHERE key>='composerData:' AND key<'composerData;' ORDER BY key LIMIT 100001")?;
        let mut rows = statement.query([])?;
        let mut count = 0_usize;
        let mut total = 0_u64;
        while let Some(row) = rows.next()? {
            count += 1;
            ensure!(
                count <= io.limits().max_entries,
                "Cursor candidate budget exceeded"
            );
            let key: String = row.get(0)?;
            let upstream = key
                .strip_prefix("composerData:")
                .context("invalid Cursor composer key")?;
            if expected.is_some_and(|id| id != scoped_id(io.root(), upstream)) {
                continue;
            }
            let size: u64 = row.get(1)?;
            total += size;
            ensure!(
                total <= io.limits().max_file_bytes,
                "Cursor scan byte budget exceeded"
            );
            let parsed = if size > 8_388_608 {
                Err(anyhow::anyhow!("Cursor record too large"))
            } else {
                row_text(row, 2)
                    .map_err(Into::into)
                    .and_then(|raw| decode(&raw))
                    .and_then(|v| composer(io, relative, &conn, upstream, &v, metadata_only))
            };
            match parsed {
                Ok(s) => result.push(s),
                Err(e) if expected.is_some() => return Err(e),
                Err(_) => complete = false,
            }
        }
    } else if has_table(&conn, "ItemTable")? {
        if let Some(index) = exact(&conn, "ItemTable", "composer.composerData")? {
            let composers = index
                .get("allComposers")
                .and_then(Value::as_array)
                .context("Cursor workspace composer list unsupported")?;
            ensure!(
                composers.len() <= io.limits().max_entries,
                "Cursor workspace candidate budget exceeded"
            );
            for v in composers {
                let Some(upstream) = string(v, "composerId") else {
                    complete = false;
                    continue;
                };
                if expected.is_some_and(|id| id != scoped_id(io.root(), &upstream)) {
                    continue;
                }
                match composer(io, relative, &conn, &upstream, v, metadata_only) {
                    Ok(s) => result.push(s),
                    Err(e) if expected.is_some() => return Err(e),
                    Err(_) => complete = false,
                }
            }
        }
    } else {
        anyhow::bail!("unsupported Cursor database schema");
    }
    Ok((result, complete))
}
