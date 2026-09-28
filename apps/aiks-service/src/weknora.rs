//! Durable, best-effort bridge from accepted AIKS snapshots to WeKnora manual knowledge.
use crate::team::config::ConfigIssue;
use aiks_core::{config::ContentConfig, model::NormalizedSession, renderer::MarkdownRenderer};
use reqwest::{Client, Method, StatusCode, Url};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, Notify};

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_TITLE_CHARS: usize = 200;
const MAX_DRAIN_BATCH: usize = 32;
const RETRY_POLL_SECONDS: u64 = 30;
const MAX_RETRY_SECONDS: u64 = 300;

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
            api_key_env: "AIKS_WEKNORA_API_KEY".into(),
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
    notify: Arc<Notify>,
    worker_started: AtomicBool,
}

#[derive(Clone)]
struct SyncIntent {
    source: String,
    external_session_id: String,
    external_id: String,
    revision: u32,
    title: String,
    markdown: String,
    content_hash: String,
    attempts: u32,
}

struct Mapping {
    revision: u32,
    knowledge_id: String,
    content_hash: String,
}

#[derive(Serialize)]
struct ManualKnowledgeRequest<'a> {
    title: &'a str,
    content: &'a str,
    status: &'static str,
    channel: &'a str,
    external_id: &'a str,
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

#[derive(Debug)]
struct RemoteError {
    code: &'static str,
    retryable: bool,
    not_found: bool,
}

impl RemoteError {
    const fn retry(code: &'static str) -> Self {
        Self {
            code,
            retryable: true,
            not_found: false,
        }
    }

    const fn permanent(code: &'static str) -> Self {
        Self {
            code,
            retryable: false,
            not_found: false,
        }
    }

    const fn not_found() -> Self {
        Self {
            code: "not_found",
            retryable: false,
            not_found: true,
        }
    }
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}

impl std::error::Error for RemoteError {}

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
        initialize_tables(&database)?;
        Ok(Some(Self {
            inner: Arc::new(WeKnoraInner {
                client,
                base_url,
                knowledge_base_id: settings.knowledge_base_id.clone(),
                api_key,
                channel: settings.channel.clone(),
                database,
                serial: Mutex::new(()),
                notify: Arc::new(Notify::new()),
                worker_started: AtomicBool::new(false),
            }),
        }))
    }

    /// Start one recovery worker for this adapter. It uses a Weak reference so
    /// dropping the service/router eventually stops the task without a detached
    /// lifetime owner.
    pub fn start_background(&self) {
        if self.inner.worker_started.swap(true, Ordering::AcqRel) {
            return;
        }
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            loop {
                let Some(inner) = weak.upgrade() else {
                    break;
                };
                let notify = inner.notify.clone();
                let sync = WeKnoraSync { inner };
                if let Err(error) = sync.process_due(MAX_DRAIN_BATCH).await {
                    tracing::warn!(error = %error, "WeKnora outbox drain failed");
                }
                drop(sync);
                tokio::select! {
                    _ = notify.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(RETRY_POLL_SECONDS)) => {}
                }
            }
        });
        self.inner.notify.notify_one();
    }

    /// Persist the sync intent before returning to the caller. Remote WeKnora
    /// availability is deliberately not part of the local snapshot acceptance.
    pub async fn enqueue_session(
        &self,
        session: &NormalizedSession,
        revision: u32,
    ) -> anyhow::Result<()> {
        let intent = intent_from_session(session, revision);
        enqueue_intent(self.inner.database.clone(), intent).await?;
        self.inner.notify.notify_one();
        Ok(())
    }

    /// Deterministic foreground sync used by contracts and maintenance tools.
    /// Production request handling uses enqueue_session + the background worker.
    pub async fn sync_session(
        &self,
        session: &NormalizedSession,
        revision: u32,
    ) -> anyhow::Result<()> {
        let intent = intent_from_session(session, revision);
        enqueue_intent(self.inner.database.clone(), intent.clone()).await?;
        let _serial = self.inner.serial.lock().await;
        self.process_intent(intent).await
    }

    /// Operational retry hook: clear retry delays for non-terminal rows and
    /// synchronously drain a bounded batch.
    pub async fn retry_pending_now(&self) -> anyhow::Result<usize> {
        reset_retry_delays(self.inner.database.clone()).await?;
        self.process_due(MAX_DRAIN_BATCH).await
    }

    pub async fn pending_count(&self) -> anyhow::Result<u64> {
        let (pending, terminal) = outbox_counts(self.inner.database.clone()).await?;
        Ok(pending.saturating_add(terminal))
    }

    pub async fn outbox_counts(&self) -> anyhow::Result<(u64, u64)> {
        outbox_counts(self.inner.database.clone()).await
    }

    async fn process_due(&self, limit: usize) -> anyhow::Result<usize> {
        let _serial = self.inner.serial.lock().await;
        let mut processed = 0;
        for _ in 0..limit {
            let Some(intent) = load_due_intent(self.inner.database.clone()).await? else {
                break;
            };
            match self.apply_intent(&intent).await {
                Ok(()) => {
                    finish_intent(self.inner.database.clone(), &intent).await?;
                    processed += 1;
                }
                Err(error) => {
                    record_failure(self.inner.database.clone(), &intent, &error).await?;
                    tracing::warn!(
                        source = %intent.source,
                        external_session_id = %intent.external_session_id,
                        error_code = error.code,
                        retryable = error.retryable,
                        "WeKnora session sync deferred"
                    );
                }
            }
        }
        Ok(processed)
    }

    async fn process_intent(&self, intent: SyncIntent) -> anyhow::Result<()> {
        match self.apply_intent(&intent).await {
            Ok(()) => {
                finish_intent(self.inner.database.clone(), &intent).await?;
                Ok(())
            }
            Err(error) => {
                record_failure(self.inner.database.clone(), &intent, &error).await?;
                Err(anyhow::Error::new(error))
            }
        }
    }

    async fn apply_intent(&self, intent: &SyncIntent) -> Result<(), RemoteError> {
        let existing = load_mapping(
            self.inner.database.clone(),
            intent.source.clone(),
            intent.external_session_id.clone(),
        )
        .await
        .map_err(|_| RemoteError::retry("mapping_read_failed"))?;

        if let Some(existing) = existing.as_ref() {
            if existing.revision >= intent.revision {
                return Ok(());
            }
            if existing.content_hash == intent.content_hash {
                store_mapping(
                    self.inner.database.clone(),
                    intent.source.clone(),
                    intent.external_session_id.clone(),
                    intent.revision,
                    existing.knowledge_id.clone(),
                    intent.content_hash.clone(),
                )
                .await
                .map_err(|_| RemoteError::retry("mapping_write_failed"))?;
                return Ok(());
            }
        }

        let request = ManualKnowledgeRequest {
            title: &intent.title,
            content: &intent.markdown,
            status: "publish",
            channel: &self.inner.channel,
            external_id: &intent.external_id,
        };

        let knowledge_id = if let Some(existing) = existing {
            match self.update(&existing.knowledge_id, &request).await {
                Ok(()) => existing.knowledge_id,
                Err(error) if error.not_found => self.create(&request).await?,
                Err(error) => return Err(error),
            }
        } else {
            self.create(&request).await?
        };

        store_mapping(
            self.inner.database.clone(),
            intent.source.clone(),
            intent.external_session_id.clone(),
            intent.revision,
            knowledge_id,
            intent.content_hash.clone(),
        )
        .await
        .map_err(|_| RemoteError::retry("mapping_write_failed"))?;
        Ok(())
    }

    async fn create(
        &self,
        request: &ManualKnowledgeRequest<'_>,
    ) -> Result<String, RemoteError> {
        let mut url = self.inner.base_url.clone();
        url.set_path(&format!(
            "/api/v1/knowledge-bases/{}/knowledge/manual",
            self.inner.knowledge_base_id
        ));
        let envelope = self.request_json(Method::POST, url, request, false).await?;
        if !envelope.success {
            return Err(RemoteError::retry("rejected_response"));
        }
        envelope
            .data
            .map(|value| value.id)
            .filter(|value| valid_id(value))
            .ok_or_else(|| RemoteError::retry("invalid_response"))
    }

    async fn update(
        &self,
        knowledge_id: &str,
        request: &ManualKnowledgeRequest<'_>,
    ) -> Result<(), RemoteError> {
        if !valid_id(knowledge_id) {
            return Err(RemoteError::permanent("invalid_mapped_knowledge_id"));
        }
        let mut url = self.inner.base_url.clone();
        url.set_path(&format!("/api/v1/knowledge/manual/{knowledge_id}"));
        let envelope = self.request_json(Method::PUT, url, request, true).await?;
        if !envelope.success {
            return Err(RemoteError::retry("rejected_response"));
        }
        Ok(())
    }

    async fn request_json(
        &self,
        method: Method,
        url: Url,
        request: &ManualKnowledgeRequest<'_>,
        allow_not_found: bool,
    ) -> Result<ApiResponse, RemoteError> {
        let response = self
            .inner
            .client
            .request(method, url)
            .header("X-API-Key", &self.inner.api_key)
            .json(request)
            .send()
            .await
            .map_err(|_| RemoteError::retry("transport"))?;

        let status = response.status();
        if allow_not_found && status == StatusCode::NOT_FOUND {
            return Err(RemoteError::not_found());
        }
        if !status.is_success() {
            return Err(status_error(status));
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
        {
            return Err(RemoteError::permanent("oversize_response"));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| RemoteError::retry("response_read"))?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(RemoteError::permanent("oversize_response"));
        }
        serde_json::from_slice(&bytes).map_err(|_| RemoteError::retry("invalid_response"))
    }
}

fn intent_from_session(session: &NormalizedSession, revision: u32) -> SyncIntent {
    let source = session.source.as_str().to_owned();
    let external_session_id = session.external_session_id.clone();
    let markdown = MarkdownRenderer::new(ContentConfig::default(), true).render(session);
    let content_hash = hex::encode(Sha256::digest(markdown.as_bytes()));
    let external_id = stable_external_id(&source, &external_session_id);
    SyncIntent {
        source,
        external_session_id,
        external_id,
        revision,
        title: markdown_title(&markdown),
        markdown,
        content_hash,
        attempts: 0,
    }
}

fn stable_external_id(source: &str, external_session_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    hasher.update([0]);
    hasher.update(external_session_id.as_bytes());
    format!("aiks-{}", hex::encode(hasher.finalize()))
}

fn status_error(status: StatusCode) -> RemoteError {
    match status {
        StatusCode::REQUEST_TIMEOUT => RemoteError::retry("request_timeout"),
        StatusCode::TOO_MANY_REQUESTS => RemoteError::retry("rate_limited"),
        StatusCode::UNAUTHORIZED => RemoteError::retry("unauthorized"),
        StatusCode::FORBIDDEN => RemoteError::retry("forbidden"),
        StatusCode::CONFLICT => RemoteError::retry("conflict"),
        status if status.is_server_error() => RemoteError::retry("upstream_5xx"),
        StatusCode::PAYLOAD_TOO_LARGE => RemoteError::permanent("payload_too_large"),
        StatusCode::UNPROCESSABLE_ENTITY => RemoteError::permanent("unprocessable"),
        _ => RemoteError::permanent("upstream_4xx"),
    }
}

fn initialize_tables(database: &PathBuf) -> anyhow::Result<()> {
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
        );
        CREATE TABLE IF NOT EXISTS aiks_weknora_outbox (
            source TEXT NOT NULL,
            external_session_id TEXT NOT NULL,
            external_id TEXT NOT NULL,
            revision INTEGER NOT NULL CHECK(revision >= 0),
            title TEXT NOT NULL,
            markdown TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts >= 0),
            next_attempt_at INTEGER NOT NULL DEFAULT 0 CHECK(next_attempt_at >= 0),
            terminal INTEGER NOT NULL DEFAULT 0 CHECK(terminal IN (0,1)),
            last_error_code TEXT,
            updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
            PRIMARY KEY(source, external_session_id)
        );
        CREATE INDEX IF NOT EXISTS idx_aiks_weknora_outbox_due
            ON aiks_weknora_outbox(terminal, next_attempt_at, updated_at);",
    )?;
    Ok(())
}

async fn enqueue_intent(database: PathBuf, intent: SyncIntent) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = Connection::open(database)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute(
            "INSERT INTO aiks_weknora_outbox(
                source, external_session_id, external_id, revision, title, markdown,
                content_hash, attempts, next_attempt_at, terminal, last_error_code, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,0,0,0,NULL,?8)
             ON CONFLICT(source, external_session_id) DO UPDATE SET
                external_id=excluded.external_id,
                revision=excluded.revision,
                title=excluded.title,
                markdown=excluded.markdown,
                content_hash=excluded.content_hash,
                attempts=0,
                next_attempt_at=0,
                terminal=0,
                last_error_code=NULL,
                updated_at=excluded.updated_at
             WHERE excluded.revision >= aiks_weknora_outbox.revision",
            params![
                intent.source,
                intent.external_session_id,
                intent.external_id,
                intent.revision,
                intent.title,
                intent.markdown,
                intent.content_hash,
                unix_now()?
            ],
        )?;
        Ok(())
    })
    .await?
}

async fn load_due_intent(database: PathBuf) -> anyhow::Result<Option<SyncIntent>> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let conn = Connection::open(database)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let at = unix_now()?;
        Ok(conn
            .query_row(
                "SELECT source, external_session_id, external_id, revision, title,
                        markdown, content_hash, attempts
                 FROM aiks_weknora_outbox
                 WHERE terminal=0 AND next_attempt_at <= ?1
                 ORDER BY next_attempt_at ASC, updated_at ASC
                 LIMIT 1",
                [at],
                |row| {
                    Ok(SyncIntent {
                        source: row.get(0)?,
                        external_session_id: row.get(1)?,
                        external_id: row.get(2)?,
                        revision: row.get(3)?,
                        title: row.get(4)?,
                        markdown: row.get(5)?,
                        content_hash: row.get(6)?,
                        attempts: row.get(7)?,
                    })
                },
            )
            .optional()?)
    })
    .await?
}

async fn load_mapping(
    database: PathBuf,
    source: String,
    external_session_id: String,
) -> anyhow::Result<Option<Mapping>> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let conn = Connection::open(database)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(conn
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
            .optional()?)
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
             WHERE excluded.revision >= aiks_weknora_session_sync.revision",
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

async fn finish_intent(database: PathBuf, intent: &SyncIntent) -> anyhow::Result<()> {
    let source = intent.source.clone();
    let external_session_id = intent.external_session_id.clone();
    let revision = intent.revision;
    let content_hash = intent.content_hash.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = Connection::open(database)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute(
            "DELETE FROM aiks_weknora_outbox
             WHERE source=?1 AND external_session_id=?2 AND revision=?3 AND content_hash=?4",
            params![source, external_session_id, revision, content_hash],
        )?;
        Ok(())
    })
    .await?
}

async fn record_failure(
    database: PathBuf,
    intent: &SyncIntent,
    error: &RemoteError,
) -> anyhow::Result<()> {
    let source = intent.source.clone();
    let external_session_id = intent.external_session_id.clone();
    let revision = intent.revision;
    let content_hash = intent.content_hash.clone();
    let attempts = intent.attempts.saturating_add(1);
    let error_code = error.code.to_owned();
    let retryable = error.retryable;
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = Connection::open(database)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let at = unix_now()?;
        let next_attempt_at = if retryable {
            at.saturating_add(retry_delay_seconds(attempts))
        } else {
            at
        };
        conn.execute(
            "UPDATE aiks_weknora_outbox
             SET attempts=?5, next_attempt_at=?6, terminal=?7, last_error_code=?8, updated_at=?9
             WHERE source=?1 AND external_session_id=?2 AND revision=?3 AND content_hash=?4",
            params![
                source,
                external_session_id,
                revision,
                content_hash,
                attempts,
                next_attempt_at,
                if retryable { 0 } else { 1 },
                error_code,
                at
            ],
        )?;
        Ok(())
    })
    .await?
}

async fn reset_retry_delays(database: PathBuf) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = Connection::open(database)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute(
            "UPDATE aiks_weknora_outbox SET next_attempt_at=0 WHERE terminal=0",
            [],
        )?;
        Ok(())
    })
    .await?
}

async fn outbox_counts(database: PathBuf) -> anyhow::Result<(u64, u64)> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<(u64, u64)> {
        let conn = Connection::open(database)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let (pending, terminal): (i64, i64) = conn.query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN terminal=0 THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN terminal=1 THEN 1 ELSE 0 END),0)
             FROM aiks_weknora_outbox",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok((pending.max(0) as u64, terminal.max(0) as u64))
    })
    .await?
}

fn retry_delay_seconds(attempts: u32) -> u64 {
    let shift = attempts.min(8);
    (1_u64 << shift).min(MAX_RETRY_SECONDS)
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
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
