// CI lint baseline: pre-existing Clippy debt; remove allowances incrementally.
#![allow(clippy::field_reassign_with_default)]

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
use serde::Deserialize;

use crate::config::SiYuanConfig;

const SINK_NAME: &str = "siyuan";

/// Attributes stored on managed SiYuan documents.
pub const ATTR_MANAGED: &str = "custom-aiks-managed";
/// Kind of managed doc: "session" or "knowledge".
pub const ATTR_KIND: &str = "custom-aiks-kind";
pub const ATTR_SOURCE: &str = "custom-aiks-source";
pub const ATTR_SESSION_ID: &str = "custom-aiks-session-id";
pub const ATTR_CONTENT_HASH: &str = "custom-aiks-content-hash";
pub const ATTR_PARSER_VERSION: &str = "custom-aiks-parser-version";
pub const ATTR_SYNCED_AT: &str = "custom-aiks-synced-at";

pub struct SiYuanSink {
    base_url: String,
    /// Canonical content notebook.
    notebook_name: String,
    /// Raw Session notebook. V4.1 intentionally resolves this to the same
    /// canonical notebook as Knowledge in every runtime mode.
    session_notebook_name: String,
    session_root: String,
    knowledge_root: String,
    /// Optional token — None for embedded mode, Some for external mode
    token: Option<String>,
    client: Client,
}

const DEFAULT_SESSION_ROOT: &str = "/10 AI Sessions";
const DEFAULT_KNOWLEDGE_ROOT: &str = "/20 Knowledge";

impl SiYuanSink {
    /// Embedded mode: no token required (spec §25-26).
    /// SiYuan listens on 127.0.0.1 only and allows unauthenticated local requests.
    /// Raw Sessions and Knowledge intentionally share one notebook in V4.1.
    pub fn embedded(
        base_url: impl Into<String>,
        notebook_name: impl Into<String>,
    ) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()?;
        let notebook_name = notebook_name.into();
        Ok(Self {
            base_url: base_url.into(),
            notebook_name: notebook_name.clone(),
            session_notebook_name: notebook_name,
            session_root: DEFAULT_SESSION_ROOT.to_string(),
            knowledge_root: DEFAULT_KNOWLEDGE_ROOT.to_string(),
            token: None,
            client,
        })
    }

    /// External/developer mode keeps its URL/token/path configuration, but V4.1
    /// ignores the legacy separate session notebook so the content model stays
    /// identical to embedded mode.
    pub fn new(config: SiYuanConfig) -> anyhow::Result<Self> {
        let token = if config.token.is_empty() {
            None
        } else {
            Some(config.token.clone())
        };
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()?;
        let notebook_name = config.notebook_name;
        Ok(Self {
            base_url: config.base_url,
            notebook_name: notebook_name.clone(),
            session_notebook_name: notebook_name,
            session_root: config.session_root,
            knowledge_root: config.knowledge_root,
            token,
            client,
        })
    }

    /// Read-only service adapter: no credential forwarding through redirects.
    pub fn service_reader(mut config: SiYuanConfig) -> anyhow::Result<Self> {
        let url = reqwest::Url::parse(&config.base_url)?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && matches!(url.path(), "" | "/"),
            "Invalid content store origin"
        );
        config.base_url = config.base_url.trim_end_matches('/').to_string();
        let mut sink = Self::new(config)?;
        sink.client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(sink)
    }

    /// Fixed endpoint and trusted mapped document ID; never a caller URL/path.
    pub async fn get_document_markdown_bounded(
        &self,
        doc_id: &str,
        budget: usize,
    ) -> anyhow::Result<String> {
        anyhow::ensure!(
            !doc_id.is_empty() && doc_id.len() <= 256,
            "Invalid mapped document identity"
        );
        let url = format!("{}/api/block/getBlockKramdown", self.base_url);
        let mut response = self
            .request_builder(reqwest::Method::POST, &url)
            .json(&serde_json::json!({"id":doc_id}))
            .send()
            .await?;
        anyhow::ensure!(response.status().is_success(), "Content store unavailable");
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                bytes.len().saturating_add(chunk.len()) <= budget,
                "Content response exceeds budget"
            );
            bytes.extend_from_slice(&chunk);
        }
        #[derive(Deserialize)]
        struct Content {
            kramdown: String,
        }
        let parsed: ApiResponse<Content> = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(parsed.code == 0, "Content store rejected mapped document");
        let content = parsed.data.context("Content store returned no document")?;
        Ok(strip_kramdown_attrs(&content.kramdown))
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
        match self
            .request_builder(reqwest::Method::GET, &url)
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Get or create the canonical content notebook.
    pub async fn ensure_notebook(&self) -> anyhow::Result<String> {
        self.ensure_notebook_named(&self.notebook_name).await
    }

    /// Get or create the notebook that stores raw Sessions. In V4.1 this is
    /// always the same canonical content notebook as Knowledge.
    pub async fn ensure_session_notebook(&self) -> anyhow::Result<String> {
        self.ensure_notebook_named(&self.session_notebook_name)
            .await
    }

    /// Get or create a notebook by name.
    pub async fn ensure_notebook_named(&self, name: &str) -> anyhow::Result<String> {
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
            if nb.name == name {
                return Ok(nb.id.clone());
            }
        }

        let create_url = format!("{}/api/notebook/createNotebook", self.base_url);
        let resp: ApiResponse<NotebookCreateResult> = self
            .request_builder(reqwest::Method::POST, &create_url)
            .json(&serde_json::json!({"name": name}))
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

    /// Get the notebook (box) ID that currently contains a document.
    /// Returns the notebook id (`box`) that currently contains `doc_id`, or
    /// None if the document does not exist.
    ///
    /// Implemented via /api/query/sql instead of /api/filetree/getDocInfo:
    /// the embedded SiYuan runtime does not expose getDocInfo (it answers
    /// plain-text 404), which made every knowledge-sync update-path call fail
    /// with "parse getDocInfo response". The blocks table answers the same
    /// question on any kernel build.
    pub async fn get_doc_notebook(&self, doc_id: &str) -> anyhow::Result<Option<String>> {
        let escaped = doc_id.replace('\'', "''");
        let rows = self
            .query_sql(&format!(
                "SELECT box FROM blocks WHERE id = '{escaped}' AND type = 'd' LIMIT 1"
            ))
            .await?;
        Ok(rows
            .into_iter()
            .next()
            .and_then(|row| row.get("box").and_then(|b| b.as_str()).map(str::to_string)))
    }

    /// Run a read-only SQL query via the official /api/query/sql endpoint.
    /// Used for bulk lookups during one-time migrations only.
    pub async fn query_sql(&self, stmt: &str) -> anyhow::Result<Vec<serde_json::Value>> {
        let url = format!("{}/api/query/sql", self.base_url);
        let resp: ApiResponse<Vec<serde_json::Value>> = self
            .request_builder(reqwest::Method::POST, &url)
            .json(&serde_json::json!({"stmt": stmt}))
            .send()
            .await
            .context("query sql")?
            .json()
            .await
            .context("parse query sql response")?;

        if resp.code != 0 {
            anyhow::bail!("SiYuan query sql error {}: {}", resp.code, resp.msg);
        }

        Ok(resp.data.unwrap_or_default())
    }

    /// Move documents to another notebook/path.
    /// SiYuan: /api/filetree/moveDocs {fromIDs, toNotebook, toPath}
    /// Doc block IDs are preserved across moves (the .sy file is the ID).
    pub async fn move_docs(
        &self,
        from_ids: &[String],
        to_notebook: &str,
        to_path: &str,
    ) -> anyhow::Result<()> {
        let url = format!("{}/api/filetree/moveDocs", self.base_url);
        let resp: ApiResponse<serde_json::Value> = self
            .request_builder(reqwest::Method::POST, &url)
            .json(&serde_json::json!({
                "fromIDs": from_ids,
                "toNotebook": to_notebook,
                "toPath": to_path,
            }))
            .send()
            .await
            .context("move docs")?
            .json()
            .await
            .context("parse moveDocs response")?;

        if resp.code != 0 {
            anyhow::bail!("SiYuan moveDocs error: {}", resp.msg);
        }

        Ok(())
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

        match resp.data {
            Some(serde_json::Value::String(id)) if !id.is_empty() => Ok(id),
            Some(serde_json::Value::Object(obj)) => obj
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow::anyhow!("createDocWithMd: no id in object response")),
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
    pub async fn get_block_attrs(&self, block_id: &str) -> anyhow::Result<HashMap<String, String>> {
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

    /// Fetch the current text content of a document (conflict baseline).
    pub async fn get_document_markdown(&self, doc_id: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct KramdownResult {
            #[serde(default)]
            kramdown: String,
        }
        let url = format!("{}/api/block/getBlockKramdown", self.base_url);
        let resp: ApiResponse<KramdownResult> = self
            .request_builder(reqwest::Method::POST, &url)
            .json(&serde_json::json!({"id": doc_id}))
            .send()
            .await
            .context("get block kramdown")?
            .json()
            .await
            .context("parse block kramdown response")?;

        if resp.code != 0 {
            anyhow::bail!("SiYuan getBlockKramdown error {}: {}", resp.code, resp.msg);
        }

        let raw = resp
            .data
            .map(|d| d.kramdown)
            .ok_or_else(|| anyhow::anyhow!("getBlockKramdown: no data in response"))?;
        Ok(strip_kramdown_attrs(&raw))
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

    /// Build the document path for a knowledge item.
    pub fn build_knowledge_path(&self, category: &str, knowledge_id: &str, title: &str) -> String {
        let cat_label = crate::renderer::knowledge::category_display_name(category);
        let short_id: String = knowledge_id.chars().take(8).collect();
        let title_part: String = title.chars().take(50).collect();
        let sanitized = title_part.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-");
        let sanitized = crate::util::sanitizer::default_sanitizer().sanitize(&sanitized);
        format!(
            "{}/{}/{} [{}]",
            self.knowledge_root, cat_label, sanitized, short_id
        )
    }

    /// Set AIKS managed attributes on a knowledge document.
    pub async fn set_knowledge_attrs(
        &self,
        doc_id: &str,
        knowledge_id: &str,
        session_ext_id: &str,
        content_hash: &str,
        category: &str,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut attrs = HashMap::new();
        attrs.insert(ATTR_MANAGED.to_string(), "true".to_string());
        attrs.insert(ATTR_KIND.to_string(), "knowledge".to_string());
        attrs.insert(
            "custom-aiks-knowledge-id".to_string(),
            knowledge_id.to_string(),
        );
        attrs.insert(ATTR_SESSION_ID.to_string(), session_ext_id.to_string());
        attrs.insert(ATTR_CONTENT_HASH.to_string(), content_hash.to_string());
        attrs.insert("custom-aiks-category".to_string(), category.to_string());
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
        let sanitized_title =
            title_part.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-");
        let sanitized_title =
            crate::util::sanitizer::default_sanitizer().sanitize(&sanitized_title);

        format!(
            "{}/{}/{}/{}/{} {} [{}]",
            self.session_root, source_dir, year, month, date_str, sanitized_title, short_id
        )
    }
}

fn strip_kramdown_attrs(md: &str) -> String {
    let (body, had_trailing_nl) = match md.strip_suffix('\n') {
        Some(rest) => (rest, true),
        None => (md, false),
    };
    let mut out = String::with_capacity(md.len());
    for line in body.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        out.push_str(strip_trailing_attr(line));
        out.push('\n');
    }
    if !had_trailing_nl && out.ends_with('\n') {
        out.truncate(out.len() - 1);
    }
    out
}

fn strip_trailing_attr(line: &str) -> &str {
    if let Some(idx) = line.rfind("{:") {
        let candidate = line[idx..].trim_end();
        if candidate.starts_with("{:") && candidate.ends_with('}') && candidate.contains("id=\"") {
            return line[..idx].trim_end();
        }
    }
    line
}

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
    fn strip_kramdown_attrs_removes_block_ids() {
        let md = "# Title\n{: id=\"20260101-abc\" updated=\"20260101\"}\n\n正文段落\n{: id=\"20260101-def\"}\n";
        let out = strip_kramdown_attrs(md);
        assert!(!out.contains("id="), "attrs must be stripped: {}", out);
        assert!(out.contains("# Title"));
        assert!(out.contains("正文段落"));
    }

    #[test]
    fn strip_kramdown_attrs_keeps_plain_content() {
        let md = "{\"notebook\":\"20260101-abc\"}\ncode with {: unusual } tail\n";
        let out = strip_kramdown_attrs(md);
        assert!(out.contains("{\"notebook\""));
        assert!(out.contains("{: unusual }"));
    }

    #[test]
    fn strip_kramdown_attrs_preserves_trailing_newline_semantics() {
        assert_eq!(strip_kramdown_attrs("exported"), "exported");
        assert_eq!(strip_kramdown_attrs("exported\n"), "exported\n");
        assert_eq!(strip_kramdown_attrs("a\n{: id=\"x\"}\nb"), "a\n\nb");
        assert_eq!(strip_kramdown_attrs("a\n{: id=\"x\"}\nb\n"), "a\n\nb\n");
    }

    #[test]
    fn build_document_path() {
        let sink = SiYuanSink::embedded("http://127.0.0.1:6806", "AI Knowledge").unwrap();

        let ts: chrono::DateTime<chrono::Utc> =
            chrono::DateTime::parse_from_rfc3339("2024-03-15T10:00:00Z")
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

        let path =
            sink.build_document_path("opencode", "sess-1", Some("Fix: path/to/file issue"), None);
        assert!(
            !path.contains("path/to/file"),
            "Title / should be sanitized: {}",
            path
        );
        assert!(path.contains("path"));
    }

    #[test]
    fn embedded_sink_without_token_and_uses_one_content_notebook() {
        let sink = SiYuanSink::embedded("http://127.0.0.1:6806", "AI Knowledge").unwrap();
        assert!(sink.token.is_none());
        assert_eq!(sink.notebook_name, "AI Knowledge");
        assert_eq!(sink.session_notebook_name, sink.notebook_name);
    }

    #[test]
    fn external_sink_with_token_uses_one_content_notebook() {
        let config = crate::config::SiYuanConfig {
            token: "my-test-token".to_string(),
            notebook_name: "Company Knowledge".to_string(),
            session_notebook_name: "Legacy Session Archive".to_string(),
            ..Default::default()
        };
        let sink = SiYuanSink::new(config).unwrap();
        assert_eq!(sink.token.as_deref(), Some("my-test-token"));
        assert_eq!(sink.notebook_name, "Company Knowledge");
        assert_eq!(sink.session_notebook_name, sink.notebook_name);
    }

    #[test]
    fn external_sink_empty_token_becomes_none() {
        let config = crate::config::SiYuanConfig::default();
        let sink = SiYuanSink::new(config).unwrap();
        assert!(sink.token.is_none());
        assert_eq!(sink.session_notebook_name, sink.notebook_name);
    }
}
