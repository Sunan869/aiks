//! Thin, best-effort bridge from accepted AIKS snapshots to WeKnora manual knowledge.
use crate::team::config::ConfigIssue;
use aiks_core::{config::ContentConfig, model::NormalizedSession, renderer::MarkdownRenderer};
use reqwest::{Client, Url};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_TITLE_CHARS: usize = 200;

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WeKnoraSettings {
    pub enabled: bool,
    pub base_url: String,
    pub knowledge_base_id: String,
    pub api_key_env: String,
    pub channel: String,
}

impl Default for WeKnoraSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            knowledge_base_id: String::new(),
            api_key_env: String::new(),
            channel: "aiks".into(),
        }
    }
}

#[derive(Clone)]
pub struct WeKnoraSync {
    inner: Arc<WeKnoraInner>,
}

struct WeKnoraInner {
    client: Client,
    base_url: Url,
    knowledge_base_id: String,
    api_key: String,
    channel: String,
    database: PathBuf,
    serial: Mutex<()>,
}

#[derive(Serialize)]
struct ManualKnowledgeRequest<'a> {
    title: &'a str,
    content: &'a str,
    status: &'static str,
    channel: &'a str,
}

#[derive(Deserialize)]
struct ApiResponse {
    success: bool,
    data: Option<KnowledgeResponse>,
}

#[derive(Deserialize)]
struct KnowledgeResponse {
    id: String,
}

struct Mapping {
    revision: u32,
    knowledge_id: String,
    content_hash: String,
}

pub fn check_settings_with(
    settings: &WeKnoraSettings,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<(), ConfigIssue> {
    if !settings.enabled {
        return Ok(());
    }
    let issue = |field, code| ConfigIssue { field, code };
    let url = Url::parse(settings.base_url.trim())
        .map_err(|_| issue("weknora.base_url", "invalid_origin"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err(issue("weknora.base_url", "invalid_origin"));
    }
    if !valid_id(&settings.knowledge_base_id) {
        return Err(issue(
            "weknora.knowledge_base_id",
            "invalid_knowledge_base_id",
        ));
    }
    if !valid_env_name(&settings.api_key_env) {
        return Err(issue(
            "weknora.api_key_env",
            "invalid_environment_reference",
        ));
    }
    if settings.channel.is_empty()
        || settings.channel.len() > 64
        || !settings
            .channel
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err(issue("weknora.channel", "invalid_channel"));
    }
    let key = lookup(&settings.api_key_env)
        .ok_or_else(|| issue("weknora.api_key_env", "secret_missing"))?;
    if key.is_empty()
        || key.len() > 8192
        || !key.bytes().all(|b| (0x21..=0x7e).contains(&b))
    {
        return Err(issue("weknora.api_key_env", "secret_invalid"));
    }
    Ok(())
}

impl WeKnoraSync {
    pub fn build_with(
        settings: &WeKnoraSettings,
        database: PathBuf,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> anyhow::Result<Option<Self>> {
        if !settings.enabled {
            return Ok(None);
        }
        check_settings_with(settings, &lookup)?;
        let api_key = lookup(&settings.api_key_env)
            .ok_or_else(|| anyhow::anyhow!("WeKnora API key is unavailable"))?;
        let base_url = Url::parse(settings.base_url.trim())?;
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()?;
        initialize_mapping_table(&database)?;
        Ok(Some(Self {
            inner: Arc::new(WeKnoraInner {
                client,
                base_url,
                knowledge_base_id: settings.knowledge_base_id.clone(),
                api_key,
                channel: settings.channel.clone(),
                database,
                serial: Mutex::new(()),
            }),
        }))
    }

    pub fn schedule(&self, session: NormalizedSession, revision: u32) {
        let sync = self.clone();
        tokio::spawn(async move {
            if let Err(error) = sync.sync_session(&session, revision).await {
                tracing::warn!(
                    source = session.source.as_str(),
                    external_session_id = %session.external_session_id,
                    error = %error,
                    "WeKnora session sync deferred"
                );
            }
        });
    }

    pub async fn sync_session(
        &self,
        session: &NormalizedSession,
        revision: u32,
    ) -> anyhow::Result<()> {
        let _serial = self.inner.serial.lock().await;
        let source = session.source.as_str().to_owned();
        let external_session_id = session.external_session_id.clone();
        let markdown = MarkdownRenderer::new(ContentConfig::default(), true).render(session);
        let content_hash = hex::encode(Sha256::digest(markdown.as_bytes()));
        let title = markdown_title(&markdown);
        let existing = load_mapping(
            self.inner.database.clone(),
            source.clone(),
            external_session_id.clone(),
        )
        .await?;

        if let Some(existing) = existing.as_ref() {
            if existing.revision >= revision {
                return Ok(());
            }
            if existing.content_hash == content_hash {
                store_mapping(
                    self.inner.database.clone(),
                    source,
                    external_session_id,
                    revision,
                    existing.knowledge_id.clone(),
                    content_hash,
                )
                .await?;
                return Ok(());
            }
        }

        let request = ManualKnowledgeRequest {
            title: &title,
            content: &markdown,
            status: "publish",
            channel: &self.inner.channel,
        };
        let knowledge_id = if let Some(existing) = existing {
            self.update(&existing.knowledge_id, &request).await?;
            existing.knowledge_id
        } else {
            self.create(&request).await?
        };

        store_mapping(
            self.inner.database.clone(),
            source,
            external_session_id,
            revision,
            knowledge_id,
            content_hash,
        )
        .await?;
        Ok(())
    }

    async fn create(&self, request: &ManualKnowledgeRequest<'_>) -> anyhow::Result<String> {
        let mut url = self.inner.base_url.clone();
        url.set_path(&format!(
            "/api/v1/knowledge-bases/{}/knowledge/manual",
            self.inner.knowledge_base_id
        ));
        let response = self
            .inner
            .client
            .post(url)
            .header("X-API-Key", &self.inner.api_key)
            .json(request)
            .send()
            .await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        anyhow::ensure!(
            bytes.len() <= MAX_RESPONSE_BYTES,
            "WeKnora response exceeds budget"
        );
        anyhow::ensure!(status.is_success(), "WeKnora create request failed");
        let envelope: ApiResponse = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(envelope.success, "WeKnora create request was rejected");
        let id = envelope
            .data
            .map(|value| value.id)
            .filter(|value| valid_id(value))
            .ok_or_else(|| anyhow::anyhow!("WeKnora create response is invalid"))?;
        Ok(id)
    }

    async fn update(
        &self,
        knowledge_id: &str,
        request: &ManualKnowledgeRequest<'_>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(valid_id(knowledge_id), "Invalid mapped WeKnora knowledge id");
        let mut url = self.inner.base_url.clone();
        url.set_path(&format!("/api/v1/knowledge/manual/{knowledge_id}"));
        let response = self
            .inner
            .client
            .put(url)
            .header("X-API-Key", &self.inner.api_key)
            .json(request)
            .send()
            .await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        anyhow::ensure!(
            bytes.len() <= MAX_RESPONSE_BYTES,
            "WeKnora response exceeds budget"
        );
        anyhow::ensure!(status.is_success(), "WeKnora update request failed");
        if !bytes.is_empty() {
            let envelope: ApiResponse = serde_json::from_slice(&bytes)?;
            anyhow::ensure!(envelope.success, "WeKnora update request was rejected");
        }
        Ok(())
    }
}

fn initialize_mapping_table(database: &PathBuf) -> anyhow::Result<()> {
    let conn = Connection::open(database)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS aiks_weknora_session_sync (
            source TEXT NOT NULL,
            external_session_id TEXT NOT NULL,
            revision INTEGER NOT NULL CHECK(revision >= 0),
            knowledge_id TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
            PRIMARY KEY(source, external_session_id)
        );",
    )?;
    Ok(())
}

async fn load_mapping(
    database: PathBuf,
    source: String,
    external_session_id: String,
) -> anyhow::Result<Option<Mapping>> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let conn = Connection::open(database)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let row = conn
            .query_row(
                "SELECT revision, knowledge_id, content_hash
                 FROM aiks_weknora_session_sync
                 WHERE source=?1 AND external_session_id=?2",
                params![source, external_session_id],
                |row| {
                    Ok(Mapping {
                        revision: row.get(0)?,
                        knowledge_id: row.get(1)?,
                        content_hash: row.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    })
    .await?
}

async fn store_mapping(
    database: PathBuf,
    source: String,
    external_session_id: String,
    revision: u32,
    knowledge_id: String,
    content_hash: String,
) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = Connection::open(database)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute(
            "INSERT INTO aiks_weknora_session_sync(
                source, external_session_id, revision, knowledge_id, content_hash, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(source, external_session_id) DO UPDATE SET
                revision=excluded.revision,
                knowledge_id=excluded.knowledge_id,
                content_hash=excluded.content_hash,
                updated_at=excluded.updated_at
             WHERE excluded.revision > aiks_weknora_session_sync.revision",
            params![
                source,
                external_session_id,
                revision,
                knowledge_id,
                content_hash,
                unix_now()?
            ],
        )?;
        Ok(())
    })
    .await?
}

fn markdown_title(markdown: &str) -> String {
    let candidate = markdown
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("# "))
        .unwrap_or("AI Session");
    let mut title: String = candidate.chars().take(MAX_TITLE_CHARS).collect();
    if title.trim().is_empty() {
        title = "AI Session".into();
    }
    title
}

fn valid_env_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn unix_now() -> anyhow::Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs())
}
