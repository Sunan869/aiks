//! Read-only local session selection. No ServiceClient, HTTP, queue or models.
use super::{collector, ClientError, ClientResult};
use aiks_core::{
    providers::{SessionProvider, SessionSummary},
    service::{validate_submission, SnapshotSubmission},
    util::sanitizer::default_sanitizer,
    SourceKind,
};
use serde::Serialize;

struct Entry {
    key: String,
    summary: SessionSummary,
}
pub struct LocalSessionBrowser {
    source: SourceKind,
    entries: Vec<Entry>,
    complete: bool,
    discovered: usize,
}
#[derive(Serialize)]
pub struct SessionRow {
    pub key: String,
    pub title: String,
    pub project: Option<String>,
    pub updated_at: Option<String>,
    pub message_count: usize,
    pub excluded: bool,
}
#[derive(Serialize)]
pub struct SessionPage {
    pub items: Vec<SessionRow>,
    pub total: usize,
    pub discovered: usize,
    pub complete: bool,
    pub offset: usize,
    pub limit: usize,
}
#[derive(Serialize)]
pub struct PreviewMessage {
    pub role: String,
    pub text: String,
}
#[derive(Serialize)]
pub struct SessionPreview {
    pub title: String,
    pub messages: Vec<PreviewMessage>,
    pub truncated: bool,
}

impl LocalSessionBrowser {
    pub async fn scan(provider: &dyn SessionProvider) -> ClientResult<Self> {
        let report = provider
            .discover_report()
            .await
            .map_err(|_| ClientError::InvalidInput)?;
        let discovered = report.sessions.len();
        let mut sessions = report.sessions;
        sessions.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.external_session_id.cmp(&b.external_session_id))
        });
        let mut entries = Vec::new();
        let complete = report.complete && sessions.len() <= 10_000;
        for summary in sessions.into_iter().take(10_000) {
            if summary.source != provider.source() {
                return Err(ClientError::InvalidInput);
            }
            entries.push(Entry {
                key: uuid::Uuid::new_v4().to_string(),
                summary,
            });
        }
        Ok(Self {
            source: provider.source(),
            entries,
            complete,
            discovered,
        })
    }
    pub fn resolve(&self, key: &str) -> ClientResult<&SessionSummary> {
        self.entries
            .iter()
            .find(|e| e.key == key)
            .map(|e| &e.summary)
            .ok_or(ClientError::NotFound)
    }
    pub fn page(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
        excluded: &[String],
    ) -> ClientResult<SessionPage> {
        if query.len() > 4096 || offset > 10_000 || limit == 0 || limit > 100 {
            return Err(ClientError::InvalidInput);
        }
        let query = query.trim().to_lowercase();
        let selected: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|e| {
                let title = e.summary.title.as_deref().unwrap_or("");
                let project = e.summary.project_name.as_deref().unwrap_or("");
                query.is_empty()
                    || title.to_lowercase().contains(&query)
                    || project.to_lowercase().contains(&query)
            })
            .collect();
        let total = selected.len();
        let items = selected
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|entry| {
                let s = &entry.summary;
                SessionRow {
                    key: entry.key.clone(),
                    title: clean(
                        s.title
                            .as_deref()
                            .filter(|t| !t.trim().is_empty())
                            .unwrap_or("未命名会话"),
                        240,
                    ),
                    project: s.project_name.as_deref().map(|v| clean(v, 120)),
                    updated_at: s.updated_at.or(s.started_at).map(|v| v.to_rfc3339()),
                    message_count: s.message_count,
                    excluded: excluded.contains(&s.external_session_id),
                }
            })
            .collect();
        Ok(SessionPage {
            items,
            total,
            discovered: self.discovered,
            complete: self.complete,
            offset,
            limit,
        })
    }
    pub async fn preview(
        &self,
        provider: &dyn SessionProvider,
        key: &str,
    ) -> ClientResult<SessionPreview> {
        if provider.source() != self.source {
            return Err(ClientError::InvalidInput);
        }
        let summary = self.resolve(key)?;
        let mut session = collector::load_complete(provider, summary).await?;
        let input = SnapshotSubmission {
            api_version: 1,
            submission_id: "preview".into(),
            service_instance_id: "preview".into(),
            space_id: "preview".into(),
            source_registration_id: "preview".into(),
            expected_revision: 0,
            complete: true,
            parser_version: provider.parser_version().into(),
            session: session.clone(),
        };
        validate_submission(&input).map_err(|_| ClientError::InvalidInput)?;
        collector::sanitize(&mut session)?;
        let fallback = session
            .messages
            .iter()
            .flat_map(|m| &m.blocks)
            .find_map(|b| b.text_content())
            .unwrap_or("未命名会话");
        let title = clean(
            session
                .title
                .as_deref()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or(fallback),
            240,
        );
        let mut remaining = 64 * 1024;
        let mut messages = Vec::new();
        let mut truncated = false;
        for message in &session.messages {
            if messages.len() >= 200 || remaining == 0 {
                truncated = true;
                break;
            }
            let mut text = String::new();
            for block in &message.blocks {
                let Some(value) = block.text_content() else {
                    continue;
                };
                if !text.is_empty() && remaining > 0 {
                    text.push('\n');
                    remaining -= 1;
                }
                for ch in value.chars() {
                    if ch.len_utf8() > remaining {
                        truncated = true;
                        remaining = 0;
                        break;
                    }
                    text.push(ch);
                    remaining -= ch.len_utf8();
                }
                if remaining == 0 {
                    break;
                }
            }
            if !text.is_empty() {
                messages.push(PreviewMessage {
                    role: message.role.as_str().into(),
                    text,
                });
            }
        }
        Ok(SessionPreview {
            title,
            messages,
            truncated,
        })
    }
}
fn clean(value: &str, limit: usize) -> String {
    default_sanitizer()
        .sanitize(value)
        .chars()
        .take(limit)
        .collect()
}
