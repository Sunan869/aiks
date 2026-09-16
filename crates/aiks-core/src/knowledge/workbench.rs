use chrono::Utc;
use rusqlite::{params, params_from_iter, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::StateDb;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateKnowledgeInput {
    pub title: String,
    pub category: Option<String>,
    pub project_name: Option<String>,
    pub summary: Option<String>,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateKnowledgeInput {
    pub title: String,
    pub category: String,
    pub project_name: Option<String>,
    pub summary: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnowledgeListFilter {
    pub project: Option<String>,
    pub category: Option<String>,
    pub source_type: Option<String>,
    pub status: Option<String>,
    pub favorite: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRecord {
    pub id: String,
    pub source_session_id: Option<i64>,
    pub project_name: Option<String>,
    pub title: String,
    pub category: String,
    pub summary: String,
    pub content: String,
    pub tags: Vec<String>,
    pub confidence: f64,
    pub source_type: String,
    pub managed_by: String,
    pub status: String,
    pub is_favorite: bool,
    pub created_at: String,
    pub updated_at: String,
    pub source: Option<String>,
    pub session_external_id: Option<String>,
    pub session_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeListResult {
    pub items: Vec<KnowledgeRecord>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

pub struct KnowledgeService<'a> {
    db: &'a StateDb,
}

impl<'a> KnowledgeService<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    pub fn create_manual(&self, input: CreateKnowledgeInput) -> anyhow::Result<KnowledgeRecord> {
        let title = required_text("title", &input.title)?;
        let content = required_text("content", &input.content)?;
        let category = normalize_category(input.category.as_deref().unwrap_or("general"));
        let project_name = normalize_optional(input.project_name.as_deref());
        let summary = input.summary.unwrap_or_default().trim().to_string();
        let tags = normalize_tags(input.tags);
        let tags_json = serde_json::to_string(&tags)?;
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        let conn = self.db.conn();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result: anyhow::Result<()> = (|| {
            conn.execute(
                "INSERT INTO knowledge_item
                 (id, source_session_id, project_name, title, category, summary, content,
                  tags, confidence, worth_extracting, source_type, managed_by, status,
                  is_favorite, created_at, updated_at)
                 VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, 1.0, 1,
                         'manual', 'user', 'active', 0, ?8, ?8)",
                params![
                    id,
                    project_name,
                    title,
                    category,
                    summary,
                    content,
                    tags_json,
                    now
                ],
            )?;
            upsert_fts(&conn, &id, &title, &summary, &content, &tags_json)?;
            Ok(())
        })();
        match result {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(e);
            }
        }
        drop(conn);

        self.get(&id)?
            .ok_or_else(|| anyhow::anyhow!("new knowledge item disappeared: {id}"))
    }

    pub fn update(&self, id: &str, input: UpdateKnowledgeInput) -> anyhow::Result<KnowledgeRecord> {
        let title = required_text("title", &input.title)?;
        let content = required_text("content", &input.content)?;
        let category = normalize_category(&input.category);
        let project_name = normalize_optional(input.project_name.as_deref());
        let summary = input.summary.trim().to_string();
        let tags = normalize_tags(input.tags);
        let tags_json = serde_json::to_string(&tags)?;
        let now = Utc::now().to_rfc3339();

        let conn = self.db.conn();
        let old_content: Option<String> = conn
            .query_row(
                "SELECT content FROM knowledge_item WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(old_content) = old_content else {
            return Err(anyhow::anyhow!("Knowledge item not found: {id}"));
        };

        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result: anyhow::Result<()> = (|| {
            let changed = conn.execute(
                "UPDATE knowledge_item
                 SET project_name = ?2, title = ?3, category = ?4, summary = ?5,
                     content = ?6, tags = ?7, managed_by = 'user', updated_at = ?8
                 WHERE id = ?1",
                params![
                    id,
                    project_name,
                    title,
                    category,
                    summary,
                    content,
                    tags_json,
                    now
                ],
            )?;
            if changed == 0 {
                anyhow::bail!("Knowledge item not found: {id}");
            }

            upsert_fts(&conn, id, &title, &summary, &content, &tags_json)?;

            if old_content != content {
                conn.execute(
                    "DELETE FROM embedding_record
                     WHERE chunk_id IN (SELECT id FROM knowledge_chunk WHERE knowledge_id = ?1)",
                    params![id],
                )?;
                conn.execute(
                    "DELETE FROM knowledge_chunk WHERE knowledge_id = ?1",
                    params![id],
                )?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(e);
            }
        }
        drop(conn);

        self.get(id)?
            .ok_or_else(|| anyhow::anyhow!("Knowledge item not found after update: {id}"))
    }

    pub fn set_favorite(&self, id: &str, favorite: bool) -> anyhow::Result<KnowledgeRecord> {
        self.update_flag(id, "is_favorite", if favorite { 1 } else { 0 })
    }

    pub fn archive(&self, id: &str) -> anyhow::Result<KnowledgeRecord> {
        self.update_status(id, "archived")
    }

    pub fn restore(&self, id: &str) -> anyhow::Result<KnowledgeRecord> {
        self.update_status(id, "active")
    }

    pub fn get(&self, id: &str) -> anyhow::Result<Option<KnowledgeRecord>> {
        let conn = self.db.conn();
        let record = conn
            .query_row(
                "SELECT ki.id, ki.source_session_id, ki.project_name, ki.title, ki.category,
                        ki.summary, ki.content, ki.tags, ki.confidence, ki.source_type,
                        ki.managed_by, ki.status, ki.is_favorite, ki.created_at, ki.updated_at,
                        ss.source, ss.external_session_id, ss.title
                 FROM knowledge_item ki
                 LEFT JOIN source_session ss ON ss.id = ki.source_session_id
                 WHERE ki.id = ?1",
                params![id],
                row_to_record,
            )
            .optional()?;
        Ok(record)
    }

    pub fn list(&self, filter: KnowledgeListFilter) -> anyhow::Result<KnowledgeListResult> {
        let limit = filter.limit.unwrap_or(50).clamp(1, 200);
        let offset = filter.offset.unwrap_or(0);
        let mut where_parts: Vec<String> = Vec::new();
        let mut values: Vec<rusqlite::types::Value> = Vec::new();

        push_text_filter(
            &mut where_parts,
            &mut values,
            "ki.project_name",
            filter.project,
        );
        push_text_filter(
            &mut where_parts,
            &mut values,
            "ki.category",
            filter.category,
        );
        push_text_filter(
            &mut where_parts,
            &mut values,
            "ki.source_type",
            filter.source_type,
        );
        push_text_filter(&mut where_parts, &mut values, "ki.status", filter.status);
        if let Some(favorite) = filter.favorite {
            where_parts.push("ki.is_favorite = ?".to_string());
            values.push(rusqlite::types::Value::Integer(if favorite {
                1
            } else {
                0
            }));
        }

        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };

        let conn = self.db.conn();
        let sql = format!(
            "SELECT ki.id, ki.source_session_id, ki.project_name, ki.title, ki.category,
                    ki.summary, ki.content, ki.tags, ki.confidence, ki.source_type,
                    ki.managed_by, ki.status, ki.is_favorite, ki.created_at, ki.updated_at,
                    ss.source, ss.external_session_id, ss.title
             FROM knowledge_item ki
             LEFT JOIN source_session ss ON ss.id = ki.source_session_id
             {where_clause}
             ORDER BY ki.is_favorite DESC, ki.updated_at DESC
             LIMIT ? OFFSET ?"
        );
        let mut query_values = values.clone();
        query_values.push((limit as i64).into());
        query_values.push((offset as i64).into());
        let mut stmt = conn.prepare(&sql)?;
        let items = stmt
            .query_map(params_from_iter(query_values.iter()), row_to_record)?
            .collect::<Result<Vec<_>, _>>()?;

        let count_sql = format!("SELECT COUNT(*) FROM knowledge_item ki {where_clause}");
        let total: i64 = conn.query_row(&count_sql, params_from_iter(values.iter()), |row| {
            row.get(0)
        })?;

        Ok(KnowledgeListResult {
            items,
            total: total.max(0) as usize,
            limit,
            offset,
        })
    }

    fn update_status(&self, id: &str, status: &str) -> anyhow::Result<KnowledgeRecord> {
        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn();
        let changed = conn.execute(
            "UPDATE knowledge_item SET status = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, status, now],
        )?;
        if changed == 0 {
            anyhow::bail!("Knowledge item not found: {id}");
        }
        drop(conn);
        self.get(id)?
            .ok_or_else(|| anyhow::anyhow!("Knowledge item not found after status update: {id}"))
    }

    fn update_flag(&self, id: &str, column: &str, value: i64) -> anyhow::Result<KnowledgeRecord> {
        debug_assert_eq!(column, "is_favorite");
        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn();
        let changed = conn.execute(
            "UPDATE knowledge_item SET is_favorite = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, value, now],
        )?;
        if changed == 0 {
            anyhow::bail!("Knowledge item not found: {id}");
        }
        drop(conn);
        self.get(id)?
            .ok_or_else(|| anyhow::anyhow!("Knowledge item not found after favorite update: {id}"))
    }
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeRecord> {
    let tags_json: String = row.get(7)?;
    let tags = serde_json::from_str::<Vec<String>>(&tags_json).unwrap_or_default();
    Ok(KnowledgeRecord {
        id: row.get(0)?,
        source_session_id: row.get(1)?,
        project_name: row.get(2)?,
        title: row.get(3)?,
        category: row.get(4)?,
        summary: row.get(5)?,
        content: row.get(6)?,
        tags,
        confidence: row.get(8)?,
        source_type: row.get(9)?,
        managed_by: row.get(10)?,
        status: row.get(11)?,
        is_favorite: row.get::<_, i64>(12)? != 0,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
        source: row.get(15)?,
        session_external_id: row.get(16)?,
        session_title: row.get(17)?,
    })
}

fn required_text(field: &str, value: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("{field} is required");
    }
    Ok(trimmed.to_string())
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_category(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "troubleshooting" | "architecture" | "implementation" | "configuration" | "research"
        | "decision" | "general" => normalized,
        _ => "general".to_string(),
    }
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_lowercase();
        if seen.insert(key) {
            result.push(trimmed.to_string());
        }
    }
    result
}

fn push_text_filter(
    where_parts: &mut Vec<String>,
    values: &mut Vec<rusqlite::types::Value>,
    column: &str,
    value: Option<String>,
) {
    if let Some(value) = value {
        let value = value.trim().to_string();
        if !value.is_empty() {
            where_parts.push(format!("{column} = ?"));
            values.push(value.into());
        }
    }
}

fn upsert_fts(
    conn: &rusqlite::Connection,
    id: &str,
    title: &str,
    summary: &str,
    content: &str,
    tags_json: &str,
) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM knowledge_fts WHERE knowledge_id = ?1",
        params![id],
    )?;
    conn.execute(
        "INSERT INTO knowledge_fts (knowledge_id, title, summary, content, tags)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, title, summary, content, tags_json],
    )?;
    Ok(())
}
