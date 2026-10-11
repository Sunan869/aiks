//! Durable S3.4 user-reviewed organization audit. Read-only retrieval and
//! append-only provenance; no Provider or SiYuan content writes occur here.
use crate::knowledge::organization_scope::OrganizationScopeGuard;
use crate::storage::StateDb;
use anyhow::{bail, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationAuditEntry {
    pub derived_knowledge_id: String,
    pub title: String,
    pub status: String,
    pub operation: String,
    pub reviewed_at: String,
    pub source_knowledge_ids: Vec<String>,
}

pub struct OrganizationAuditRepo<'a> {
    db: &'a StateDb,
}

impl<'a> OrganizationAuditRepo<'a> {
    pub fn new(db: &'a StateDb) -> Self {
        Self { db }
    }

    /// Validate all caller-controlled audit fields before remote document creation.
    pub fn validate_input(operation: &str, source_ids: &[String]) -> Result<()> {
        if !matches!(
            operation,
            "structure" | "rewrite" | "key_conclusions" | "compare" | "merge_draft"
        ) {
            bail!("Invalid organization operation");
        }
        anyhow::ensure!(
            !source_ids.is_empty() && source_ids.len() <= 5,
            "Organization audit needs 1 to 5 source identities"
        );
        if matches!(operation, "compare" | "merge_draft") {
            anyhow::ensure!(
                source_ids.len() >= 2,
                "Multi-source organization must cite at least two sources"
            );
        }
        let mut unique = HashSet::new();
        for id in source_ids {
            anyhow::ensure!(
                !id.is_empty()
                    && id.len() <= 256
                    && id.chars().all(|ch| ch.is_ascii_alphanumeric()
                        || matches!(ch, '-' | '_' | '.' | ':')),
                "Invalid source knowledge identity"
            );
            anyhow::ensure!(unique.insert(id), "Duplicate organization audit sources");
        }
        Ok(())
    }

    /// Record only after an independent user-managed document was created.
    /// This cannot make a remote SiYuan HTTP write atomic; failures are
    /// explicitly surfaced to the caller, not silently treated as success.
    pub fn record(&self, derived_id: &str, operation: &str, source_ids: &[String]) -> Result<()> {
        Self::validate_input(operation, source_ids)?;
        anyhow::ensure!(
            source_ids.iter().all(|id| id != derived_id),
            "Derived knowledge cannot cite itself"
        );
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let managed: Option<(String, String)> = tx
            .query_row(
                "SELECT source_type,managed_by FROM knowledge_item WHERE id = ?1
                 AND status = 'active'",
                params![derived_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        anyhow::ensure!(
            matches!(
                managed,
                Some((ref source, ref owner))
                    if source.as_str() == "manual" && owner.as_str() == "user"
            ),
            "Only an active independent user-managed knowledge document can be audited"
        );
        for id in source_ids {
            let exists: i64 = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_item WHERE id = ?1)",
                params![id],
                |row| row.get(0),
            )?;
            anyhow::ensure!(exists == 1, "Original source knowledge was not found");
        }
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO knowledge_organization_record
             (derived_knowledge_id,operation,reviewed_at) VALUES (?1,?2,?3)",
            params![derived_id, operation, now],
        )?;
        for (position, id) in source_ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO knowledge_organization_source
                 (derived_knowledge_id,source_knowledge_id,position)
                 VALUES (?1,?2,?3)",
                params![derived_id, id, position as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Recover the audit of an existing, locally bound SiYuan document only.
    /// No remote calls or new knowledge rows are made. A local-only manual
    /// draft (or a forged audit footer) must never be claimed as published.
    pub fn recover_existing(
        &self,
        derived_id: &str,
        operation: &str,
        source_ids: &[String],
    ) -> Result<()> {
        Self::validate_input(operation, source_ids)?;
        OrganizationScopeGuard::new(self.db).validate(source_ids)?;
        let content: Option<String> = self
            .db
            .conn()
            .query_row(
                "SELECT content FROM knowledge_item WHERE id = ?1
                 AND source_type = 'manual' AND managed_by = 'user' AND status = 'active'
                 AND NULLIF(TRIM(siyuan_doc_id),'') IS NOT NULL
                 AND NULLIF(TRIM(generated_hash),'') IS NOT NULL",
                params![derived_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(content) = content else {
            bail!("Reviewed knowledge has no active local SiYuan binding");
        };
        let (prefix, footer) = content
            .rsplit_once("\n## AIKS 整理审核记录\n")
            .ok_or_else(|| anyhow::anyhow!("Existing document has no AIKS review footer"))?;
        anyhow::ensure!(
            prefix.ends_with("\n\n---\n"),
            "Existing document has no separate review audit section"
        );

        // Match the exact footer emitted by appendOrganizationAudit rather
        // than accepting operation/source tokens somewhere in arbitrary text.
        let mut lines = footer.lines();
        let expected_operation = format!("操作：{operation}");
        anyhow::ensure!(
            lines.next() == Some(expected_operation.as_str()),
            "Reviewed operation does not match the existing document"
        );
        let reviewed_at = lines
            .next()
            .and_then(|line| line.strip_prefix("人工确认时间（UTC）："))
            .ok_or_else(|| anyhow::anyhow!("Reviewed confirmation time is missing"))?;
        anyhow::ensure!(
            reviewed_at.ends_with('Z') && chrono::DateTime::parse_from_rfc3339(reviewed_at).is_ok(),
            "Reviewed confirmation timestamp is invalid"
        );
        anyhow::ensure!(
            lines.next() == Some("原始知识未修改；此文档为可独立归档的人工确认副本。"),
            "Existing document lacks the reviewed-copy declaration"
        );
        anyhow::ensure!(
            lines.next() == Some("来源知识："),
            "Existing document lacks the original source section"
        );
        for id in source_ids {
            let expected = format!("- AIKS 知识：{id}");
            anyhow::ensure!(
                lines.next() == Some(expected.as_str()),
                "Reviewed document source evidence does not match the selected knowledge"
            );
        }
        anyhow::ensure!(
            lines.next().is_none(),
            "Reviewed document has unexpected extra source evidence"
        );
        self.record(derived_id, operation, source_ids)
    }

    /// Reconstruct all reviewed copies after app restart. Archive/restore is
    /// projected from the current canonical knowledge status, not a fake
    /// second status machine.
    pub fn list_for_source(&self, source_id: &str) -> Result<Vec<OrganizationAuditEntry>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT r.derived_knowledge_id,r.operation,r.reviewed_at,
                    COALESCE(ki.title,'（知识条目已移除）'),
                    COALESCE(ki.status,'missing')
             FROM knowledge_organization_record r
             JOIN knowledge_organization_source os
               ON os.derived_knowledge_id = r.derived_knowledge_id
             LEFT JOIN knowledge_item ki ON ki.id = r.derived_knowledge_id
             WHERE os.source_knowledge_id = ?1
             ORDER BY r.reviewed_at DESC,r.derived_knowledge_id DESC
             LIMIT 100",
        )?;
        let rows = stmt.query_map(params![source_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let summaries = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        let mut sources = conn.prepare(
            "SELECT source_knowledge_id FROM knowledge_organization_source
             WHERE derived_knowledge_id = ?1 ORDER BY position",
        )?;
        let mut result = Vec::new();
        for (derived_knowledge_id, operation, reviewed_at, title, status) in summaries {
            let source_ids = sources
                .query_map(params![derived_knowledge_id], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            result.push(OrganizationAuditEntry {
                derived_knowledge_id,
                title,
                status,
                operation,
                reviewed_at,
                source_knowledge_ids: source_ids,
            });
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::workbench::{CreateKnowledgeInput, KnowledgeService};

    fn create(db: &StateDb, title: &str) -> String {
        KnowledgeService::new(db)
            .create_manual(CreateKnowledgeInput {
                title: title.into(),
                category: Some("decision".into()),
                project_name: Some("alpha".into()),
                summary: Some("reviewed".into()),
                content: "Explicit human-reviewed text".into(),
                tags: vec![],
            })
            .unwrap()
            .id
    }

    #[test]
    fn persists_audit_and_restores_status_after_restart_without_editing_sources() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.db");
        let db = StateDb::open(&path).unwrap();
        let first = create(&db, "First source");
        let second = create(&db, "Second source");
        let derived = create(&db, "Reviewed comparison");
        let repo = OrganizationAuditRepo::new(&db);
        repo.record(&derived, "compare", &[first.clone(), second.clone()])
            .unwrap();
        let entries = repo.list_for_source(&first).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].derived_knowledge_id, derived);
        assert_eq!(entries[0].source_knowledge_ids, vec![first.clone(), second]);
        assert_eq!(entries[0].status, "active");
        assert!(repo
            .record(&derived, "compare", std::slice::from_ref(&first))
            .is_err());
        assert!(repo
            .record(&derived, "structure", std::slice::from_ref(&first))
            .is_err());
        KnowledgeService::new(&db).archive(&derived).unwrap();
        assert_eq!(repo.list_for_source(&first).unwrap()[0].status, "archived");
        drop(db);

        let reopened = StateDb::open(&path).unwrap();
        let repo = OrganizationAuditRepo::new(&reopened);
        assert_eq!(repo.list_for_source(&first).unwrap()[0].status, "archived");
        KnowledgeService::new(&reopened).restore(&derived).unwrap();
        assert_eq!(repo.list_for_source(&first).unwrap()[0].status, "active");
        assert_eq!(
            KnowledgeService::new(&reopened)
                .get(&first)
                .unwrap()
                .unwrap()
                .status,
            "active"
        );
    }

    #[test]
    fn existing_reviewed_document_can_repair_missing_audit_without_remote_writes() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("manual-recovery.db")).unwrap();
        let source = create(&db, "Source");
        let content = format!(
            "Reviewed plan\n\n---\n\n## AIKS 整理审核记录\n操作：structure\n人工确认时间（UTC）：2026-10-11T08:00:00.000Z\n原始知识未修改；此文档为可独立归档的人工确认副本。\n来源知识：\n- AIKS 知识：{source}"
        );
        let derived = KnowledgeService::new(&db)
            .create_manual(CreateKnowledgeInput {
                title: "Reviewed result".into(),
                category: Some("decision".into()),
                project_name: Some("alpha".into()),
                summary: None,
                content: content.clone(),
                tags: vec![],
            })
            .unwrap();
        let repo = OrganizationAuditRepo::new(&db);
        let sources = vec![source.clone()];

        // A local-only note with a plausible footer cannot be recovered.
        assert!(repo
            .recover_existing(&derived.id, "structure", &sources)
            .is_err());
        db.conn()
            .execute(
                "UPDATE knowledge_item
                 SET siyuan_doc_id = 'simulated-bound-doc', generated_hash = 'simulated-hash'
                 WHERE id = ?1",
                params![derived.id],
            )
            .unwrap();

        assert!(repo
            .recover_existing(&derived.id, "rewrite", &sources)
            .is_err());
        assert!(repo
            .recover_existing(&source, "structure", &sources)
            .is_err());

        // Reject incomplete, forged or appended review evidence before insert.
        for invalid in [
            content.replace("2026-10-11T08:00:00.000Z", "not-a-time"),
            content.replace("原始知识未修改；此文档为可独立归档的人工确认副本。", ""),
            format!("{content}\n- AIKS 知识：injected"),
        ] {
            db.conn()
                .execute(
                    "UPDATE knowledge_item SET content = ?2 WHERE id = ?1",
                    params![derived.id, invalid],
                )
                .unwrap();
            assert!(repo
                .recover_existing(&derived.id, "structure", &sources)
                .is_err());
        }
        db.conn()
            .execute(
                "UPDATE knowledge_item SET content = ?2 WHERE id = ?1",
                params![derived.id, content],
            )
            .unwrap();
        assert!(repo.list_for_source(&source).unwrap().is_empty());
        repo.recover_existing(&derived.id, "structure", &sources)
            .unwrap();
        assert_eq!(repo.list_for_source(&source).unwrap().len(), 1);
        assert!(repo
            .recover_existing(&derived.id, "structure", &sources)
            .is_err());
        assert_eq!(repo.list_for_source(&source).unwrap().len(), 1);
        assert_eq!(
            KnowledgeService::new(&db)
                .get(&source)
                .unwrap()
                .unwrap()
                .status,
            "active"
        );
    }

    #[test]
    fn refuses_invalid_source_and_unrelated_operations_without_partial_audit() {
        let dir = tempfile::tempdir().unwrap();
        let db = StateDb::open(&dir.path().join("fail-closed.db")).unwrap();
        let source = create(&db, "Source");
        let derived = create(&db, "Derived");
        let repo = OrganizationAuditRepo::new(&db);
        for (operation, ids) in [
            ("forged", vec![source.clone()]),
            ("merge_draft", vec![source.clone()]),
            ("structure", vec![source.clone(), source.clone()]),
            ("structure", vec!["not-existing".into()]),
            ("structure", vec![derived.clone()]),
        ] {
            assert!(repo.record(&derived, operation, &ids).is_err());
        }
        assert!(repo.list_for_source(&source).unwrap().is_empty());
    }
}
