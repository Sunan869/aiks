/// SiYuan knowledge base sink.
///
/// Integrates with SiYuan via HTTP API to create/update session documents.
///
/// Required SiYuan API endpoints:
/// - GET /api/system/getConf - health check
/// - POST /api/notebook/lsNotebooks - list notebooks
/// - POST /api/notebook/createNotebook - create notebook
/// - POST /api/filetree/createDocWithMd - create document with markdown
/// - POST /api/block/getBlockAttrs - get block attributes
/// - POST /api/block/setBlockAttrs - set block attributes
/// - POST /api/block/updateBlock - update block content
/// - POST /api/filetree/getPathByID - get document path
/// - POST /api/search/searchAttr - search by custom attributes
use std::collections::HashMap;

use anyhow::Context;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::config::SiYuanConfig;

const SINK_NAME: &str = "siyuan";

/// Attributes stored on managed SiYuan documents.
pub const ATTR_MANAGED: &str = "custom-aiks-managed";
pub const ATTR_SOURCE: &str = "custom-aiks-source";
pub const ATTR_SESSION_ID: &str = "custom-aiks-session-id";
pub const ATTR_CONTENT_HASH: &str = "custom-aiks-content-hash";
pub const ATTR_PARSER_VERSION: &str = "custom-aiks-parser-version";
pub const ATTR_SYNCED_AT: &str = "custom-aiks-synced-at";

pub struct SiYuanSink {
    config: SiYuanConfig,
    client: Client,
}

impl SiYuanSink {
    pub fn new(config: SiYuanConfig) -> anyhow::Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        if !config.token.is_empty() {
            let auth_value = reqwest::header::HeaderValue::from_str(&format!("Token {}", config.token))?;
            headers.insert(reqwest::header::AUTHORIZATION, auth_value);
        }

        let client = Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self { config, client })
    }

    pub fn sink_name() -> &'static str {
        SINK_NAME
    }

    /// Check if SiYuan is running and accessible.
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/api/system/getConf", self.config.base_url);
        match self.client.post(&url).json(&serde_json::json!({})).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Get or create the target notebook.
    pub async fn ensure_notebook(&self) -> anyhow::Result<String> {
        let url = format!("{}/api/notebook/lsNotebooks", self.config.base_url);
        let resp: ApiResponse<NotebooksResult> = self
            .client
            .post(&url)
            .json(&serde_json::json!({}))
            .send()
            .await
            .context("list notebooks")?
            .json()
            .await
            .context("parse notebooks response")?;

        if resp.code != 0 {
            anyhow::bail!("SiYuan lsNotebooks error: {}", resp.msg);
        }

        let notebooks = resp.data.map(|d| d.notebooks).unwrap_or_default();
        for nb in &notebooks {
            if nb.name == self.config.notebook_name {
                return Ok(nb.id.clone());
            }
        }

        // Create new notebook
        let create_url = format!("{}/api/notebook/createNotebook", self.config.base_url);
        let resp: ApiResponse<NotebookCreateResult> = self
            .client
            .post(&create_url)
            .json(&serde_json::json!({"name": self.config.notebook_name}))
            .send()
            .await
            .context("create notebook")?
            .json()
            .await
            .context("parse create notebook response")?;

        if resp.code != 0 {
            anyhow::bail!("SiYuan createNotebook error: {}", resp.msg);
        }

        resp.data
            .map(|d| d.notebook.id)
            .ok_or_else(|| anyhow::anyhow!("No notebook ID in response"))
    }

    /// Find a managed document by its AIKS session ID.
    pub async fn find_document_by_session(
        &self,
        source: &str,
        session_id: &str,
    ) -> anyhow::Result<Option<DocumentInfo>> {
        let url = format!("{}/api/search/searchAttr", self.config.base_url);
        let resp: ApiResponse<SearchAttrResult> = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "name": ATTR_SESSION_ID,
                "value": session_id
            }))
            .send()
            .await
            .context("search by session attr")?
            .json()
            .await
            .context("parse search attr response")?;

        if resp.code != 0 {
            return Ok(None);
        }

        let blocks = resp.data.map(|d| d.blocks).unwrap_or_default();
        for block in blocks {
            // Verify it's our document (source matches)
            if let Some(attrs) = self.get_block_attrs(&block.id).await.ok() {
                if attrs.get(ATTR_SOURCE).map(|s| s.as_str()) == Some(source) {
                    return Ok(Some(DocumentInfo {
                        id: block.id,
                        path: block.path,
                        content_hash: attrs.get(ATTR_CONTENT_HASH).cloned(),
                        parser_version: attrs.get(ATTR_PARSER_VERSION).cloned(),
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Create a new document with Markdown content.
    pub async fn create_document(
        &self,
        notebook_id: &str,
        path: &str,
        markdown: &str,
    ) -> anyhow::Result<String> {
        let url = format!("{}/api/filetree/createDocWithMd", self.config.base_url);
        let resp: ApiResponse<CreateDocResult> = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "notebook": notebook_id,
                "path": path,
                "markdown": markdown
            }))
            .send()
            .await
            .context("create document")?
            .json()
            .await
            .context("parse create document response")?;

        if resp.code != 0 {
            anyhow::bail!("SiYuan createDocWithMd error: {}", resp.msg);
        }

        resp.data
            .and_then(|d| d.id)
            .ok_or_else(|| anyhow::anyhow!("No document ID in create response"))
    }

    /// Update the content of an existing document.
    pub async fn update_document(&self, doc_id: &str, markdown: &str) -> anyhow::Result<()> {
        let url = format!("{}/api/block/updateBlock", self.config.base_url);
        let resp: ApiResponse<serde_json::Value> = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "id": doc_id,
                "data": markdown,
                "dataType": "markdown"
            }))
            .send()
            .await
            .context("update document")?
            .json()
            .await
            .context("parse update document response")?;

        if resp.code != 0 {
            anyhow::bail!("SiYuan updateBlock error: {}", resp.msg);
        }

        Ok(())
    }

    /// Set attributes on a block (document).
    pub async fn set_block_attrs(
        &self,
        block_id: &str,
        attrs: &HashMap<String, String>,
    ) -> anyhow::Result<()> {
        let url = format!("{}/api/block/setBlockAttrs", self.config.base_url);
        let resp: ApiResponse<serde_json::Value> = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "id": block_id,
                "attrs": attrs
            }))
            .send()
            .await
            .context("set block attrs")?
            .json()
            .await
            .context("parse set attrs response")?;

        if resp.code != 0 {
            anyhow::bail!("SiYuan setBlockAttrs error: {}", resp.msg);
        }

        Ok(())
    }

    /// Get attributes of a block.
    pub async fn get_block_attrs(
        &self,
        block_id: &str,
    ) -> anyhow::Result<HashMap<String, String>> {
        let url = format!("{}/api/block/getBlockAttrs", self.config.base_url);
        let resp: ApiResponse<HashMap<String, String>> = self
            .client
            .post(&url)
            .json(&serde_json::json!({"id": block_id}))
            .send()
            .await
            .context("get block attrs")?
            .json()
            .await
            .context("parse get attrs response")?;

        if resp.code != 0 {
            anyhow::bail!("SiYuan getBlockAttrs error: {}", resp.msg);
        }

        Ok(resp.data.unwrap_or_default())
    }

    /// Set AIKS managed attributes on a document.
    pub async fn set_aiks_attrs(
        &self,
        doc_id: &str,
        source: &str,
        session_id: &str,
        content_hash: &str,
        parser_version: &str,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut attrs = HashMap::new();
        attrs.insert(ATTR_MANAGED.to_string(), "true".to_string());
        attrs.insert(ATTR_SOURCE.to_string(), source.to_string());
        attrs.insert(ATTR_SESSION_ID.to_string(), session_id.to_string());
        attrs.insert(ATTR_CONTENT_HASH.to_string(), content_hash.to_string());
        attrs.insert(ATTR_PARSER_VERSION.to_string(), parser_version.to_string());
        attrs.insert(ATTR_SYNCED_AT.to_string(), now);
        self.set_block_attrs(doc_id, &attrs).await
    }

    /// Build the document path for a session.
    ///
    /// Format: /10 AI Sessions/{source}/{yyyy}/{MM}/{yyyy-MM-dd} {title} [{short-id}]
    pub fn build_document_path(
        &self,
        source: &str,
        session_id: &str,
        title: Option<&str>,
        started_at: Option<&chrono::DateTime<chrono::Utc>>,
    ) -> String {
        let source_dir = match source {
            "claude_code" => "Claude",
            "codex" => "Codex",
            "gemini_cli" => "Gemini",
            "opencode" => "OpenCode",
            other => other,
        };

        let (year, month, date_str) = if let Some(ts) = started_at {
            (
                ts.format("%Y").to_string(),
                ts.format("%m").to_string(),
                ts.format("%Y-%m-%d").to_string(),
            )
        } else {
            let now = chrono::Utc::now();
            (
                now.format("%Y").to_string(),
                now.format("%m").to_string(),
                now.format("%Y-%m-%d").to_string(),
            )
        };

        let short_id: String = session_id.chars().take(8).collect();
        let title_part = title
            .unwrap_or("Untitled")
            .chars()
            .take(50)
            .collect::<String>();
        let sanitized_title = title_part.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-");

        format!(
            "{}/{}/{}/{}/{} {} [{}]",
            self.config.session_root,
            source_dir,
            year,
            month,
            date_str,
            sanitized_title,
            short_id
        )
    }
}

// ===== API Types =====

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    code: i32,
    msg: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct NotebooksResult {
    notebooks: Vec<Notebook>,
}

#[derive(Debug, Deserialize)]
struct Notebook {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct NotebookCreateResult {
    notebook: Notebook,
}

#[derive(Debug, Deserialize)]
struct CreateDocResult {
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchAttrResult {
    blocks: Vec<BlockInfo>,
}

#[derive(Debug, Deserialize)]
struct BlockInfo {
    id: String,
    path: String,
}

/// Information about a SiYuan document managed by AIKS.
#[derive(Debug, Clone)]
pub struct DocumentInfo {
    pub id: String,
    pub path: String,
    pub content_hash: Option<String>,
    pub parser_version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_document_path() {
        let config = SiYuanConfig::default();
        let sink = SiYuanSink::new(config).unwrap();

        let ts: chrono::DateTime<chrono::Utc> = chrono::DateTime::parse_from_rfc3339("2024-03-15T10:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let path = sink.build_document_path(
            "claude_code",
            "abc12345-6789-xxxx",
            Some("How do I sort a list?"),
            Some(&ts),
        );

        assert!(path.contains("/10 AI Sessions"));
        assert!(path.contains("Claude"));
        assert!(path.contains("2024"));
        assert!(path.contains("03"));
        assert!(path.contains("2024-03-15"));
        assert!(path.contains("sort"));
        assert!(path.contains("abc12345"));
    }

    #[test]
    fn build_document_path_sanitizes_title() {
        let config = SiYuanConfig::default();
        let sink = SiYuanSink::new(config).unwrap();

        let path = sink.build_document_path(
            "opencode",
            "sess-1",
            Some("Fix: path/to/file issue"),
            None,
        );
        // The path contains directory separators, but the title part must not
        // contain the original / that was in the title
        assert!(!path.contains("path/to/file"), "Title / should be sanitized: {}", path);
        // The path should still contain the word "path" (from the title)
        assert!(path.contains("path"));
    }
}
