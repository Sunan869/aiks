use std::ops::{Deref, DerefMut};

use anyhow::Context;

use crate::config::SiYuanConfig;

use super::siyuan;

/// A single huge `createDocWithMd` call is the failure mode that originally
/// produced duplicate documents: SiYuan could commit the document after the
/// client had already timed out. Keep a conservative hard stop at the HTTP
/// sink boundary so no caller can accidentally reintroduce that unsafe write.
const MAX_SAFE_DOCUMENT_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
enum SiYuanSafetyError {
    #[error("SiYuan document too large: {bytes} bytes exceeds the {limit} byte safety limit")]
    DocumentTooLarge { bytes: usize, limit: usize },
    #[error("SiYuan volume {volume_no} ({doc_id}) was edited; refusing to overwrite")]
    VolumeConflict { volume_no: usize, doc_id: String },
}

/// Safety wrapper around the raw SiYuan HTTP sink.
///
/// SiYuan document creation is not transactionally idempotent from the HTTP
/// client's point of view: a very large `createDocWithMd` request can commit
/// server-side and still time out before AIKS receives the new document id.
/// This wrapper reconciles by the deterministic notebook + hpath before and
/// after create so a retry adopts the existing document instead of duplicating
/// it.
pub struct SessionVolumeRequest<'a> {
    pub db: &'a crate::storage::StateDb,
    pub session_db_id: i64,
    pub source: &'a str,
    pub external_id: &'a str,
    pub parser_version: &'a str,
    pub notebook_id: &'a str,
    pub base_path: &'a str,
    pub markdown: &'a str,
}

pub struct SiYuanSink {
    inner: siyuan::SiYuanSink,
}

impl SiYuanSink {
    pub fn embedded(
        base_url: impl Into<String>,
        notebook_name: impl Into<String>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            inner: siyuan::SiYuanSink::embedded(base_url, notebook_name)?,
        })
    }

    pub fn new(config: SiYuanConfig) -> anyhow::Result<Self> {
        Ok(Self {
            inner: siyuan::SiYuanSink::new(config)?,
        })
    }

    pub fn sink_name() -> &'static str {
        siyuan::SiYuanSink::sink_name()
    }

    /// Classify sink errors for the sync state machine. Safety-policy failures
    /// caused by an unchanged payload cannot improve through blind retries;
    /// transport/API failures remain retryable.
    pub fn is_retryable_write_error(error: &anyhow::Error) -> bool {
        !matches!(
            error.downcast_ref::<SiYuanSafetyError>(),
            Some(
                SiYuanSafetyError::DocumentTooLarge { .. }
                    | SiYuanSafetyError::VolumeConflict { .. }
            )
        )
    }

    fn ensure_safe_document_size(markdown: &str) -> anyhow::Result<()> {
        let bytes = markdown.len();
        if bytes > MAX_SAFE_DOCUMENT_BYTES {
            return Err(SiYuanSafetyError::DocumentTooLarge {
                bytes,
                limit: MAX_SAFE_DOCUMENT_BYTES,
            }
            .into());
        }
        Ok(())
    }

    fn trim_trailing_line_endings(value: &str) -> &str {
        value.trim_end_matches(['\r', '\n'])
    }

    fn markdown_matches(remote: &str, expected: &str) -> bool {
        Self::trim_trailing_line_endings(remote) == Self::trim_trailing_line_endings(expected)
    }

    async fn verify_existing_document_matches(
        &self,
        doc_id: &str,
        expected_markdown: &str,
    ) -> anyhow::Result<()> {
        let remote_markdown = self
            .inner
            .get_document_markdown(doc_id)
            .await
            .with_context(|| format!("verify reconciled SiYuan document {doc_id}"))?;

        if !Self::markdown_matches(&remote_markdown, expected_markdown) {
            anyhow::bail!(
                "SiYuan document content mismatch for {doc_id}; refusing to adopt an existing document that may contain user edits"
            );
        }

        Ok(())
    }

    /// Resolve a document by its deterministic human path inside one notebook.
    /// `path` is the same hpath passed to `createDocWithMd`.
    pub async fn find_document_by_hpath(
        &self,
        notebook_id: &str,
        path: &str,
    ) -> anyhow::Result<Option<String>> {
        let box_id = notebook_id.replace('\'', "''");
        let hpath = path.replace('\'', "''");
        let rows = self
            .inner
            .query_sql(&format!(
                "SELECT id FROM blocks WHERE box = '{box_id}' AND type = 'd' AND hpath = '{hpath}' ORDER BY id DESC LIMIT 1"
            ))
            .await?;

        Ok(rows
            .into_iter()
            .find_map(|row| row.get("id").and_then(|id| id.as_str()).map(str::to_owned)))
    }

    /// Idempotent create for deterministic AIKS document paths.
    ///
    /// 1. If a document already exists at the target hpath, adopt it only when
    ///    its current content still matches the payload we intended to create.
    /// 2. Otherwise issue createDocWithMd.
    /// 3. If create fails/returns an error, reconcile the hpath once more and
    ///    adopt a discovered document only after the same content check.
    pub async fn create_document_reconciled(
        &self,
        notebook_id: &str,
        path: &str,
        markdown: &str,
    ) -> anyhow::Result<String> {
        Self::ensure_safe_document_size(markdown)?;

        if let Some(id) = self.find_document_by_hpath(notebook_id, path).await? {
            self.verify_existing_document_matches(&id, markdown).await?;
            return Ok(id);
        }

        match self
            .inner
            .create_document(notebook_id, path, markdown)
            .await
        {
            Ok(id) => Ok(id),
            Err(create_error) => match self.find_document_by_hpath(notebook_id, path).await {
                Ok(Some(id)) => match self.verify_existing_document_matches(&id, markdown).await {
                    Ok(()) => Ok(id),
                    Err(verify_error) => Err(anyhow::anyhow!(
                        "SiYuan create failed: {create_error}; reconciliation found document {id}, but it could not be safely adopted: {verify_error}"
                    )),
                },
                Ok(None) => Err(create_error),
                Err(reconcile_error) => Err(create_error).with_context(|| {
                    format!(
                        "SiYuan create failed and reconciliation also failed: {reconcile_error}"
                    )
                }),
            },
        }
    }

    /// True when a previous sync already used multi-document volumes. Keep
    /// using the index even if the session later shrinks below the threshold.
    pub fn has_session_volumes(
        db: &crate::storage::StateDb,
        session_id: i64,
    ) -> anyhow::Result<bool> {
        let conn = db.conn();
        let exists: i64 = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_volume')",
            [], |row| row.get(0),
        )?;
        if exists == 0 {
            return Ok(false);
        }
        Ok(conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM session_volume WHERE session_id = ?1)",
            [session_id],
            |row| row.get::<_, i64>(0),
        )? != 0)
    }

    /// Write content volumes and return a small index document's Markdown.
    /// Each volume has its own stable hpath, ID and remote hash baseline.
    /// A failed attempt can resume without losing prior successful writes.
    pub async fn sync_session_volumes(
        &self,
        request: SessionVolumeRequest<'_>,
    ) -> anyhow::Result<String> {
        let SessionVolumeRequest {
            db,
            session_db_id,
            source,
            external_id,
            parser_version,
            notebook_id,
            base_path,
            markdown,
        } = request;
        use anyhow::Context;
        use rusqlite::{params, OptionalExtension};
        {
            let conn = db.conn();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS session_volume (
                    session_id INTEGER NOT NULL,
                    volume_index INTEGER NOT NULL,
                    doc_id TEXT NOT NULL,
                    doc_path TEXT NOT NULL,
                    remote_hash TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY(session_id, volume_index)
                );",
            )?;
        }
        let volumes = split_session_markdown(markdown);
        let mut entries = Vec::with_capacity(volumes.len());
        for (index, body) in volumes.iter().enumerate() {
            let volume_no = index + 1;
            let path = format!("{base_path} - Part {volume_no:04}");
            let volume_markdown = format!("# Session Part {volume_no}/{}\n\n{body}", volumes.len());
            Self::ensure_safe_document_size(&volume_markdown)?;
            let existing: Option<(String, String)> = {
                let conn = db.conn();
                conn.query_row(
                    "SELECT doc_id, remote_hash FROM session_volume
                     WHERE session_id = ?1 AND volume_index = ?2",
                    params![session_db_id, volume_no as i64],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
            };
            let doc_id = if let Some((id, baseline)) = existing {
                match self.inner.get_document_markdown(&id).await {
                    Ok(remote) => {
                        let actual = volume_baseline(&remote);
                        if actual != baseline {
                            return Err(SiYuanSafetyError::VolumeConflict {
                                volume_no,
                                doc_id: id,
                            }
                            .into());
                        }
                        if !Self::markdown_matches(&remote, &volume_markdown) {
                            self.update_document(&id, &volume_markdown)
                                .await
                                .with_context(|| format!("update volume {volume_no}"))?;
                        }
                        id
                    }
                    Err(read_error) => match self.inner.get_doc_notebook(&id).await {
                        Ok(None) => {
                            // The mapped block really was deleted. Reconcile by stable
                            // hpath; an existing document is only adopted after
                            // content validation by create_document_reconciled.
                            self.create_document(notebook_id, &path, &volume_markdown)
                                .await
                                .with_context(|| {
                                    format!("recreate confirmed missing volume {volume_no} ({id})")
                                })?
                        }
                        Ok(Some(_)) => {
                            return Err(read_error).with_context(|| {
                                format!("volume {volume_no} ({id}) still exists but is unreadable")
                            });
                        }
                        Err(probe_error) => {
                            return Err(read_error).with_context(|| {
                                format!(
                                    "volume {volume_no} ({id}) read failed and existence is unknown: {probe_error}"
                                )
                            });
                        }
                    },
                }
            } else {
                self.create_document(notebook_id, &path, &volume_markdown)
                    .await
                    .with_context(|| format!("create volume {volume_no}"))?
            };
            // Capture the server's own Markdown representation after write,
            // not the local payload, because SiYuan may normalize Markdown.
            let remote = self
                .inner
                .get_document_markdown(&doc_id)
                .await
                .with_context(|| format!("capture volume {volume_no} remote baseline"))?;
            let baseline = volume_baseline(&remote);
            {
                let conn = db.conn();
                conn.execute(
                    "INSERT INTO session_volume
                     (session_id, volume_index, doc_id, doc_path, remote_hash, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(session_id, volume_index) DO UPDATE SET
                       doc_id = excluded.doc_id,
                       doc_path = excluded.doc_path,
                       remote_hash = excluded.remote_hash,
                       updated_at = excluded.updated_at",
                    params![
                        session_db_id,
                        volume_no as i64,
                        doc_id,
                        path,
                        baseline,
                        chrono::Utc::now().to_rfc3339()
                    ],
                )?;
            }
            self.set_aiks_attrs(&doc_id, source, external_id, &baseline, parser_version)
                .await
                .with_context(|| format!("set volume {volume_no} attributes"))?;
            entries.push(format!("- [第 {volume_no} 部分](siyuan://blocks/{doc_id})"));
        }
        // Historical volumes after shrink are retained, never deleted automatically.
        let stale_count: i64 = db.conn().query_row(
            "SELECT count(*) FROM session_volume
             WHERE session_id = ?1 AND volume_index > ?2",
            rusqlite::params![session_db_id, volumes.len() as i64],
            |row| row.get(0),
        )?;
        let mut index = format!(
            "# Session 分卷目录\n\n> 来源：{source}\n> Session ID：{external_id}\n> 分卷数：{}\n\n",
            entries.len()
        );
        index.push_str(&entries.join("\n"));
        index.push('\n');
        if stale_count > 0 {
            index.push_str(&format!(
                "\n> ⚠️ 会话缩短后仍保留 {stale_count} 个历史分卷，请人工核查后归档。\n"
            ));
        }
        Self::ensure_safe_document_size(&index)?;
        Ok(index)
    }

    /// Safe create used by all existing callers.
    pub async fn create_document(
        &self,
        notebook_id: &str,
        path: &str,
        markdown: &str,
    ) -> anyhow::Result<String> {
        self.create_document_reconciled(notebook_id, path, markdown)
            .await
    }

    /// Preserve the existing update API. The sync engine owns the decision to
    /// recreate the current mapped document after an update failure, so the
    /// sink must not retain cross-session state that can poison another create.
    pub async fn update_document(&self, doc_id: &str, markdown: &str) -> anyhow::Result<()> {
        Self::ensure_safe_document_size(markdown)?;
        self.inner.update_document(doc_id, markdown).await
    }
}

impl Deref for SiYuanSink {
    type Target = siyuan::SiYuanSink;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for SiYuanSink {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Four MiB leaves space for part headings and SiYuan's HTTP overhead.
const TARGET_VOLUME_BYTES: usize = 4 * 1024 * 1024;

fn volume_baseline(md: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("md:{}", hex::encode(Sha256::digest(md.as_bytes())))
}

/// Preserve every UTF-8 byte, preferring newline boundaries. A single huge
/// line is split on character boundaries; no content is dropped.
fn split_session_markdown(markdown: &str) -> Vec<String> {
    if markdown.is_empty() {
        return vec![String::new()];
    }
    let mut parts = Vec::new();
    let mut rest = markdown;
    while rest.len() > TARGET_VOLUME_BYTES {
        let mut limit = TARGET_VOLUME_BYTES;
        while !rest.is_char_boundary(limit) {
            limit -= 1;
        }
        let prefix = &rest[..limit];
        // Message headings are emitted by MarkdownRenderer at the start
        // of each User/Assistant/Tool turn. Prefer a complete turn boundary.
        let heading = prefix.rfind("\n## ").map(|at| at + 1);
        let newline = prefix.rfind('\n').map(|at| at + 1);
        let cut = heading
            .filter(|&at| at >= TARGET_VOLUME_BYTES / 3)
            .or_else(|| newline.filter(|&at| at >= TARGET_VOLUME_BYTES / 3))
            .unwrap_or(limit);
        parts.push(rest[..cut].to_string());
        rest = &rest[cut..];
    }
    if !rest.is_empty() {
        parts.push(rest.to_string());
    }
    parts
}

#[cfg(test)]
mod volume_tests {
    use super::*;
    #[test]
    fn splits_large_markdown_without_losing_any_bytes() {
        let input = format!(
            "{}\n{}\n{}",
            "a".repeat(TARGET_VOLUME_BYTES),
            "中".repeat(2_000_000),
            "TAIL"
        );
        let parts = split_session_markdown(&input);
        assert!(parts.len() >= 3);
        assert!(parts.iter().all(|p| p.len() <= TARGET_VOLUME_BYTES));
        assert_eq!(parts.concat(), input);
    }
    #[test]
    fn prefers_complete_message_boundary() {
        let markdown = format!(
            "{}\n## 👤 用户 (User)\n\n{}",
            "a".repeat(TARGET_VOLUME_BYTES - 500),
            "b".repeat(1500),
        );
        let parts = split_session_markdown(&markdown);
        assert_eq!(parts.len(), 2);
        assert!(parts[1].starts_with("## 👤 用户 (User)"));
        assert_eq!(parts.concat(), markdown);
    }

    #[test]
    fn split_is_deterministic_and_preserves_multibyte_boundaries() {
        let content = format!(
            "{}\n## 🤖 助手 (Assistant)\n{}\n## 👤 用户 (User)\n{}",
            "前言".repeat(850_000),
            "中文🚀".repeat(330_000),
            "结束".repeat(400_000),
        );
        let first = split_session_markdown(&content);
        let second = split_session_markdown(&content);
        assert_eq!(first, second, "retry must reuse the same boundaries");
        assert_eq!(first.concat(), content, "no text may be lost or duplicated");
        assert!(first.iter().all(|part| part.len() <= TARGET_VOLUME_BYTES));
        assert!(first.len() > 1);
        assert!(first.iter().all(|part| !part.is_empty()));
    }

    #[test]
    fn split_preserves_content_when_session_grows() {
        let head = format!(
            "## 👤 用户 (User)\n{}\n",
            "a".repeat(TARGET_VOLUME_BYTES - 300)
        );
        let addition = format!("## 🤖 助手 (Assistant)\n{}\n", "b".repeat(1200));
        let original = split_session_markdown(&head);
        assert_eq!(original.concat(), head);
        let expanded = split_session_markdown(&(head.clone() + &addition));
        assert_eq!(expanded.concat(), head + &addition);
        assert!(expanded.len() >= 2);
        assert!(expanded
            .iter()
            .all(|part| part.len() <= TARGET_VOLUME_BYTES));
    }

    #[test]
    fn shrinking_session_keeps_deterministic_part_paths() {
        let full = format!(
            "{}\n## 🤖 助手 (Assistant)\n{}",
            "a".repeat(TARGET_VOLUME_BYTES - 200),
            "b".repeat(TARGET_VOLUME_BYTES),
        );
        let shortened = &full[..TARGET_VOLUME_BYTES / 2];
        let full_parts = split_session_markdown(&full);
        let short_parts = split_session_markdown(shortened);
        assert!(full_parts.len() >= 2);
        assert_eq!(short_parts.len(), 1);
        assert_eq!(full_parts.concat(), full);
        assert_eq!(short_parts.concat(), shortened);
        assert!(full_parts.iter().all(|part| part.len() <= TARGET_VOLUME_BYTES));
    }

    #[test]
    fn split_handles_single_line_larger_than_document_limit() {
        let input = "🦀".repeat(MAX_SAFE_DOCUMENT_BYTES / 2);
        let parts = split_session_markdown(&input);
        assert!(parts.len() >= 2);
        assert!(parts.iter().all(|part| part.len() <= TARGET_VOLUME_BYTES));
        assert_eq!(parts.concat(), input);
    }

    #[test]
    fn preserves_single_small_document() {
        assert_eq!(split_session_markdown("# hello\n"), vec!["# hello\n"]);
    }
}
