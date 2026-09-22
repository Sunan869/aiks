//! One business writer and one snapshot worker. No provider discovery or HOME scan.
use super::{
    query::{self, Page},
    validate_submission, LocalContext, ServiceError, ServiceStore, SnapshotReceipt,
    SnapshotSubmission,
};
use crate::{
    ai::{config::AiModelConfig, ModelService},
    config::SiYuanConfig,
    knowledge::ai_assist::{AiAssistOperation, AiAssistRequest, AiAssistService},
    model::SourceKind,
    pipeline::{EmbeddingConfig, PipelineWorker},
    search::{UnifiedSearchFilter, UnifiedSearchService},
    sink::SiYuanSink,
    storage::StateDb,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Clone)]
pub struct ServiceRuntimeConfig {
    pub database: PathBuf,
    pub ai: AiModelConfig,
    pub embedding: EmbeddingConfig,
    pub siyuan: SiYuanConfig,
}

pub struct ServiceRuntime {
    store: Arc<ServiceStore>,
    worker: PipelineWorker,
    models: Arc<ModelService>,
    content: SiYuanSink,
    closing: AtomicBool,
}

impl ServiceRuntime {
    pub async fn open(config: ServiceRuntimeConfig) -> anyhow::Result<Self> {
        anyhow::ensure!(
            config.database.is_absolute(),
            "Service requires an explicit absolute database path"
        );
        let store = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let db = Arc::new(StateDb::open_exclusive(&config.database)?);
            Ok(Arc::new(ServiceStore::open(db)?))
        })
        .await??;
        let models = Arc::new(ModelService::new(
            config.ai.clone(),
            config.embedding.clone(),
        )?);
        let content = SiYuanSink::service_reader(config.siyuan)?;
        let worker =
            PipelineWorker::start_from_snapshots(store.db().clone(), config.ai, config.embedding);
        Ok(Self {
            store,
            worker,
            models,
            content,
            closing: AtomicBool::new(false),
        })
    }

    pub fn context(&self) -> LocalContext {
        self.store.local_context()
    }

    pub fn capabilities(&self) -> Value {
        let ctx = self.context();
        json!({"api_version":1,"instance_id":ctx.instance_id(),"space_id":ctx.space_id(),
            "mode":"personal","snapshots":true,"keyword_search":true,
            "semantic_search":self.models.embedding_config().enabled,
            "ai_assist":self.models.llm_config().enabled,"team":false,"content_write":false,"rag":false})
    }

    async fn blocking<T, F>(&self, f: F) -> Result<T, ServiceError>
    where
        T: Send + 'static,
        F: FnOnce(&ServiceStore, &LocalContext) -> Result<T, ServiceError> + Send + 'static,
    {
        if self.closing.load(Ordering::Acquire) {
            return Err(ServiceError::Unavailable);
        }
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || f(&store, &store.local_context()))
            .await
            .map_err(|_| ServiceError::Internal)?
    }

    pub async fn register_source(
        &self,
        source: SourceKind,
        key: String,
    ) -> Result<String, ServiceError> {
        self.blocking(move |store, ctx| store.register_source(ctx, source, &key))
            .await
    }

    /// bool is receipt creation (202), not work creation; both are decided in the
    /// same transaction. The old Core accept API retains its work-queued bool.
    pub async fn accept(
        &self,
        request: SnapshotSubmission,
    ) -> Result<(SnapshotReceipt, bool), ServiceError> {
        let result = self
            .blocking(move |store, ctx| {
                let checked = validate_submission(&request)?;
                let (receipt, _queued, created) = store.accept_with_flags(ctx, &checked)?;
                Ok((receipt, created))
            })
            .await?;
        self.worker.wake();
        Ok(result)
    }

    pub async fn sessions(&self, page: Page) -> Result<Value, ServiceError> {
        self.blocking(move |store, ctx| query::sessions(store.db(), ctx, page))
            .await
    }
    pub async fn session(&self, id: String) -> Result<Value, ServiceError> {
        self.blocking(move |store, ctx| query::session(store.db(), ctx, &id))
            .await
    }
    pub async fn receipt(&self, id: String) -> Result<SnapshotReceipt, ServiceError> {
        self.blocking(move |store, ctx| query::receipt(store.db(), ctx, &id))
            .await
    }
    pub async fn job(&self, id: String) -> Result<Value, ServiceError> {
        self.blocking(move |store, ctx| query::job(store.db(), ctx, &id))
            .await
    }
    pub async fn knowledge_list(&self, page: Page) -> Result<Value, ServiceError> {
        self.blocking(move |store, ctx| query::knowledge_list(store.db(), ctx, page))
            .await
    }
    async fn knowledge_metadata(&self, id: String) -> Result<query::KnowledgeView, ServiceError> {
        self.blocking(move |store, ctx| query::knowledge(store.db(), ctx, &id))
            .await
    }
    pub async fn knowledge(&self, id: String) -> Result<query::KnowledgeView, ServiceError> {
        let mut result = self.knowledge_metadata(id.clone()).await?;
        if let Some(doc_id) = &result.doc_id {
            let body = self
                .content
                .get_document_markdown_bounded(doc_id, query::MAX_CONTENT_BYTES)
                .await
                .map_err(|_| ServiceError::ContentUnavailable)?;
            let checked = self.knowledge_metadata(id).await?;
            if checked.generation != result.generation || checked.doc_id != result.doc_id {
                return Err(ServiceError::Conflict);
            }
            result.content = Some(body);
        }
        Ok(result)
    }
    pub async fn assist(
        &self,
        id: String,
        operation: AiAssistOperation,
    ) -> Result<Value, ServiceError> {
        // Authorize the resource first, without fetching remote content when off.
        self.knowledge_metadata(id.clone()).await?;
        if !self.models.llm_config().enabled {
            return Err(ServiceError::AiDisabled);
        }
        let view = self.knowledge(id.clone()).await?;
        if view.stale {
            return Err(ServiceError::Conflict);
        }
        if view.doc_id.is_none() {
            return Err(ServiceError::InvalidInput);
        }
        let request = AiAssistRequest {
            operation,
            title: view.title,
            content: view.content.ok_or(ServiceError::Unavailable)?,
            existing_summary: Some(view.summary),
            existing_tags: view.tags,
            existing_category: Some(view.category),
        };
        let suggestion = tokio::time::timeout(
            Duration::from_secs(20),
            AiAssistService::new(self.models.clone()).suggest(request),
        )
        .await
        .map_err(|_| ServiceError::Unavailable)?
        .map_err(|_| ServiceError::Unavailable)?;
        let current = self.knowledge_metadata(id).await?;
        if current.generation != view.generation || current.doc_id != view.doc_id {
            return Err(ServiceError::Conflict);
        }
        Ok(json!({"revision":current.revision,"suggestion":suggestion,"saved":false}))
    }
    pub async fn search(
        &self,
        query: String,
        limit: usize,
        filter: UnifiedSearchFilter,
    ) -> Result<Value, ServiceError> {
        if query.trim().is_empty() || query.chars().count() > 4096 || limit == 0 || limit > 100 {
            return Err(ServiceError::InvalidInput);
        }
        for value in [&filter.project, &filter.source].into_iter().flatten() {
            if value.len() > 4096 {
                return Err(ServiceError::InvalidInput);
            }
        }
        let before = self
            .blocking(|store, _| query::epoch(&store.db().conn()))
            .await?;
        let search = UnifiedSearchService::scoped(
            self.store.db().clone(),
            self.models.clone(),
            self.context(),
        );
        let results = search
            .search(&query, limit, filter)
            .await
            .map_err(|_| ServiceError::Unavailable)?;
        self.blocking(move |store, ctx| query::search_response(store.db(), ctx, before, results))
            .await
    }
    pub async fn shutdown(&self, grace: Duration) -> anyhow::Result<()> {
        self.closing.store(true, Ordering::Release);
        self.worker.shutdown(grace).await
    }
}
