//! Durable, fail-closed voiding of AIKS-managed Session documents.
//! Never removes provider files or user-edited SiYuan documents.
use std::collections::BTreeMap;
use std::sync::Arc;

use aiks_core::sink::SiYuanSink;
use aiks_core::storage::backup_quiescence::BackupQuiescence;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;

#[derive(Clone)]
struct Document {
    id: String,
    kind: String,
    knowledge_id: Option<String>,
    baseline: String,
    deleted: bool,
}

#[derive(Serialize)]
pub struct VoidResult {
    pub state: String,
    pub documents_removed: usize,
}

fn sql_error(error: rusqlite::Error) -> String {
    error.to_string()
}

fn mark_phase(
    db: &aiks_core::storage::StateDb,
    id: i64,
    phase: &str,
    error: Option<&str>,
) -> Result<(), String> {
    db.conn()
        .execute(
            "INSERT INTO session_void_state (session_id, phase, last_error, updated_at)
         VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         ON CONFLICT(session_id) DO UPDATE SET phase = excluded.phase,
            last_error = excluded.last_error, updated_at = excluded.updated_at",
            params![id, phase, error],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn load_documents(db: &aiks_core::storage::StateDb, id: i64) -> Result<Vec<Document>, String> {
    let conn = db.conn();
    let mut stmt = conn
        .prepare(
            "SELECT doc_id, kind, knowledge_id, expected_hash, deleted
         FROM session_void_document WHERE session_id = ?1 ORDER BY kind, doc_id",
        )
        .map_err(sql_error)?;
    let docs = stmt
        .query_map(params![id], |row| {
            Ok(Document {
                id: row.get(0)?,
                kind: row.get(1)?,
                knowledge_id: row.get(2)?,
                baseline: row.get(3)?,
                deleted: row.get::<_, i64>(4)? != 0,
            })
        })
        .map_err(sql_error)?;
    docs.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}

fn prepare_documents(db: &aiks_core::storage::StateDb, id: i64) -> Result<Vec<Document>, String> {
    let conn = db.conn();
    let mut documents = BTreeMap::<String, Document>::new();
    {
        let mut stmt = conn.prepare(
            "SELECT target_id, COALESCE(target_hash, '') FROM sync_target
             WHERE session_id = ?1 AND sink = 'siyuan' AND target_id IS NOT NULL AND target_id <> ''",
        ).map_err(sql_error)?;
        let rows = stmt
            .query_map(params![id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (doc_id, baseline) = row.map_err(sql_error)?;
            documents.insert(
                doc_id.clone(),
                Document {
                    id: doc_id,
                    kind: "session".into(),
                    knowledge_id: None,
                    baseline,
                    deleted: false,
                },
            );
        }
    }
    let has_volumes: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'session_volume')",
        [], |row| row.get(0),
    ).map_err(sql_error)?;
    if has_volumes {
        let mut stmt = conn
            .prepare("SELECT doc_id, remote_hash FROM session_volume WHERE session_id = ?1")
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(params![id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (doc_id, baseline) = row.map_err(sql_error)?;
            documents.insert(
                doc_id.clone(),
                Document {
                    id: doc_id,
                    kind: "session".into(),
                    knowledge_id: None,
                    baseline,
                    deleted: false,
                },
            );
        }
    }
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, siyuan_doc_id, COALESCE(current_remote_hash, '') FROM knowledge_item
             WHERE source_session_id = ?1 AND siyuan_doc_id IS NOT NULL AND siyuan_doc_id <> ''",
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map(params![id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (knowledge_id, doc_id, baseline) = row.map_err(sql_error)?;
            if documents.contains_key(&doc_id) {
                return Err(format!(
                    "SiYuan document {doc_id} maps to both session and knowledge"
                ));
            }
            documents.insert(
                doc_id.clone(),
                Document {
                    id: doc_id,
                    kind: "knowledge".into(),
                    knowledge_id: Some(knowledge_id),
                    baseline,
                    deleted: false,
                },
            );
        }
    }
    // A mapped document without a trusted baseline must not be silently orphaned.
    let source_document: Option<String> = conn
        .query_row(
            "SELECT siyuan_doc_id FROM source_session WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if let Some(doc_id) = source_document.filter(|value| !value.is_empty()) {
        if !documents.contains_key(&doc_id) {
            return Err(format!(
                "session document {doc_id} has no trusted sync baseline"
            ));
        }
    }
    let unmanaged_knowledge_links: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_sync_target st
         JOIN knowledge_item ki ON ki.id = st.knowledge_id
         WHERE ki.source_session_id = ?1 AND st.sink = 'siyuan'
           AND st.target_id IS NOT NULL AND st.target_id <> ''
           AND COALESCE(ki.siyuan_doc_id, '') <> st.target_id",
            params![id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if unmanaged_knowledge_links > 0 {
        return Err("knowledge has SiYuan mappings without a verified document identity".into());
    }
    Ok(documents.into_values().collect())
}

fn cleanup_local(
    db: &aiks_core::storage::StateDb,
    session_id: i64,
    source: &str,
    external_id: &str,
) -> Result<(), String> {
    let mut conn = db.conn();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    // Remove indexed material before source rows. Every step is in one transaction.
    let statements = [
        "DELETE FROM knowledge_fts WHERE knowledge_id IN (SELECT id FROM knowledge_item WHERE source_session_id = ?1)",
        "DELETE FROM session_search_fts WHERE session_id = ?1",
        "DELETE FROM session_fts WHERE session_id = ?1",
        "DELETE FROM session_embedding_record WHERE chunk_id IN (SELECT id FROM session_search_chunk WHERE session_id = ?1)",
        "DELETE FROM session_search_chunk WHERE session_id = ?1",
        "DELETE FROM session_index_state WHERE session_id = ?1",
        "DELETE FROM embedding_record WHERE chunk_id IN (SELECT id FROM knowledge_chunk WHERE knowledge_id IN (SELECT id FROM knowledge_item WHERE source_session_id = ?1))",
        "DELETE FROM knowledge_chunk WHERE knowledge_id IN (SELECT id FROM knowledge_item WHERE source_session_id = ?1)",
        "DELETE FROM knowledge_relation_review WHERE relation_id IN (SELECT id FROM knowledge_relation WHERE source_id IN (SELECT id FROM knowledge_item WHERE source_session_id = ?1) OR target_id IN (SELECT id FROM knowledge_item WHERE source_session_id = ?1))",
        "DELETE FROM knowledge_relation WHERE source_id IN (SELECT id FROM knowledge_item WHERE source_session_id = ?1) OR target_id IN (SELECT id FROM knowledge_item WHERE source_session_id = ?1)",
        "DELETE FROM knowledge_feedback WHERE knowledge_id IN (SELECT id FROM knowledge_item WHERE source_session_id = ?1)",
        "DELETE FROM knowledge_organization_source WHERE source_knowledge_id IN (SELECT id FROM knowledge_item WHERE source_session_id = ?1) OR derived_knowledge_id IN (SELECT id FROM knowledge_item WHERE source_session_id = ?1)",
        "DELETE FROM knowledge_organization_record WHERE derived_knowledge_id IN (SELECT id FROM knowledge_item WHERE source_session_id = ?1)",
        "DELETE FROM knowledge_sync_target WHERE knowledge_id IN (SELECT id FROM knowledge_item WHERE source_session_id = ?1)",
        "DELETE FROM knowledge_item WHERE source_session_id = ?1",
        "DELETE FROM session_chunk WHERE session_id = ?1",
        "DELETE FROM ai_request_log WHERE pipeline_run_id IN (SELECT id FROM pipeline_run WHERE session_id = ?1)",
        "DELETE FROM pipeline_stage_run WHERE pipeline_run_id IN (SELECT id FROM pipeline_run WHERE session_id = ?1)",
        "DELETE FROM pipeline_job WHERE session_id = ?1",
        "DELETE FROM pipeline_run WHERE session_id = ?1",
        "DELETE FROM sync_target WHERE session_id = ?1",
        "DELETE FROM session_volume WHERE session_id = ?1",
    ];
    for statement in statements {
        tx.execute(statement, params![session_id])
            .map_err(sql_error)?;
    }
    tx.execute(
        "DELETE FROM knowledge_extraction WHERE source = ?1 AND external_session_id = ?2",
        params![source, external_id],
    )
    .map_err(sql_error)?;
    tx.execute(
        "UPDATE source_session SET content_hash = NULL, source_updated_at = NULL,
         parser_version = NULL, siyuan_doc_id = NULL WHERE id = ?1",
        params![session_id],
    )
    .map_err(sql_error)?;
    tx.execute(
        "UPDATE session_void_state SET phase = 'VOIDED', last_error = NULL,
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(sql_error)?;
    tx.commit().map_err(sql_error)
}

#[tauri::command]
pub async fn void_session(
    session_id: i64,
    state: State<'_, AppState>,
    gate: State<'_, Arc<BackupQuiescence>>,
) -> Result<VoidResult, String> {
    let _writer = gate.try_begin_write().ok_or("正在执行备份，暂不能作废")?;
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let (source, external_id, phase) = {
        let conn = db.conn();
        let (source, external_id): (String, String) = conn
            .query_row(
                "SELECT source, external_session_id FROM source_session WHERE id = ?1",
                params![session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let phase: Option<String> = conn
            .query_row(
                "SELECT phase FROM session_void_state WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        (source, external_id, phase)
    };
    if phase.as_deref() == Some("VOIDED") {
        return Ok(VoidResult {
            state: "VOIDED".into(),
            documents_removed: 0,
        });
    }
    // Atomically refuse a running worker and persist the exclusion before any remote I/O.
    {
        let mut conn = db.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let running: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM pipeline_job WHERE session_id = ?1 AND status = 'RUNNING')",
            params![session_id], |row| row.get(0),
        ).map_err(sql_error)?;
        if running {
            return Err("AI 任务正在运行，暂不能作废该会话".into());
        }
        tx.execute(
            "INSERT OR IGNORE INTO session_sync_exclusion
             (source, external_session_id, created_at, baseline_updated_at, latest_seen_updated_at)
             SELECT source, external_session_id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                    source_updated_at, source_updated_at FROM source_session WHERE id = ?1",
            params![session_id],
        )
        .map_err(sql_error)?;
        tx.execute(
            "UPDATE pipeline_job SET status = 'CANCELLED', available_at = NULL, lease_until = NULL,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE session_id = ?1 AND status = 'PENDING'",
            params![session_id],
        )
        .map_err(sql_error)?;
        tx.commit().map_err(sql_error)?;
    }
    mark_phase(&db, session_id, "PREPARING", None)?;
    // User-managed knowledge and cross-session derived documents cannot be
    // recreated from this source Session. Require manual review before purge.
    let (user_managed, cross_session_derived): (i64, i64) = {
        let conn = db.conn();
        let user_managed = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_item WHERE source_session_id = ?1
             AND (managed_by <> 'pipeline' OR source_type <> 'conversation')",
            params![session_id],
            |row| row.get(0),
        ).map_err(sql_error)?;
        let derived = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_organization_source os
             JOIN knowledge_item src ON src.id = os.source_knowledge_id
             LEFT JOIN knowledge_item derived ON derived.id = os.derived_knowledge_id
             WHERE src.source_session_id = ?1
               AND (derived.source_session_id IS NULL OR derived.source_session_id <> ?1)",
            params![session_id],
            |row| row.get(0),
        ).map_err(sql_error)?;
        (user_managed, derived)
    };
    if user_managed > 0 || cross_session_derived > 0 {
        let error = format!(
            "存在 {user_managed} 条用户管理知识和 {cross_session_derived} 条跨会话衍生引用，需要人工审核后才能清理"
        );
        mark_phase(&db, session_id, "CONFLICT", Some(&error))?;
        return Err(error);
    }
    let docs = load_documents(&db, session_id)?;
    let documents = if docs.is_empty() {
        prepare_documents(&db, session_id)?
    } else {
        docs.clone()
    };
    let mut cfg = engine.config().siyuan.clone();
    cfg.base_url = engine.siyuan_base_url().to_string();
    let sink = SiYuanSink::new(cfg).map_err(|error| error.to_string())?;

    // Verify every existing document before removing the first one.
    for doc in documents.iter().filter(|doc| !doc.deleted) {
        let exists = sink
            .get_doc_notebook(&doc.id)
            .await
            .map_err(|error| error.to_string())?;
        if exists.is_none() {
            // Only a previously journaled deletion may be reconciled as absent.
            if docs.is_empty() {
                let error = format!("文档 {} 在作废预检前已不存在，需人工核查", doc.id);
                mark_phase(&db, session_id, "CONFLICT", Some(&error))?;
                return Err(error);
            }
            continue;
        }
        if let Err(error) = sink
            .verify_managed_document_for_removal(
                &doc.id,
                &doc.kind,
                &source,
                &external_id,
                doc.knowledge_id.as_deref(),
                &doc.baseline,
            )
            .await
        {
            let reason = format!("SiYuan 清理校验失败：{error}");
            mark_phase(&db, session_id, "CONFLICT", Some(&reason))?;
            return Err(reason);
        }
    }
    // A durable deletion journal exists before any remote mutation occurs.
    if docs.is_empty() {
        let mut conn = db.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        for doc in &documents {
            tx.execute(
                "INSERT INTO session_void_document
                 (session_id, doc_id, kind, knowledge_id, expected_hash, deleted)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0)",
                params![session_id, doc.id, doc.kind, doc.knowledge_id, doc.baseline],
            )
            .map_err(sql_error)?;
        }
        tx.commit().map_err(sql_error)?;
    }
    mark_phase(&db, session_id, "REMOVING", None)?;
    let mut removed = 0;
    for doc in documents.iter().filter(|doc| !doc.deleted) {
        let exists = sink
            .get_doc_notebook(&doc.id)
            .await
            .map_err(|error| error.to_string())?;
        if exists.is_some() {
            if let Err(error) = sink
                .verify_managed_document_for_removal(
                    &doc.id,
                    &doc.kind,
                    &source,
                    &external_id,
                    doc.knowledge_id.as_deref(),
                    &doc.baseline,
                )
                .await
            {
                let reason = format!("删除前二次校验失败：{error}");
                mark_phase(&db, session_id, "CONFLICT", Some(&reason))?;
                return Err(reason);
            }
            if let Err(error) = sink.remove_managed_document(&doc.id).await {
                let reason = format!("SiYuan 删除失败：{error}");
                mark_phase(&db, session_id, "FAILED", Some(&reason))?;
                return Err(reason);
            }
        }
        db.conn().execute(
            "UPDATE session_void_document SET deleted = 1 WHERE session_id = ?1 AND doc_id = ?2",
            params![session_id, doc.id],
        ).map_err(sql_error)?;
        removed += 1;
    }
    if let Err(error) = cleanup_local(&db, session_id, &source, &external_id) {
        mark_phase(&db, session_id, "FAILED", Some(&error))?;
        return Err(format!("远端清理后本地收尾失败；可重试作废：{error}"));
    }
    Ok(VoidResult {
        state: "VOIDED".into(),
        documents_removed: removed,
    })
}

#[tauri::command]
pub async fn restore_void_session(
    session_id: i64,
    state: State<'_, AppState>,
    gate: State<'_, Arc<BackupQuiescence>>,
) -> Result<(), String> {
    let _writer = gate.try_begin_write().ok_or("正在执行备份，暂不能恢复")?;
    let engine = state.engine().ok_or("Engine not initialized")?;
    let db = engine.db();
    let mut conn = db.conn();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    let phase: Option<String> = tx
        .query_row(
            "SELECT phase FROM session_void_state WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    if phase.as_deref() != Some("VOIDED") {
        return Err("清理尚未完成，不能恢复；请先处理作废冲突或重试".into());
    }
    tx.execute(
        "DELETE FROM session_sync_exclusion WHERE (source, external_session_id) IN
         (SELECT source, external_session_id FROM source_session WHERE id = ?1)",
        params![session_id],
    )
    .map_err(sql_error)?;
    tx.execute(
        "DELETE FROM session_void_document WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(sql_error)?;
    tx.execute(
        "UPDATE session_void_state SET phase = 'RESTORED', last_error = NULL,
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(sql_error)?;
    tx.commit().map_err(sql_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_cleanup_removes_knowledge_and_indexes_but_keeps_session_identity() {
        let path = std::env::temp_dir().join(format!(
            "aiks-session-void-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = aiks_core::storage::StateDb::open(&path).expect("open test state db");
        let session_id = {
            let conn = db.conn();
            conn.execute(
                "INSERT INTO source_session
                 (source, external_session_id, content_hash, last_seen_at, created_at, updated_at)
                 VALUES ('codex', 'void-test', 'source-hash', 'now', 'now', 'now')",
                [],
            )
            .unwrap();
            let id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO knowledge_item
                 (id, source_session_id, title, content, created_at, updated_at)
                 VALUES ('knowledge-void-test', ?1, 'Example', 'secret text', 'now', 'now')",
                params![id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO knowledge_chunk
                 (id, knowledge_id, chunk_index, text, content_hash, created_at)
                 VALUES ('chunk-void-test', 'knowledge-void-test', 0, 'secret text', 'hash', 'now')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO embedding_record
                 (id, chunk_id, model, dimensions, vector, created_at)
                 VALUES ('vector-void-test', 'chunk-void-test', 'test', 1, X'0000803F', 'now')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO knowledge_fts (knowledge_id, title, summary, content, tags)
                 VALUES ('knowledge-void-test', 'Example', '', 'secret text', '')",
                [],
            )
            .unwrap();
            id
        };
        mark_phase(&db, session_id, "REMOVING", None).unwrap();
        cleanup_local(&db, session_id, "codex", "void-test").expect("atomic local purge");

        let conn = db.conn();
        for table in ["knowledge_item", "knowledge_chunk", "embedding_record", "knowledge_fts"] {
            let sql = format!("SELECT COUNT(*) FROM {table}");
            let count: i64 = conn.query_row(&sql, [], |row| row.get(0)).unwrap();
            assert_eq!(count, 0, "{table} should be empty");
        }
        let (retained, hash): (i64, Option<String>) = conn.query_row(
            "SELECT id, content_hash FROM source_session WHERE id = ?1",
            params![session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(retained, session_id);
        assert!(hash.is_none(), "restoration must re-read source content");
        let phase: String = conn.query_row(
            "SELECT phase FROM session_void_state WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(phase, "VOIDED");
        drop(conn);
        drop(db);
        let _ = std::fs::remove_file(&path);
    }
}
