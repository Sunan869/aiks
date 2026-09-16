use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::renderer::knowledge::{render_knowledge_item_md, KnowledgeItemDoc};
use crate::sink::v41::{KnowledgeBindingAttrs, SiYuanContentStore};
use crate::sink::SiYuanSink;
use crate::storage::{KnowledgeSyncRepo, StateDb};

use super::{KnowledgeListFilter, KnowledgeRecord, KnowledgeService};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationSnapshot {
    pub target_id: Option<String>,
    pub synced_hash: Option<String>,
    pub target_hash: Option<String>,
    pub local_hash: String,
    pub remote_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationDecision {
    Create,
    Reuse { doc_id: String },
    Update { doc_id: String },
    Conflict { doc_id: String },
}

pub fn decide_migration(snapshot: &MigrationSnapshot) -> MigrationDecision {
    let Some(doc_id) = snapshot.target_id.clone() else {
        return MigrationDecision::Create;
    };

    let (Some(synced_hash), Some(target_hash)) = (
        snapshot.synced_hash.as_deref(),
        snapshot.target_hash.as_deref(),
    ) else {
        return MigrationDecision::Reuse { doc_id };
    };

    let Some(remote_hash) = snapshot.remote_hash.as_deref() else {
        return MigrationDecision::Conflict { doc_id };
    };

    let local_changed = snapshot.local_hash.as_str() != synced_hash;
    let remote_changed = remote_hash != target_hash;

    if remote_changed {
        MigrationDecision::Conflict { doc_id }
    } else if local_changed {
        MigrationDecision::Update { doc_id }
    } else {
        MigrationDecision::Reuse { doc_id }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentMigrationStats {
    pub total: usize,
    pub migrated: usize,
    pub reused: usize,
    pub conflicts: usize,
    pub failed: usize,
}

pub struct ContentMigrationService<'a> {
    db: &'a StateDb,
}

impl<'a> ContentMigrationService<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    pub async fn migrate(&self, sink: &SiYuanSink) -> anyhow::Result<ContentMigrationStats> {
        let items = self.load_items()?;
        let mut stats = ContentMigrationStats {
            total: items.len(),
            ..ContentMigrationStats::default()
        };
        let store = SiYuanContentStore::new(sink);

        for item in items {
            if let Some(status) = self.completed_status(&item.id)? {
                match status.as_str() {
                    "migrated" => stats.migrated += 1,
                    "reused" => stats.reused += 1,
                    _ => {}
                }
                if matches!(status.as_str(), "migrated" | "reused") {
                    continue;
                }
            }

            match self.migrate_item(sink, &store, &item).await {
                Ok(MigrationDecision::Create | MigrationDecision::Update { .. }) => {
                    stats.migrated += 1;
                }
                Ok(MigrationDecision::Reuse { .. }) => stats.reused += 1,
                Ok(MigrationDecision::Conflict { .. }) => stats.conflicts += 1,
                Err(error) => {
                    stats.failed += 1;
                    self.record_migration(
                        &item.id,
                        "failed",
                        self.bound_doc_id(&item.id)?.as_deref(),
                        None,
                        None,
                        Some(&error.to_string()),
                    )?;
                }
            }
        }

        Ok(stats)
    }

    fn load_items(&self) -> anyhow::Result<Vec<KnowledgeRecord>> {
        let service = KnowledgeService::new(self.db);
        let mut items = Vec::new();
        let mut offset = 0usize;

        loop {
            let page = service.list(KnowledgeListFilter {
                limit: Some(200),
                offset: Some(offset),
                ..KnowledgeListFilter::default()
            })?;
            let page_len = page.items.len();
            items.extend(page.items);
            offset += page_len;
            if page_len == 0 || offset >= page.total {
                break;
            }
        }

        Ok(items)
    }

    async fn migrate_item(
        &self,
        sink: &SiYuanSink,
        store: &SiYuanContentStore<'_>,
        item: &KnowledgeRecord,
    ) -> anyhow::Result<MigrationDecision> {
        let markdown = self.render_legacy_item(item)?;
        let local_hash = hex::encode(Sha256::digest(markdown.as_bytes()));
        let sync_repo = KnowledgeSyncRepo::new(self.db);
        let existing = sync_repo.find(&item.id, "siyuan")?;
        let bound_doc_id = self.bound_doc_id(&item.id)?;
        let candidate_doc_id = bound_doc_id.or_else(|| {
            existing
                .as_ref()
                .and_then(|target| target.target_id.clone())
        });

        let target_id = match candidate_doc_id {
            Some(doc_id) if store.document_exists(&doc_id).await? => Some(doc_id),
            _ => None,
        };
        let remote_hash = match target_id.as_deref() {
            Some(doc_id) => store.document_hash(doc_id).await.ok(),
            None => None,
        };
        let snapshot = MigrationSnapshot {
            target_id,
            synced_hash: existing
                .as_ref()
                .and_then(|target| target.synced_hash.clone()),
            target_hash: existing
                .as_ref()
                .and_then(|target| target.target_hash.clone()),
            local_hash: local_hash.clone(),
            remote_hash: remote_hash.clone(),
        };
        let decision = decide_migration(&snapshot);
        let path = sink.build_knowledge_path(&item.category, &item.id, &item.title);
        let attrs = KnowledgeBindingAttrs {
            knowledge_id: item.id.clone(),
            source_type: item.source_type.clone(),
            managed_by: item.managed_by.clone(),
            session_id: item.session_external_id.clone(),
            project: item.project_name.clone(),
            category: item.category.clone(),
            generated_hash: local_hash.clone(),
        };

        match &decision {
            MigrationDecision::Create => {
                let doc_id = store.create_knowledge_document(&path, &markdown).await?;
                sync_repo.record_target_doc(&item.id, "siyuan", &doc_id, &path)?;
                store.set_knowledge_binding_attrs(&doc_id, &attrs).await?;
                let current_remote_hash = store.document_hash(&doc_id).await?;
                sync_repo.mark_synced(&item.id, "siyuan", &doc_id, &path, &local_hash)?;
                sync_repo.record_target_hash(&item.id, "siyuan", &current_remote_hash)?;
                self.mark_binding(
                    &item.id,
                    &doc_id,
                    &local_hash,
                    Some(&current_remote_hash),
                    "migrated",
                    None,
                )?;
                self.record_migration(
                    &item.id,
                    "migrated",
                    Some(&doc_id),
                    Some(&local_hash),
                    Some(&current_remote_hash),
                    None,
                )?;
            }
            MigrationDecision::Update { doc_id } => {
                store.update_knowledge_document(doc_id, &markdown).await?;
                store.set_knowledge_binding_attrs(doc_id, &attrs).await?;
                let current_remote_hash = store.document_hash(doc_id).await?;
                sync_repo.mark_synced(&item.id, "siyuan", doc_id, &path, &local_hash)?;
                sync_repo.record_target_hash(&item.id, "siyuan", &current_remote_hash)?;
                self.mark_binding(
                    &item.id,
                    doc_id,
                    &local_hash,
                    Some(&current_remote_hash),
                    "migrated",
                    None,
                )?;
                self.record_migration(
                    &item.id,
                    "migrated",
                    Some(doc_id),
                    Some(&local_hash),
                    Some(&current_remote_hash),
                    None,
                )?;
            }
            MigrationDecision::Reuse { doc_id } => {
                let unknown_baseline = snapshot.synced_hash.is_none() || snapshot.target_hash.is_none();
                let mut reuse_attrs = attrs;
                if unknown_baseline {
                    reuse_attrs.managed_by = "user".into();
                }
                store
                    .set_knowledge_binding_attrs(doc_id, &reuse_attrs)
                    .await?;
                self.mark_binding(
                    &item.id,
                    doc_id,
                    &local_hash,
                    remote_hash.as_deref(),
                    "reused",
                    if unknown_baseline { Some("user") } else { None },
                )?;
                self.record_migration(
                    &item.id,
                    "reused",
                    Some(doc_id),
                    Some(&local_hash),
                    remote_hash.as_deref(),
                    None,
                )?;
            }
            MigrationDecision::Conflict { doc_id } => {
                sync_repo.mark_conflict(&item.id, "siyuan")?;
                self.mark_binding(
                    &item.id,
                    doc_id,
                    &local_hash,
                    remote_hash.as_deref(),
                    "conflict",
                    Some("user"),
                )?;
                self.record_migration(
                    &item.id,
                    "conflict",
                    Some(doc_id),
                    Some(&local_hash),
                    remote_hash.as_deref(),
                    None,
                )?;
            }
        }

        Ok(decision)
    }

    fn render_legacy_item(&self, item: &KnowledgeRecord) -> anyhow::Result<String> {
        let source_display = match item.source.as_deref().unwrap_or("Manual") {
            "claude_code" => "Claude",
            "codex" => "Codex",
            "gemini_cli" => "Gemini",
            "opencode" => "OpenCode",
            other => other,
        };
        let session_ext_id = item
            .session_external_id
            .clone()
            .unwrap_or_else(|| format!("manual:{}", item.id));
        let session_doc_id = item
            .source_session_id
            .and_then(|session_id| self.session_doc_id(session_id).ok().flatten());
        let tags_json = serde_json::to_string(&item.tags)?;
        let doc = KnowledgeItemDoc {
            knowledge_id: &item.id,
            title: &item.title,
            category: &item.category,
            project_name: item.project_name.as_deref(),
            summary: &item.summary,
            content: &item.content,
            tags_json: &tags_json,
            confidence: item.confidence,
            source_display,
            session_ext_id: &session_ext_id,
            session_title: item.session_title.as_deref(),
            session_doc_id: session_doc_id.as_deref(),
        };
        Ok(render_knowledge_item_md(&doc))
    }

    fn session_doc_id(&self, session_id: i64) -> anyhow::Result<Option<String>> {
        let value = self
            .db
            .conn()
            .query_row(
                "SELECT siyuan_doc_id FROM source_session WHERE id = ?1",
                params![session_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        Ok(value.flatten())
    }

    fn bound_doc_id(&self, knowledge_id: &str) -> anyhow::Result<Option<String>> {
        let value = self
            .db
            .conn()
            .query_row(
                "SELECT siyuan_doc_id FROM knowledge_item WHERE id = ?1",
                params![knowledge_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        Ok(value.flatten())
    }

    fn completed_status(&self, knowledge_id: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .db
            .conn()
            .query_row(
                "SELECT status FROM content_migration WHERE entity_type = 'knowledge' AND entity_id = ?1",
                params![knowledge_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn mark_binding(
        &self,
        knowledge_id: &str,
        doc_id: &str,
        generated_hash: &str,
        current_remote_hash: Option<&str>,
        migration_status: &str,
        managed_by_override: Option<&str>,
    ) -> anyhow::Result<()> {
        if let Some(managed_by) = managed_by_override {
            self.db.conn().execute(
                "UPDATE knowledge_item
                 SET siyuan_doc_id = ?2, generated_hash = ?3, current_remote_hash = ?4,
                     migration_status = ?5, managed_by = ?6
                 WHERE id = ?1",
                params![
                    knowledge_id,
                    doc_id,
                    generated_hash,
                    current_remote_hash,
                    migration_status,
                    managed_by
                ],
            )?;
        } else {
            self.db.conn().execute(
                "UPDATE knowledge_item
                 SET siyuan_doc_id = ?2, generated_hash = ?3, current_remote_hash = ?4,
                     migration_status = ?5
                 WHERE id = ?1",
                params![
                    knowledge_id,
                    doc_id,
                    generated_hash,
                    current_remote_hash,
                    migration_status
                ],
            )?;
        }
        Ok(())
    }

    fn record_migration(
        &self,
        knowledge_id: &str,
        status: &str,
        target_doc_id: Option<&str>,
        source_hash: Option<&str>,
        target_hash: Option<&str>,
        error_message: Option<&str>,
    ) -> anyhow::Result<()> {
        self.db.conn().execute(
            "INSERT INTO content_migration
             (entity_type, entity_id, status, target_doc_id, source_hash, target_hash,
              error_message, updated_at)
             VALUES ('knowledge', ?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(entity_type, entity_id) DO UPDATE SET
               status = excluded.status,
               target_doc_id = excluded.target_doc_id,
               source_hash = excluded.source_hash,
               target_hash = excluded.target_hash,
               error_message = excluded.error_message,
               updated_at = excluded.updated_at",
            params![
                knowledge_id,
                status,
                target_doc_id,
                source_hash,
                target_hash,
                error_message,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }
}
