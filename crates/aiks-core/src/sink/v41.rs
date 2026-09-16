use std::collections::HashMap;

use sha2::{Digest, Sha256};

use super::SiYuanSink;

pub const KNOWLEDGE_ROOT: &str = "/20 Knowledge";
pub const SESSION_ROOT: &str = "/10 AI Sessions";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeBindingAttrs {
    pub knowledge_id: String,
    pub source_type: String,
    pub managed_by: String,
    pub session_id: Option<String>,
    pub project: Option<String>,
    pub category: String,
    pub generated_hash: String,
}

impl KnowledgeBindingAttrs {
    pub fn to_block_attrs(&self) -> HashMap<String, String> {
        let mut attrs = HashMap::from([
            ("custom-aiks-managed".to_string(), "true".to_string()),
            ("custom-aiks-kind".to_string(), "knowledge".to_string()),
            ("custom-aiks-id".to_string(), self.knowledge_id.clone()),
            (
                "custom-aiks-source-type".to_string(),
                self.source_type.clone(),
            ),
            (
                "custom-aiks-managed-by".to_string(),
                self.managed_by.clone(),
            ),
            ("custom-aiks-category".to_string(), self.category.clone()),
            (
                "custom-aiks-generated-hash".to_string(),
                self.generated_hash.clone(),
            ),
        ]);

        if let Some(session_id) = self.session_id.as_ref() {
            attrs.insert("custom-aiks-session-id".into(), session_id.clone());
        }
        if let Some(project) = self.project.as_ref() {
            attrs.insert("custom-aiks-project".into(), project.clone());
        }

        attrs
    }
}

pub struct SiYuanContentStore<'a> {
    sink: &'a SiYuanSink,
}

impl<'a> SiYuanContentStore<'a> {
    pub fn new(sink: &'a SiYuanSink) -> Self {
        Self { sink }
    }

    pub const fn knowledge_root() -> &'static str {
        KNOWLEDGE_ROOT
    }

    pub const fn session_root() -> &'static str {
        SESSION_ROOT
    }

    pub async fn ensure_content_notebook(&self) -> anyhow::Result<String> {
        self.sink.ensure_notebook().await
    }

    pub async fn create_knowledge_document(
        &self,
        path: &str,
        markdown: &str,
    ) -> anyhow::Result<String> {
        let notebook_id = self.ensure_content_notebook().await?;
        self.sink
            .create_document(&notebook_id, path, markdown)
            .await
    }

    pub async fn move_documents_to_content_notebook(
        &self,
        from_ids: &[String],
        to_parent_id: &str,
    ) -> anyhow::Result<()> {
        let notebook_id = self.ensure_content_notebook().await?;
        self.sink
            .move_docs(from_ids, &notebook_id, to_parent_id)
            .await
    }

    pub async fn document_hash(&self, doc_id: &str) -> anyhow::Result<String> {
        let markdown = self.sink.get_document_markdown(doc_id).await?;
        Ok(hex::encode(Sha256::digest(markdown.as_bytes())))
    }

    pub async fn set_knowledge_binding_attrs(
        &self,
        doc_id: &str,
        attrs: &KnowledgeBindingAttrs,
    ) -> anyhow::Result<()> {
        let block_attrs = attrs.to_block_attrs();
        self.sink.set_block_attrs(doc_id, &block_attrs).await
    }
}
