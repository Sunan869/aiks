use std::ops::{Deref, DerefMut};
use std::sync::Mutex;

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
}

/// Safety wrapper around the raw SiYuan HTTP sink.
///
/// SiYuan document creation is not transactionally idempotent from the HTTP
/// client's point of view: a very large `createDocWithMd` request can commit
/// server-side and still time out before AIKS receives the new document id.
/// This wrapper reconciles by the deterministic notebook + hpath before and
/// after create so a retry adopts the existing document instead of duplicating
/// it.
pub struct SiYuanSink {
    inner: siyuan::SiYuanSink,
    /// When an update fails but the mapped document is confirmed to still
    /// exist, the legacy sync engine immediately falls back to create. Remember
    /// that ambiguity so the following create can be rejected instead of
    /// cloning a stale mapped document. This is consumed by `create_document`.
    blocked_recreate: Mutex<Option<String>>,
}

impl SiYuanSink {
    pub fn embedded(
        base_url: impl Into<String>,
        notebook_name: impl Into<String>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            inner: siyuan::SiYuanSink::embedded(base_url, notebook_name)?,
            blocked_recreate: Mutex::new(None),
        })
    }

    pub fn new(config: SiYuanConfig) -> anyhow::Result<Self> {
        Ok(Self {
            inner: siyuan::SiYuanSink::new(config)?,
            blocked_recreate: Mutex::new(None),
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
            Some(SiYuanSafetyError::DocumentTooLarge { .. })
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

    /// Safe create used by all existing callers.
    pub async fn create_document(
        &self,
        notebook_id: &str,
        path: &str,
        markdown: &str,
    ) -> anyhow::Result<String> {
        if let Some(doc_id) = self.blocked_recreate.lock().unwrap().take() {
            anyhow::bail!(
                "refusing to recreate SiYuan document {doc_id} after an ambiguous update failure"
            );
        }
        self.create_document_reconciled(notebook_id, path, markdown)
            .await
    }

    /// Preserve the existing update API while preventing the sync engine's
    /// unconditional update->create fallback from duplicating a document on a
    /// timeout or other transient error. Recreate is allowed only when SiYuan
    /// confirms that the mapped document no longer exists.
    pub async fn update_document(&self, doc_id: &str, markdown: &str) -> anyhow::Result<()> {
        Self::ensure_safe_document_size(markdown)?;

        match self.inner.update_document(doc_id, markdown).await {
            Ok(()) => {
                *self.blocked_recreate.lock().unwrap() = None;
                Ok(())
            }
            Err(update_error) => {
                let should_block_recreate = match self.inner.get_doc_notebook(doc_id).await {
                    Ok(Some(_)) => true,
                    Ok(None) => false,
                    Err(_) => true,
                };
                *self.blocked_recreate.lock().unwrap() =
                    should_block_recreate.then(|| doc_id.to_string());
                Err(update_error)
            }
        }
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
