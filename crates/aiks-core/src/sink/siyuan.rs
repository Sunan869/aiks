/// SiYuan knowledge base sink.
///
/// Integrates with SiYuan via HTTP API to create/update session documents.
///
/// Two modes (spec §28, §64):
///   - Embedded: `SiYuanSink::embedded(base_url, notebook_name)` — no token required
///   - External:  `SiYuanSink::new(config)` — uses token from config
///
/// Required SiYuan API endpoints:
/// - GET /api/system/getConf - health check
/// - POST /api/notebook/lsNotebooks - list notebooks
/// - POST /api/notebook/createNotebook - create notebook
/// - POST /api/filetree/createDocWithMd - create document with markdown
/// - POST /api/block/getBlockAttrs - get block attributes
/// - POST /api/block/setBlockAttrs - set block attributes
/// - POST /api/block/updateBlock - update block content
/// - POST /api/search/searchAttr - search by custom attributes
use std::collections::HashMap;

use anyhow::Context;
use reqwest::Client;
use serde::{Deserialize, Serialize};

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
    base_url: String,
    notebook_name: String,
    session_root: String,
    /// Optional token — None for embedded mode, Some for external mode
    token: Option<String>,
    client: Client,
}

impl SiYuanSink {
    /// Embedded mode: no token required (spec §25-26).
    /// SiYuan listens on 127.0.0.1 only and allows unauthenticated local requests.
    pub fn embedded(base_url: impl Into<String>, notebook_name: impl Into<String>) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self {
            base_url: base_url.into(),
            notebook_name: notebook_name.into(),
            session_root: "/10 AI Sessions".to_string(),
            token: None,
            client,
        })
    }

    /// External mode: uses token from config (CLI / developer mode).
    pub fn new(config: SiYuanConfig) -> anyhow::Result<Self> {
        let token = if config.token.is_empty() { None } else { Some(config.token.clone()) };
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self {
            base_url: config.base_url,
            notebook_name: config.notebook_name,
            session_root: config.session_root,
            token,
            client,
        })
    }

    pub fn sink_name() -> &'static str {
        SINK_NAME
    }

    /// Add Authorization header if token is set.
    fn request_builder(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        let builder = self.client.request(method, url);
        if let Some(token) = &self.token {
            builder.header("Authorization", format!("Token {}", token))
        } else {
            builder
        }
    }

    /// Check if SiYuan is running and accessible.
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/api/system/version", self.base_url);
        match self.request_builder(reqwest::Method::GET, &url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Get or create the target notebook.
    pub async fn ensure_notebook(&self) -> anyhow::Result<String> {
        let url = format!("{}/api/notebook/lsNotebooks", self.base_url);
        let resp: ApiResponse<NotebooksResult> = self
            .request_builder(reqwest::Method::POST, &url)
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
            if nb.name == self.notebook_name {
                return Ok(nb.id.clone());
            }
        }

        // Create new notebook
        let create_url = format!("{}/api/notebook/createNotebook", self.base_url);
        let resp: ApiResponse<NotebookCreateResult> = self
            .request_builder(reqwest::Method::POST, &create_url)
            .json(&serde_json::json!({"name": self.notebook_name}))
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

    /// Find a managed document by its target_id saved in the local sync_target table.
    /// This replaces the non-existent /api/search/searchAttr endpoint.
    /// Callers should use the locally saved target_id from the DB directly
    /// rather than searching SiYuan by attribute.
    pub async fn find_document_by_session(
        &self,
        _source: &str,
        _session_id: &str,
    ) -> anyhow::Result<Option<DocumentInfo>> {
        // Note: SiYuan v3.8.3 does not have /api/search/searchAttr.
        // Document lookup should use the locally persisted target_id from sync_target.
        // This method is kept for interface compatibility; callers should prefer
        // finding the target_id in the local DB first.
        Ok(None)
    }

    /// Create a new document with Markdown content.
    /// SiYuan v3.8.3: createDocWithMd returns data as a plain string ID.
    pub async fn create_document(
        &self,
        notebook_id: &str,
        path: &str,
        markdown: &str,
    ) -> anyhow::Result<String> {
        let url = format!("{}/api/filetree/createDocWithMd", self.base_url);
        // The API returns: {"code":0,"msg":"","data":"20260913000000-blockid"}
        // where data is a string, not an object.
        let resp: ApiResponse<serde_json::Value> = self
            .request_builder(reqwest::Method::POST, &url)
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
            anyhow::bail!("SiYuan createDocWithMd error {}: {}", resp.code, resp.msg);
        }

        // data is a string ID directly (v3.8.3 contract)
        match resp.data {
            Some(serde_json::Value::String(id)) if !id.is_empty() => Ok(id),
            Some(serde_json::Value::Object(obj)) => {
                // Fallback: older versions may return {id: "..."}
                obj.get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow::anyhow!("createDocWithMd: no id in object response"))
            }
            Some(other) => anyhow::bail!("createDocWithMd: unexpected data type: {:?}", other),
            None => anyhow::bail!("createDocWithMd: no data in response"),
        }
    }

    /// Update the content of an existing document.
    pub async fn update_document(&self, doc_id: &str, markdown: &str) -> anyhow::Result<()> {
        let url = format!("{}/api/block/updateBlock", self.base_url);
        let resp: ApiResponse<serde_json::Value> = self
            .request_builder(reqwest::Method::POST, &url)
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
    /// SiYuan v3.8.3: /api/attr/setBlockAttrs
    pub async fn set_block_attrs(
        &self,
        block_id: &str,
        attrs: &HashMap<String, String>,
    ) -> anyhow::Result<()> {
        let url = format!("{}/api/attr/setBlockAttrs", self.base_url);
        let resp: ApiResponse<serde_json::Value> = self
            .request_builder(reqwest::Method::POST, &url)
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
            anyhow::bail!("SiYuan setBlockAttrs error {}: {}", resp.code, resp.msg);
        }

        Ok(())
    }

    /// Get attributes of a block.
    /// SiYuan v3.8.3: /api/attr/getBlockAttrs
    pub async fn get_block_attrs(
        &self,
        block_id: &str,
    ) -> anyhow::Result<HashMap<String, String>> {
        let url = format!("{}/api/attr/getBlockAttrs", self.base_url);
        let resp: ApiResponse<HashMap<String, String>> = self
            .request_builder(reqwest::Method::POST, &url)
            .json(&serde_json::json!({"id": block_id}))
            .send()
            .await
            .context("get block attrs")?
            .json()
            .await
            .context("parse get attrs response")?;

        if resp.code != 0 {
            anyhow::bail!("SiYuan getBlockAttrs error {}: {}", resp.code, resp.msg);
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
    /// Format: {session_root}/{source}/{yyyy}/{MM}/{yyyy-MM-dd} {title} [{short-id}]
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
            self.session_root,
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
        let sink = SiYuanSink::embedded("http://127.0.0.1:6806", "AI Knowledge").unwrap();

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
        let sink = SiYuanSink::embedded("http://127.0.0.1:6806", "AI Knowledge").unwrap();

        let path = sink.build_document_path(
            "opencode",
            "sess-1",
            Some("Fix: path/to/file issue"),
            None,
        );
        assert!(!path.contains("path/to/file"), "Title / should be sanitized: {}", path);
        assert!(path.contains("path"));
    }

    #[test]
    fn embedded_sink_without_token() {
        // Spec §28: embedded mode must not require a token
        let sink = SiYuanSink::embedded("http://127.0.0.1:6806", "AI Knowledge").unwrap();
        assert!(sink.token.is_none());
    }

    #[test]
    fn external_sink_with_token() {
        let mut config = crate::config::SiYuanConfig::default();
        config.token = "my-test-token".to_string();
        let sink = SiYuanSink::new(config).unwrap();
        assert_eq!(sink.token.as_deref(), Some("my-test-token"));
    }

    #[test]
    fn external_sink_empty_token_becomes_none() {
        let config = crate::config::SiYuanConfig::default(); // token is empty string by default
        let sink = SiYuanSink::new(config).unwrap();
        assert!(sink.token.is_none());
    }
}
