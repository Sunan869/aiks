use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::renderer::knowledge::{render_knowledge_item_md, KnowledgeItemDoc};
use crate::sink::v41::{KnowledgeBindingAttrs, SiYuanContentStore};
use crate::sink::SiYuanSink;
use crate::storage::{KnowledgeSyncRepo, StateDb, SyncStatus};

use super::{CreateKnowledgeInput, KnowledgeRecord, KnowledgeService};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishKnowledgeResult {
    pub knowledge_id: String,
    pub outcome: String,
    pub target_id: Option<String>,
}

pub async fn create_manual_knowledge_in_siyuan(
    db: &StateDb,
    sink: &SiYuanSink,
    input: CreateKnowledgeInput,
) -> anyhow::Result<KnowledgeRecord> {
    let input = KnowledgeService::normalize_manual_input(input)?;
    let id = Uuid::new_v4().to_string();
    let category = input.category.as_deref().unwrap_or("general");
    let summary = input.summary.as_deref().unwrap_or_default();
    let session_ext_id = format!("manual:{id}");
    let tags_json = serde_json::to_string(&input.tags)?;
    let doc = KnowledgeItemDoc {
        knowledge_id: &id,
        title: &input.title,
        category,
        project_name: input.project_name.as_deref(),
        summary,
        content: &input.content,
        tags_json: &tags_json,
        confidence: 1.0,
        source_display: "Manual",
        session_ext_id: &session_ext_id,
        session_title: None,
        session_doc_id: None,
    };
    let markdown = render_knowledge_item_md(&doc);
    let generated_hash = hex::encode(Sha256::digest(markdown.as_bytes()));
    let path = sink.build_knowledge_path(category, &id, &input.title);
    let store = SiYuanContentStore::new(sink);

    let doc_id = store.create_knowledge_document(&path, &markdown).await?;
    let attrs = KnowledgeBindingAttrs {
        knowledge_id: id.clone(),
        source_type: "manual".into(),
        managed_by: "user".into(),
        session_id: None,
        project: input.project_name.clone(),
        category: category.to_string(),
        generated_hash: generated_hash.clone(),
    };
    store.set_knowledge_binding_attrs(&doc_id, &attrs).await?;
    let remote_hash = store.document_hash(&doc_id).await?;

    let service = KnowledgeService::new(db);
    let item =
        service.create_manual_bound(&id, input, &doc_id, &generated_hash, Some(&remote_hash))?;

    let repo = KnowledgeSyncRepo::new(db);
    repo.record_target_doc(&id, "siyuan", &doc_id, &path)?;
    repo.mark_synced(&id, "siyuan", &doc_id, &path, &generated_hash)?;
    repo.record_target_hash(&id, "siyuan", &remote_hash)?;

    Ok(item)
}

pub async fn publish_knowledge_to_siyuan(
    db: &StateDb,
    sink: &SiYuanSink,
    knowledge_id: &str,
    overwrite_conflict: bool,
) -> anyhow::Result<PublishKnowledgeResult> {
    let service = KnowledgeService::new(db);
    let item = service
        .get(knowledge_id)?
        .ok_or_else(|| anyhow::anyhow!("Knowledge item not found: {knowledge_id}"))?;
    if item.status != "active" {
        anyhow::bail!("Archived knowledge must be restored before publishing");
    }

    let source_display = item.source.as_deref().unwrap_or("Manual");
    let source_display = match source_display {
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
        .and_then(|session_id| session_doc_id(db, session_id).ok().flatten());
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
    let markdown = render_knowledge_item_md(&doc);
    let hash = hex::encode(Sha256::digest(markdown.as_bytes()));
    let path = sink.build_knowledge_path(&item.category, &item.id, &item.title);
    let store = SiYuanContentStore::new(sink);
    let repo = KnowledgeSyncRepo::new(db);
    let existing = repo.find(&item.id, "siyuan")?;

    if let Some(existing) = &existing {
        if existing.status == SyncStatus::Conflict && !overwrite_conflict {
            return Ok(PublishKnowledgeResult {
                knowledge_id: item.id,
                outcome: "conflict".into(),
                target_id: existing.target_id.clone(),
            });
        }
    }

    let candidate_remote_id = item
        .siyuan_doc_id
        .clone()
        .or_else(|| existing.as_ref().and_then(|value| value.target_id.clone()));
    let remote_id = match candidate_remote_id {
        Some(id) if store.document_exists(&id).await? => Some(id),
        _ => None,
    };

    if let (Some(existing), Some(id)) = (existing.as_ref(), remote_id.as_deref()) {
        if existing.status == SyncStatus::Synced && !overwrite_conflict {
            let Some(baseline) = existing.target_hash.as_deref() else {
                repo.mark_conflict(&item.id, "siyuan")?;
                let _ = service.bind_siyuan_document(&item.id, id, &hash, None, "conflict")?;
                return Ok(PublishKnowledgeResult {
                    knowledge_id: item.id,
                    outcome: "conflict".into(),
                    target_id: Some(id.to_string()),
                });
            };
            let remote_hash = store.document_hash(id).await?;
            if remote_hash != baseline {
                repo.mark_conflict(&item.id, "siyuan")?;
                let _ = service.bind_siyuan_document(
                    &item.id,
                    id,
                    &hash,
                    Some(&remote_hash),
                    "conflict",
                )?;
                return Ok(PublishKnowledgeResult {
                    knowledge_id: item.id,
                    outcome: "conflict".into(),
                    target_id: Some(id.to_string()),
                });
            }

            if existing.synced_hash.as_deref() == Some(&hash) {
                let _ = service.bind_siyuan_document(
                    &item.id,
                    id,
                    &hash,
                    Some(&remote_hash),
                    "migrated",
                )?;
                return Ok(PublishKnowledgeResult {
                    knowledge_id: item.id,
                    outcome: "unchanged".into(),
                    target_id: Some(id.to_string()),
                });
            }
        }
    }

    let (doc_id, outcome) = if let Some(id) = remote_id {
        store.update_knowledge_document(&id, &markdown).await?;
        repo.record_target_doc(&item.id, "siyuan", &id, &path)?;
        (id, "updated")
    } else {
        let id = store.create_knowledge_document(&path, &markdown).await?;
        repo.record_target_doc(&item.id, "siyuan", &id, &path)?;
        (id, "created")
    };

    let attrs = KnowledgeBindingAttrs {
        knowledge_id: item.id.clone(),
        source_type: item.source_type.clone(),
        managed_by: item.managed_by.clone(),
        session_id: item.session_external_id.clone(),
        project: item.project_name.clone(),
        category: item.category.clone(),
        generated_hash: hash.clone(),
    };
    store.set_knowledge_binding_attrs(&doc_id, &attrs).await?;
    repo.mark_synced(&item.id, "siyuan", &doc_id, &path, &hash)?;

    let remote_hash = store.document_hash(&doc_id).await?;
    repo.record_target_hash(&item.id, "siyuan", &remote_hash)?;
    let _ =
        service.bind_siyuan_document(&item.id, &doc_id, &hash, Some(&remote_hash), "migrated")?;

    Ok(PublishKnowledgeResult {
        knowledge_id: item.id,
        outcome: outcome.into(),
        target_id: Some(doc_id),
    })
}

fn session_doc_id(db: &StateDb, session_id: i64) -> anyhow::Result<Option<String>> {
    let value = db
        .conn()
        .query_row(
            "SELECT siyuan_doc_id FROM source_session WHERE id = ?1",
            params![session_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?;
    Ok(value.flatten())
}
