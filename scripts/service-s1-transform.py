"""Exact Task 6 fixes; prepare emits reviewable blobs and never changes refs."""
from pathlib import Path
import subprocess

def edit(path, expected, change):
    p = Path(path)
    actual = subprocess.check_output(['git', 'hash-object', str(p)], text=True).strip()
    assert actual == expected, (path, actual, expected)
    p.write_text(change(p.read_text()))

def sub(s, old, new, count=1):
    assert s.count(old) == count, (old[:90], s.count(old), count)
    return s.replace(old, new)

def contracts(s):
    s = sub(s, '    #[error("Internal service error")]', '''    #[error("AI is not configured or is disabled")]
    AiDisabled,
    #[error("Canonical content is temporarily unavailable")]
    ContentUnavailable,
    #[error("Internal service error")]''')
    return sub(s, '            Self::Unavailable => "unavailable",', '''            Self::Unavailable => "unavailable",
            Self::AiDisabled => "ai_disabled",
            Self::ContentUnavailable => "content_unavailable",''')
edit('crates/aiks-core/src/service/contracts.rs', '31db005262b6f202ed7a6a12149dab6a22c82cd1', contracts)

def runtime(s):
    s = sub(s, '''                .get_document_markdown_bounded(doc_id, query::MAX_CONTENT_BYTES)
                .await
                .map_err(|_| ServiceError::Unavailable)?;''', '''                .get_document_markdown_bounded(doc_id, query::MAX_CONTENT_BYTES)
                .await
                .map_err(|_| ServiceError::ContentUnavailable)?;''')
    s = sub(s, '''        if !self.models.llm_config().enabled {
            return Err(ServiceError::Unavailable);
        }
''', '')
    return sub(s, '        let view = self.knowledge(id.clone()).await?;', '''        // Authorize the resource first, without fetching remote content when off.
        self.knowledge_metadata(id.clone()).await?;
        if !self.models.llm_config().enabled {
            return Err(ServiceError::AiDisabled);
        }
        let view = self.knowledge(id.clone()).await?;''')
edit('crates/aiks-core/src/service/runtime.rs', 'a196fe590ec0ba5bf1c2d509db09412892359778', runtime)

def query(s):
    s = sub(s, '    Ok(db\n', '    db\n', 2)
    s = sub(s, '.ok_or(ServiceError::NotFound)?)', '.ok_or(ServiceError::NotFound)', 2)
    s = sub(s, 'd.knowledge_revision', 'd.revision', 3)
    s = sub(s, 'LEFT JOIN service_derived_state d ON d.session_id=b.session_id\n         WHERE ki',
        'LEFT JOIN service_knowledge_revision d ON d.session_id=b.session_id AND d.knowledge_id=ki.id\n         WHERE ki', 2)
    return sub(s, 'JOIN service_derived_state d ON d.session_id=b.session_id WHERE ki.id=?1',
        'JOIN service_knowledge_revision d ON d.session_id=b.session_id AND d.knowledge_id=ki.id WHERE ki.id=?1')
edit('crates/aiks-core/src/service/query.rs', 'c9f9078147355451c789ebaa5091a801be0c380a', query)

def knowledge(s):
    return sub(s, '''        let ids = result_res?;
        if let Some(fence) = fence {
            fence.record_knowledge_in_tx(&conn)?;''', '''        let ids = result_res?;
        if let Some(fence) = fence {
            // Only regenerated drafts advance; preserved user/published items
            // must not inherit the new extraction's source revision.
            for id in &ids {
                conn.execute(
                    "INSERT INTO service_knowledge_revision(knowledge_id,session_id,revision)
                     SELECT id,source_session_id,?3 FROM knowledge_item
                     WHERE id=?1 AND source_session_id=?2 AND managed_by='pipeline'
                       AND siyuan_doc_id IS NULL
                     ON CONFLICT(knowledge_id) DO UPDATE SET
                       session_id=excluded.session_id, revision=excluded.revision",
                    params![id, session_id, fence.revision],
                )?;
            }
            fence.record_knowledge_in_tx(&conn)?;''')
edit('crates/aiks-core/src/pipeline/knowledge_repo.rs', '1b695b38051494d561ec09ef7e6d20a7682c844c', knowledge)

def scope(s):
    s = sub(s, '''        let revision = match corpus {
            SearchCorpus::Session => "d.indexed_revision",
            SearchCorpus::Knowledge => "d.knowledge_revision",
        };''', '''        let (join, revision) = match corpus {
            SearchCorpus::Session => (
                "JOIN service_derived_state d ON d.session_id=b.session_id",
                "d.indexed_revision",
            ),
            SearchCorpus::Knowledge => (
                "JOIN service_knowledge_revision d ON d.session_id=b.session_id AND d.knowledge_id=ki.id",
                "d.revision",
            ),
        };''')
    return sub(s, '''             JOIN service_derived_state d ON d.session_id=b.session_id
             WHERE''', '''             {join}
             WHERE''')
edit('crates/aiks-core/src/search/scope.rs', 'ec6b545d2a949aa9739c04e687eb5444e36ac53a', scope)

def db(s):
    return sub(s, '''            .context("run V15 service read epoch migration")?;
        tx.commit()?;''', '''            .context("run V15 service read epoch migration")?;
        tx.execute_batch(include_str!("../../migrations/015_service_knowledge_revisions.sql"))
            .context("run V16 per-knowledge provenance migration")?;
        tx.commit()?;''')
edit('crates/aiks-core/src/storage/db.rs', '6a9171c97ffb524b696eb1147cf4b9adec35d5ea', db)
edit('apps/aiks-service/src/error.rs', '4a049e3c1f6736c57bd9185555704d6d6362d694', lambda s:
    sub(s, '            ServiceError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,',
        '            ServiceError::Unavailable | ServiceError::AiDisabled | ServiceError::ContentUnavailable => StatusCode::SERVICE_UNAVAILABLE,'))
# Correct the mock response envelope; do not relax the existing SiYuan parser.
edit('apps/aiks-service/tests/content_boundary.rs', '1cbe8c0e46963032661461d9de721f03f60a1e13', lambda s:
    sub(s, '"code":0,"data":', '"code":0,"msg":"","data":', 4))
print('Applied candidate. Canonical tests are the acceptance gate.')
