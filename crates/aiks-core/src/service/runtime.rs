//! One business writer and one snapshot worker. No provider discovery or HOME scan.
use super::{
    ingestion::{accept_with_flags_for, register_source_for},
    query::{self, Page},
    validate_submission, LocalContext, RequestContext, ServiceError, ServiceStore, SnapshotReceipt,
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
    team::{
        ContentOperation, ContentWorker, DirectoryEntry, GrantInput, ImportReceipt, ManagedAsset,
        ShareState, TeamStore,
    },
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct ServiceRuntimeConfig {
    pub database: PathBuf,
    pub ai: AiModelConfig,
    pub embedding: EmbeddingConfig,
    pub siyuan: SiYuanConfig,
}

#[derive(Clone)]
enum RuntimeMode {
    Personal(Arc<ServiceStore>),
    Team(Arc<TeamStore>),
}

impl RuntimeMode {
    fn db(&self) -> Arc<StateDb> {
        match self {
            Self::Personal(store) => store.db().clone(),
            Self::Team(store) => store.db().clone(),
        }
    }

    fn validate(&self, ctx: &RequestContext) -> Result<(), ServiceError> {
        match (self, ctx) {
            (Self::Personal(store), RequestContext::Personal(request))
                if *request == store.local_context() =>
            {
                Ok(())
            }
            (Self::Team(store), RequestContext::Team(request))
                if request.company_id() == store.company_id()
                    && request.instance_id() == store.instance_id() =>
            {
                Ok(())
            }
            _ => Err(ServiceError::Unauthorized),
        }
    }
}

pub struct ServiceRuntime {
    mode: RuntimeMode,
    worker: PipelineWorker,
    models: Arc<ModelService>,
    content: Arc<SiYuanSink>,
    content_worker: Option<ContentWorker>,
    closing: AtomicBool,
}

impl ServiceRuntime {
    pub async fn open(config: ServiceRuntimeConfig) -> anyhow::Result<Self> {
        anyhow::ensure!(
            config.database.is_absolute(),
            "Service requires an explicit absolute database path"
        );
        let database = config.database.clone();
        let store = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let db = Arc::new(StateDb::open_exclusive(&database)?);
            Ok(Arc::new(ServiceStore::open(db)?))
        })
        .await??;
        Self::build(RuntimeMode::Personal(store), config)
    }

    /// Construct the shared business runtime on an already-bound company DB.
    /// Binding happens before this call so a team database can never be opened
    /// through the personal identity initializer. No listener is enabled here.
    pub fn open_team_bound(
        store: Arc<TeamStore>,
        config: ServiceRuntimeConfig,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            config.database.is_absolute(),
            "Service requires an explicit absolute database path"
        );
        anyhow::ensure!(
            store.db().has_exclusive_lease(),
            "Team store must own the database"
        );
        Self::build(RuntimeMode::Team(store), config)
    }

    fn build(mode: RuntimeMode, config: ServiceRuntimeConfig) -> anyhow::Result<Self> {
        let models = Arc::new(ModelService::new(
            config.ai.clone(),
            config.embedding.clone(),
        )?);
        let content = Arc::new(SiYuanSink::service_reader(config.siyuan)?);
        let db = mode.db();
        let worker = PipelineWorker::start_from_snapshots(db, config.ai, config.embedding);
        let content_worker = match &mode {
            RuntimeMode::Team(store) => Some(ContentWorker::start(
                store.clone(),
                content.clone(),
                models.clone(),
            )),
            RuntimeMode::Personal(_) => None,
        };
        Ok(Self {
            mode,
            worker,
            models,
            content,
            content_worker,
            closing: AtomicBool::new(false),
        })
    }

    pub fn context(&self) -> LocalContext {
        match &self.mode {
            RuntimeMode::Personal(store) => store.local_context(),
            RuntimeMode::Team(_) => panic!("personal context requested from team runtime"),
        }
    }

    fn personal_context(&self) -> Result<RequestContext, ServiceError> {
        match &self.mode {
            RuntimeMode::Personal(store) => Ok(RequestContext::Personal(store.local_context())),
            RuntimeMode::Team(_) => Err(ServiceError::Unauthorized),
        }
    }

    pub fn capabilities(&self) -> Value {
        match &self.mode {
            RuntimeMode::Personal(store) => {
                let ctx = store.local_context();
                json!({"api_version":1,"instance_id":ctx.instance_id(),"space_id":ctx.space_id(),
                    "mode":"personal","snapshots":true,"keyword_search":true,
                    "semantic_search":self.models.embedding_config().enabled,
                    "ai_assist":self.models.llm_config().enabled,"team":false,"content_write":false,"rag":false})
            }
            RuntimeMode::Team(store) => json!({"api_version":1,"instance_id":store.instance_id(),
                "mode":"team","snapshots":true,"keyword_search":true,
                "semantic_search":self.models.embedding_config().enabled,
                "ai_assist":self.models.llm_config().enabled,"team":true,"content_write":true,"rag":false}),
        }
    }

    async fn blocking_for<T, F>(&self, ctx: RequestContext, action: F) -> Result<T, ServiceError>
    where
        T: Send + 'static,
        F: FnOnce(Arc<StateDb>, RequestContext, u64) -> Result<T, ServiceError> + Send + 'static,
    {
        if self.closing.load(Ordering::Acquire) {
            return Err(ServiceError::Unavailable);
        }
        self.mode.validate(&ctx)?;
        let db = self.mode.db();
        tokio::task::spawn_blocking(move || action(db, ctx, now()?))
            .await
            .map_err(|_| ServiceError::Internal)?
    }

    pub async fn register_source(
        &self,
        source: SourceKind,
        key: String,
    ) -> Result<String, ServiceError> {
        let ctx = self.personal_context()?;
        self.register_source_for(&ctx, source, key).await
    }

    pub async fn register_source_for(
        &self,
        ctx: &RequestContext,
        source: SourceKind,
        key: String,
    ) -> Result<String, ServiceError> {
        let ctx = ctx.clone();
        self.blocking_for(ctx, move |db, ctx, at| {
            register_source_for(&db, &ctx, source, &key, at)
        })
        .await
    }

    /// bool is receipt creation (202), not work creation; both are decided in the
    /// same transaction. The old Core accept API retains its work-queued bool.
    pub async fn accept(
        &self,
        request: SnapshotSubmission,
    ) -> Result<(SnapshotReceipt, bool), ServiceError> {
        let ctx = self.personal_context()?;
        self.accept_for(&ctx, request).await
    }

    pub async fn accept_for(
        &self,
        ctx: &RequestContext,
        request: SnapshotSubmission,
    ) -> Result<(SnapshotReceipt, bool), ServiceError> {
        let ctx = ctx.clone();
        let result = self
            .blocking_for(ctx, move |db, ctx, at| {
                let checked = validate_submission(&request)?;
                let (receipt, _queued, created) = accept_with_flags_for(&db, &ctx, &checked, at)?;
                Ok((receipt, created))
            })
            .await?;
        self.worker.wake();
        Ok(result)
    }

    pub async fn sessions(&self, page: Page) -> Result<Value, ServiceError> {
        let ctx = self.personal_context()?;
        self.sessions_for(&ctx, page).await
    }

    pub async fn sessions_for(
        &self,
        ctx: &RequestContext,
        page: Page,
    ) -> Result<Value, ServiceError> {
        let ctx = ctx.clone();
        self.blocking_for(ctx, move |db, ctx, at| {
            query::sessions_for(&db, &ctx, at, page)
        })
        .await
    }

    pub async fn session(&self, id: String) -> Result<Value, ServiceError> {
        let ctx = self.personal_context()?;
        self.session_for(&ctx, id).await
    }

    pub async fn session_for(
        &self,
        ctx: &RequestContext,
        id: String,
    ) -> Result<Value, ServiceError> {
        let ctx = ctx.clone();
        self.blocking_for(ctx, move |db, ctx, at| {
            query::session_for(&db, &ctx, at, &id)
        })
        .await
    }

    pub async fn receipt(&self, id: String) -> Result<SnapshotReceipt, ServiceError> {
        let ctx = self.personal_context()?;
        self.receipt_for(&ctx, id).await
    }

    pub async fn receipt_for(
        &self,
        ctx: &RequestContext,
        id: String,
    ) -> Result<SnapshotReceipt, ServiceError> {
        let ctx = ctx.clone();
        self.blocking_for(ctx, move |db, ctx, at| {
            query::receipt_for(&db, &ctx, at, &id)
        })
        .await
    }

    pub async fn job(&self, id: String) -> Result<Value, ServiceError> {
        let ctx = self.personal_context()?;
        self.job_for(&ctx, id).await
    }

    pub async fn job_for(&self, ctx: &RequestContext, id: String) -> Result<Value, ServiceError> {
        let ctx = ctx.clone();
        self.blocking_for(ctx, move |db, ctx, at| query::job_for(&db, &ctx, at, &id))
            .await
    }

    pub async fn knowledge_list(&self, page: Page) -> Result<Value, ServiceError> {
        let ctx = self.personal_context()?;
        self.knowledge_list_for(&ctx, page).await
    }

    pub async fn knowledge_list_for(
        &self,
        ctx: &RequestContext,
        page: Page,
    ) -> Result<Value, ServiceError> {
        let ctx = ctx.clone();
        self.blocking_for(ctx, move |db, ctx, at| {
            query::knowledge_list_for(&db, &ctx, at, page)
        })
        .await
    }

    async fn knowledge_metadata_for(
        &self,
        ctx: &RequestContext,
        id: String,
    ) -> Result<query::KnowledgeView, ServiceError> {
        let ctx = ctx.clone();
        self.blocking_for(ctx, move |db, ctx, at| {
            query::knowledge_for(&db, &ctx, at, &id)
        })
        .await
    }

    pub async fn knowledge(&self, id: String) -> Result<query::KnowledgeView, ServiceError> {
        let ctx = self.personal_context()?;
        self.knowledge_for(&ctx, id).await
    }

    pub async fn knowledge_for(
        &self,
        ctx: &RequestContext,
        id: String,
    ) -> Result<query::KnowledgeView, ServiceError> {
        let mut result = self.knowledge_metadata_for(ctx, id.clone()).await?;
        if let Some(doc_id) = &result.doc_id {
            let body = self
                .content
                .get_document_markdown_bounded(doc_id, query::MAX_CONTENT_BYTES)
                .await
                .map_err(|_| ServiceError::ContentUnavailable)?;
            let checked = self.knowledge_metadata_for(ctx, id).await?;
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
        let ctx = self.personal_context()?;
        self.assist_for(&ctx, id, operation).await
    }

    pub async fn assist_for(
        &self,
        ctx: &RequestContext,
        id: String,
        operation: AiAssistOperation,
    ) -> Result<Value, ServiceError> {
        self.knowledge_metadata_for(ctx, id.clone()).await?;
        if !self.models.llm_config().enabled {
            return Err(ServiceError::AiDisabled);
        }
        let view = self.knowledge_for(ctx, id.clone()).await?;
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
        let current = self.knowledge_metadata_for(ctx, id).await?;
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
        let ctx = self.personal_context()?;
        self.search_for(&ctx, query, limit, filter).await
    }

    pub async fn search_for(
        &self,
        ctx: &RequestContext,
        query_text: String,
        limit: usize,
        filter: UnifiedSearchFilter,
    ) -> Result<Value, ServiceError> {
        if query_text.trim().is_empty()
            || query_text.chars().count() > 4096
            || limit == 0
            || limit > 100
        {
            return Err(ServiceError::InvalidInput);
        }
        for value in [&filter.project, &filter.source].into_iter().flatten() {
            if value.len() > 4096 {
                return Err(ServiceError::InvalidInput);
            }
        }
        let before_ctx = ctx.clone();
        let (before, auth_at) = self
            .blocking_for(before_ctx, |db, ctx, at| {
                let mut conn = db.conn();
                let tx = conn.transaction()?;
                ctx.authorize_in_conn(&tx, at)?;
                Ok((query::epoch(&tx)?, at))
            })
            .await?;
        let search = UnifiedSearchService::scoped_for(
            self.mode.db(),
            self.models.clone(),
            ctx.clone(),
            auth_at,
        );
        let results = search
            .search(&query_text, limit, filter)
            .await
            .map_err(|_| ServiceError::Unavailable)?;
        let final_ctx = ctx.clone();
        self.blocking_for(final_ctx, move |db, ctx, at| {
            query::search_response_for(&db, &ctx, at, before, results)
        })
        .await
    }

    pub async fn import_knowledge_for(
        &self,
        ctx: &RequestContext,
        operation_id: String,
        title: String,
        markdown: String,
        source_fingerprint: String,
    ) -> Result<(ImportReceipt, bool), ServiceError> {
        let request = ctx.clone();
        let mode = self.mode.clone();
        self.blocking_for(request, move |_db, ctx, at| {
            let RuntimeMode::Team(store) = mode else {
                return Err(ServiceError::NotFound);
            };
            let team = ctx.team().ok_or(ServiceError::Unauthorized)?;
            Ok(store.import_knowledge(
                team,
                &operation_id,
                &title,
                &markdown,
                &source_fingerprint,
                at,
            )?)
        })
        .await
    }

    pub async fn directory_search_for(
        &self,
        ctx: &RequestContext,
        query: String,
        limit: usize,
    ) -> Result<Vec<DirectoryEntry>, ServiceError> {
        let request = ctx.clone();
        let mode = self.mode.clone();
        self.blocking_for(request, move |_db, ctx, at| {
            let RuntimeMode::Team(store) = mode else {
                return Err(ServiceError::NotFound);
            };
            let team = ctx.team().ok_or(ServiceError::Unauthorized)?;
            Ok(store.search_directory(team, &query, limit, at)?)
        })
        .await
    }

    pub async fn share_state_for(
        &self,
        ctx: &RequestContext,
        id: String,
    ) -> Result<ShareState, ServiceError> {
        let ctx = ctx.clone();
        let mode = self.mode.clone();
        self.blocking_for(ctx, move |_db, ctx, at| {
            let RuntimeMode::Team(store) = mode else {
                return Err(ServiceError::NotFound);
            };
            let team = ctx.team().ok_or(ServiceError::Unauthorized)?;
            Ok(store.list_grants(team, &id, at)?)
        })
        .await
    }

    pub async fn replace_shares_for(
        &self,
        ctx: &RequestContext,
        id: String,
        expected: u64,
        grants: Vec<GrantInput>,
    ) -> Result<u64, ServiceError> {
        let ctx = ctx.clone();
        let mode = self.mode.clone();
        self.blocking_for(ctx, move |_db, ctx, at| {
            let RuntimeMode::Team(store) = mode else {
                return Err(ServiceError::NotFound);
            };
            let team = ctx.team().ok_or(ServiceError::Unauthorized)?;
            Ok(store.replace_grants(team, &id, expected, &grants, at)?)
        })
        .await
    }

    pub async fn submit_content_for(
        &self,
        ctx: &RequestContext,
        id: String,
        operation_id: String,
        base_revision: u64,
        title: String,
        markdown: String,
    ) -> Result<ContentOperation, ServiceError> {
        let request = ctx.clone();
        let mode = self.mode.clone();
        let result = self
            .blocking_for(request, move |_db, ctx, at| {
                let RuntimeMode::Team(store) = mode else {
                    return Err(ServiceError::NotFound);
                };
                let team = ctx.team().ok_or(ServiceError::Unauthorized)?;
                Ok(store.enqueue_content_update(
                    team,
                    &id,
                    &operation_id,
                    base_revision,
                    &title,
                    &markdown,
                    at,
                )?)
            })
            .await?;
        if let Some(worker) = &self.content_worker {
            worker.wake();
        }
        Ok(result)
    }

    pub async fn publish_for(
        &self,
        ctx: &RequestContext,
        id: String,
        operation_id: String,
        base_revision: u64,
    ) -> Result<ContentOperation, ServiceError> {
        let request = ctx.clone();
        let mode = self.mode.clone();
        let result = self
            .blocking_for(request, move |_db, ctx, at| {
                let RuntimeMode::Team(store) = mode else {
                    return Err(ServiceError::NotFound);
                };
                let team = ctx.team().ok_or(ServiceError::Unauthorized)?;
                Ok(store.enqueue_publish(team, &id, &operation_id, base_revision, at)?)
            })
            .await?;
        if let Some(worker) = &self.content_worker {
            worker.wake();
        }
        Ok(result)
    }

    pub async fn content_operation_for(
        &self,
        ctx: &RequestContext,
        operation_id: String,
    ) -> Result<ContentOperation, ServiceError> {
        let request = ctx.clone();
        let mode = self.mode.clone();
        self.blocking_for(request, move |_db, ctx, at| {
            let RuntimeMode::Team(store) = mode else {
                return Err(ServiceError::NotFound);
            };
            let team = ctx.team().ok_or(ServiceError::Unauthorized)?;
            Ok(store.content_operation(team, &operation_id, at)?)
        })
        .await
    }

    pub async fn create_asset_for(
        &self,
        ctx: &RequestContext,
        knowledge_id: String,
        filename: String,
        content_type: String,
        bytes: Vec<u8>,
    ) -> Result<String, ServiceError> {
        let request = ctx.clone();
        let mode = self.mode.clone();
        self.blocking_for(request, move |_db, ctx, at| {
            let RuntimeMode::Team(store) = mode else {
                return Err(ServiceError::NotFound);
            };
            let team = ctx.team().ok_or(ServiceError::Unauthorized)?;
            Ok(store.create_managed_asset(
                team,
                &knowledge_id,
                &filename,
                &content_type,
                &bytes,
                at,
            )?)
        })
        .await
    }

    pub async fn managed_asset_for(
        &self,
        ctx: &RequestContext,
        knowledge_id: String,
        asset_id: String,
    ) -> Result<ManagedAsset, ServiceError> {
        let request = ctx.clone();
        let mode = self.mode.clone();
        self.blocking_for(request, move |_db, ctx, at| {
            let RuntimeMode::Team(store) = mode else {
                return Err(ServiceError::NotFound);
            };
            let team = ctx.team().ok_or(ServiceError::Unauthorized)?;
            Ok(store.managed_asset(team, &knowledge_id, &asset_id, at)?)
        })
        .await
    }

    pub async fn shutdown(&self, grace: Duration) -> anyhow::Result<()> {
        self.closing.store(true, Ordering::Release);
        if let Some(worker) = &self.content_worker {
            worker.shutdown(grace).await;
        }
        self.worker.shutdown(grace).await
    }
}

fn now() -> Result<u64, ServiceError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| ServiceError::Internal)
}
