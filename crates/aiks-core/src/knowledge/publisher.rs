use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::renderer::knowledge::{render_knowledge_item_md, KnowledgeItemDoc};
use crate::sink::SiYuanSink;
use crate::storage::{KnowledgeSyncRepo, StateDb, SyncStatus};

use super::KnowledgeService;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishKnowledgeResult {
    pub knowledge_id: String,
    pub outcome: String,
    pub target_id: Option<String>,
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
        session_doc_id: None,
    };
    let markdown = render_knowledge_item_md(&doc);
    let hash = hex::encode(Sha256::digest(markdown.as_bytes()));
    let path = sink.build_knowledge_path(&item.category, &item.id, &item.title);
    let notebook_id = sink.ensure_notebook().await?;
    let repo = KnowledgeSyncRepo::new(db);
    let existing = repo.find(&item.id, "siyuan")?;

    if let Some(existing) = &existing {
        if existing.status == SyncStatus::Synced && existing.synced_hash.as_deref() == Some(&hash) {
            return Ok(PublishKnowledgeResult {
                knowledge_id: item.id,
                outcome: "unchanged".into(),
                target_id: existing.target_id.clone(),
            });
        }
        if existing.status == SyncStatus::Conflict && !overwrite_conflict {
            return Ok(PublishKnowledgeResult {
                knowledge_id: item.id,
                outcome: "conflict".into(),
                target_id: existing.target_id.clone(),
            });
        }
    }

    let remote_id = match existing.as_ref().and_then(|value| value.target_id.clone()) {
        Some(id) if sink.get_doc_notebook(&id).await?.is_some() => Some(id),
        _ => None,
    };

    if let (Some(existing), Some(id)) = (existing.as_ref(), remote_id.as_deref()) {
        if existing.status == SyncStatus::Synced && !overwrite_conflict {
            if let Some(baseline) = existing.target_hash.as_deref() {
                if let Ok(remote_md) = sink.get_document_markdown(id).await {
                    let remote_hash = hex::encode(Sha256::digest(remote_md.as_bytes()));
                    if remote_hash != baseline {
                        repo.mark_conflict(&item.id, "siyuan")?;
                        return Ok(PublishKnowledgeResult {
                            knowledge_id: item.id,
                            outcome: "conflict".into(),
                            target_id: Some(id.to_string()),
                        });
                    }
                }
            }
        }
    }

    let (doc_id, outcome) = if let Some(id) = remote_id {
        sink.update_document(&id, &markdown).await?;
        repo.record_target_doc(&item.id, "siyuan", &id, &path)?;
        (id, "updated")
    } else {
        let id = sink.create_document(&notebook_id, &path, &markdown).await?;
        repo.record_target_doc(&item.id, "siyuan", &id, &path)?;
        (id, "created")
    };

    sink.set_knowledge_attrs(&doc_id, &item.id, &session_ext_id, &hash, &item.category)
        .await?;
    repo.mark_synced(&item.id, "siyuan", &doc_id, &path, &hash)?;

    if let Ok(remote_md) = sink.get_document_markdown(&doc_id).await {
        let remote_hash = hex::encode(Sha256::digest(remote_md.as_bytes()));
        repo.record_target_hash(&item.id, "siyuan", &remote_hash)?;
    }

    Ok(PublishKnowledgeResult {
        knowledge_id: item.id,
        outcome: outcome.into(),
        target_id: Some(doc_id),
    })
}
