"""Guarded Task 5 candidate. Runner emits reviewable blobs; never updates a ref."""
from pathlib import Path
import subprocess

ROOT = Path('crates/aiks-core/src')

def load(relative, expected):
    path = ROOT / relative
    actual = subprocess.check_output(['git', 'hash-object', str(path)], text=True).strip()
    assert actual == expected, (relative, actual, expected)
    return path, path.read_text()

def replace(text, old, new, count=1):
    assert text.count(old) == count, (old[:100], text.count(old), count)
    return text.replace(old, new)

def section(text, start, end, transform):
    a = text.index(start)
    b = text.index(end, a)
    return text[:a] + transform(text[a:b]) + text[b:]

p, s = load('service/mod.rs', '40fb3359312ba96b7fdd559eafc3e635368c00d0')
s = replace(s, 'pub use revision::RevisionFence;', 'pub use revision::{RevisionFence, SupersededRevision};')
p.write_text(s)

p, s = load('storage/db.rs', 'b60992e62a6ce5e7c70b672fdfa01dfaccf249b8')
s = replace(s, '        tx.commit()?;\n        Ok(())\n    }\n\n    fn ensure_column(', '''        tx.execute_batch(include_str!("../../migrations/013_service_derived_revisions.sql"))
            .context("run V14 service derived revision migration")?;
        tx.commit()?;
        Ok(())
    }

    fn ensure_column(''')
p.write_text(s)

p, s = load('service/ingestion.rs', 'c4b8220a62b56e745bd90013d509f37f7c037d7d')
s = replace(s, '        let snapshot_id = Uuid::new_v4().to_string();', '''        // Advancing current_revision invalidates provenance without deleting
        // already published/user-edited knowledge. NULL is explicitly unknown.
        tx.execute(
            "INSERT INTO service_derived_state(session_id) VALUES (?1)
             ON CONFLICT(session_id) DO NOTHING",
            [session_id],
        )?;
        let snapshot_id = Uuid::new_v4().to_string();''')
p.write_text(s)

p, s = load('pipeline/knowledge_repo.rs', '0330f6630f54f2e30dca2b4ce3a3bf94167d721d')
start = '    pub fn save_items('
end = '    /// Get all knowledge items for a session.'
def knowledge(part):
    signature = '''    pub fn save_items(
        &self,
        session_id: i64,
        project_name: Option<&str>,
        result: &V3ExtractionResult,
    ) -> anyhow::Result<Vec<String>> {'''
    part = replace(part, signature, signature + '''
        self.save_items_guarded(session_id, project_name, result, None)
    }

    pub fn save_items_guarded(
        &self,
        session_id: i64,
        project_name: Option<&str>,
        result: &V3ExtractionResult,
        fence: Option<&crate::service::RevisionFence>,
    ) -> anyhow::Result<Vec<String>> {''')
    part = replace(part, '        let conn = self.db.conn();', '''        let mut locked = self.db.conn();
        let conn = locked.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(fence) = fence {
            fence.check_session_in_tx(&conn, session_id)?;
        }''')
    part = replace(part, '        conn.execute_batch("BEGIN IMMEDIATE")?;\n', '')
    a = part.index('        match result_res {')
    part = part[:a] + '''        let ids = result_res?;
        if let Some(fence) = fence {
            fence.record_knowledge_in_tx(&conn)?;
        }
        conn.commit()?;
        Ok(ids)
    }

'''
    return part
s = section(s, start, end, knowledge)
p.write_text(s)

p, s = load('pipeline/session_chunker.rs', 'b83cf252eeb9874eb9e7ed37f1d934a53b14d0ee')
def chunks(part):
    signature = 'pub fn save_chunks(db: &StateDb, chunks: &[SessionChunk]) -> anyhow::Result<()> {'
    part = replace(part, signature, signature + '''
    save_chunks_guarded(db, chunks, None)
}

pub fn save_chunks_guarded(
    db: &StateDb,
    chunks: &[SessionChunk],
    fence: Option<&crate::service::RevisionFence>,
) -> anyhow::Result<()> {''')
    part = replace(part, '    let conn = db.conn();', '''    let mut locked = db.conn();
    let conn = locked.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Some(fence) = fence {
        fence.check_in_tx(&conn)?;
        anyhow::ensure!(chunks.iter().all(|chunk| chunk.session_id == fence.session_id),
            "Chunk belongs to another revision session");
        conn.execute("DELETE FROM session_chunk WHERE session_id=?1", [fence.session_id])?;
    }''')
    part = replace(part, '    Ok(())\n}', '    conn.commit()?;\n    Ok(())\n}')
    return part
s = section(s, 'pub fn save_chunks(', '/// Load chunks from DB for a session', chunks)
def load_chunks(part):
    signature = 'pub fn load_chunks(db: &StateDb, session_id: i64) -> anyhow::Result<Vec<(i32, String)>> {'
    part = replace(part, signature, signature + '''
    load_chunks_guarded(db, session_id, None)
}

pub fn load_chunks_guarded(
    db: &StateDb,
    session_id: i64,
    fence: Option<&crate::service::RevisionFence>,
) -> anyhow::Result<Vec<(i32, String)>> {''')
    part = replace(part, '    let conn = db.conn();', '''    let mut locked = db.conn();
    let conn = locked.transaction()?;
    if let Some(fence) = fence {
        fence.check_session_in_tx(&conn, session_id)?;
    }''')
    part = replace(part, '        .filter_map(|r| r.ok())\n        .collect();', '        .collect::<Result<Vec<_>, _>>()?;')
    return part
s = section(s, 'pub fn load_chunks(', '#[cfg(test)]', load_chunks)
p.write_text(s)

p, s = load('pipeline/ai_stage.rs', '8c655ab950825b09932fb0b2487c461dc4406557')
s = replace(s, 'use crate::pipeline::session_chunker::load_chunks;', 'use crate::pipeline::session_chunker::load_chunks_guarded;')
sig = '''    pub async fn run(
        &self,
        db: &StateDb,
        pipeline_run_id: &str,
        session_id: i64,
        session_title: Option<&str>,
        project_name: Option<&str>,
    ) -> anyhow::Result<usize> {'''
s = replace(s, sig, sig + '''
        self.run_guarded(db, pipeline_run_id, session_id, session_title, project_name, None).await
    }

    pub async fn run_guarded(
        &self,
        db: &StateDb,
        pipeline_run_id: &str,
        session_id: i64,
        session_title: Option<&str>,
        project_name: Option<&str>,
        fence: Option<&crate::service::RevisionFence>,
    ) -> anyhow::Result<usize> {''')
s = replace(s, '        let chunks = load_chunks(db, session_id)?;', '        let chunks = load_chunks_guarded(db, session_id, fence)?;')
s = replace(s, '        if !result.worth_extracting || result.items.is_empty() {', '''        if !result.worth_extracting || result.items.is_empty() {
            if fence.is_some() {
                let mut empty = result.clone();
                empty.items.clear();
                knowledge_repo.save_items_guarded(session_id, project_name, &empty, fence)?;
            }''')
s = replace(s, '        knowledge_repo.save_items(session_id, project_name, &result)?;', '        knowledge_repo.save_items_guarded(session_id, project_name, &result, fence)?;')
p.write_text(s)

p, s = load('indexing/session.rs', '00297ccf3f3787f66e364878be43156312556f33')
sig = '''    pub async fn index_session(
        &self,
        input: SessionIndexInput,
    ) -> anyhow::Result<SessionIndexResult> {'''
s = replace(s, sig, sig + '''
        self.index_session_guarded(input, None).await
    }

    pub async fn index_session_guarded(
        &self,
        input: SessionIndexInput,
        fence: Option<&crate::service::RevisionFence>,
    ) -> anyhow::Result<SessionIndexResult> {''')
s = replace(s, '''            self.upsert_fts(
                input.session_id,
                external_id,
                source,''', '''            self.upsert_fts(
                (input.session_id, external_id, source),''')
s = replace(s, '                &input.normalized_text,\n            )?;', '                &input.normalized_text,\n                fence,\n            )?;')
s = replace(s, '        self.begin_rebuild(&rebuild)?;', '        self.begin_rebuild(&rebuild, fence)?;')
s = replace(s, '        self.finish_rebuild(&finish)?;', '        self.finish_rebuild(&finish, fence)?;')
finish_old = '''        match result {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(error);
            }
        }
        Ok(())'''
def begin(part):
    part = replace(part, "    fn begin_rebuild(&self, rebuild: &SessionRebuildContext<'_>) -> anyhow::Result<()> {", '''    fn begin_rebuild(
        &self, rebuild: &SessionRebuildContext<'_>,
        fence: Option<&crate::service::RevisionFence>,
    ) -> anyhow::Result<()> {''')
    part = replace(part, '        let conn = self.db.conn();\n        conn.execute_batch("BEGIN IMMEDIATE")?;', '''        let mut locked = self.db.conn();
        let conn = locked.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(fence) = fence {
            fence.check_session_in_tx(&conn, session_id)?;
            conn.execute(
                "INSERT INTO service_derived_state(session_id,indexed_revision) VALUES (?1,NULL)
                 ON CONFLICT(session_id) DO UPDATE SET indexed_revision=NULL",
                [session_id],
            )?;
        }''')
    return replace(part, finish_old, '        result?;\n        conn.commit()?;\n        Ok(())')
s = section(s, '    fn begin_rebuild(', '    async fn embed_chunks(', begin)
def finish(part):
    part = replace(part, "    fn finish_rebuild(&self, finish: &SessionFinishContext<'_>) -> anyhow::Result<()> {", '''    fn finish_rebuild(
        &self, finish: &SessionFinishContext<'_>,
        fence: Option<&crate::service::RevisionFence>,
    ) -> anyhow::Result<()> {''')
    part = replace(part, '        let conn = self.db.conn();', '''        let mut locked = self.db.conn();
        let conn = locked.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(fence) = fence {
            fence.check_session_in_tx(&conn, session_id)?;
        }''')
    part = replace(part, '        conn.execute_batch("BEGIN IMMEDIATE")?;\n', '')
    return replace(part, finish_old, '''        result?;
        if let Some(fence) = fence {
            fence.record_indexed_in_tx(&conn)?;
        }
        conn.commit()?;
        Ok(())''')
s = section(s, '    fn finish_rebuild(', '    fn upsert_fts(', finish)
def shortcut(part):
    part = replace(part, '''        session_id: i64,
        external_id: &str,
        source: &str,''', '''        identity: (i64, &str, &str),''')
    part = replace(part, '        normalized_text: &str,\n    )', '        normalized_text: &str,\n        fence: Option<&crate::service::RevisionFence>,\n    )')
    part = replace(part, '        let conn = self.db.conn();', '''        let (session_id, external_id, source) = identity;
        let mut locked = self.db.conn();
        let conn = locked.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(fence) = fence {
            fence.check_session_in_tx(&conn, session_id)?;
        }''')
    part = replace(part, '        Ok(())\n', '''        if let Some(fence) = fence {
            fence.record_indexed_in_tx(&conn)?;
        }
        conn.commit()?;
        Ok(())
''')
    return part
s = section(s, '    fn upsert_fts(', '#[derive(Debug)]\nstruct SessionIndexState', shortcut)
p.write_text(s)

p, s = load('pipeline/repo.rs', '4af5fd7c07502c4823600ed7cd834b10a34d1ec1')
def completed(part):
    sig = '    pub fn mark_finished(&self, run_id: &str, status: &str) -> anyhow::Result<()> {'
    part = replace(part, sig, sig + '''
        self.mark_finished_guarded(run_id, status, None)
    }

    pub fn mark_finished_guarded(
        &self, run_id: &str, status: &str,
        fence: Option<&crate::service::RevisionFence>,
    ) -> anyhow::Result<()> {''')
    part = replace(part, '        let conn = self.db.conn();', '''        let mut locked = self.db.conn();
        let conn = locked.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(fence) = fence {
            fence.check_run_in_tx(&conn, run_id)?;
        }''')
    return replace(part, '        Ok(())', '''        if let Some(fence) = fence {
            if matches!(status, "READY" | "RAW_ONLY") {
                fence.record_completed_in_tx(&conn)?;
            }
        }
        conn.commit()?;
        Ok(())''')
s = section(s, '    pub fn mark_finished(', '    /// Record a stage run', completed)
p.write_text(s)

p, s = load('pipeline/job_repo.rs', '71b284153aa70674b918ef594bdfab4de5789f48')
s = replace(s, '        let row = tx\n            .query_row(', '''        if snapshot_only {
            // Includes failed/recovered old attempts. Never consume an old
            // snapshot again, and never confuse a SQLite error with supersession.
            tx.execute(
                "UPDATE pipeline_run SET status='SUPERSEDED', finished_at=?1, updated_at=?1,
                    error_stage=NULL, error_message=NULL
                 WHERE id IN (
                    SELECT i.pipeline_run_id FROM service_job_input i
                    JOIN service_session_snapshot s ON s.id=i.snapshot_id
                    JOIN service_session_binding b ON b.session_id=s.session_id
                    JOIN pipeline_job j ON j.id=i.durable_job_id
                    WHERE s.revision < b.current_revision AND j.status IN ('PENDING','SUPERSEDED')
                 ) AND status IN ('DISCOVERED','PROCESSING','FAILED')",
                [&now],
            )?;
            tx.execute(
                "UPDATE pipeline_job SET status='SUPERSEDED', lease_until=NULL,
                    available_at=NULL, last_error=NULL, updated_at=?1
                 WHERE status='PENDING' AND id IN (
                    SELECT i.durable_job_id FROM service_job_input i
                    JOIN service_session_snapshot s ON s.id=i.snapshot_id
                    JOIN service_session_binding b ON b.session_id=s.session_id
                    WHERE s.revision < b.current_revision
                 )", [&now],
            )?;
        }

        let row = tx
            .query_row(''')
def succeeded(part):
    part = replace(part, '        let conn = self.db.conn();', '''        let mut locked = self.db.conn();
        let conn = locked.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(fence) = crate::service::RevisionFence::for_job_in_tx(&conn, durable_job_id)? {
            fence.check_in_tx(&conn)?;
        }''')
    return replace(part, '        Ok(())', '        conn.commit()?;\n        Ok(())')
s = section(s, '    pub fn mark_succeeded(', '    pub fn mark_failed(', succeeded)
new_method = '''    /// Only a checked, obsolete snapshot can enter this non-error terminal state.
    pub fn mark_superseded(&self, durable_job_id: &str) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        let mut locked = self.db.conn();
        let tx = locked.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let fence = crate::service::RevisionFence::for_job_in_tx(&tx, durable_job_id)?
            .ok_or_else(|| anyhow::anyhow!("Legacy job has no snapshot revision"))?;
        match fence.check_in_tx(&tx) {
            Err(error) if error.downcast_ref::<crate::service::SupersededRevision>().is_some() => {}
            Err(error) => return Err(error),
            Ok(()) => anyhow::bail!("Cannot supersede the current snapshot"),
        }
        let changed = tx.execute(
            "UPDATE pipeline_job SET status='SUPERSEDED', lease_until=NULL,
                available_at=NULL,last_error=NULL,updated_at=?1
             WHERE id=?2 AND status IN ('RUNNING','PENDING')",
            params![now, durable_job_id],
        )?;
        if changed == 1 {
            tx.execute(
                "UPDATE pipeline_run SET status='SUPERSEDED', finished_at=?1,
                    updated_at=?1,error_stage=NULL,error_message=NULL
                 WHERE id=(SELECT pipeline_run_id FROM service_job_input WHERE durable_job_id=?2)",
                params![now, durable_job_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

'''
s = replace(s, '    pub fn mark_failed(', new_method + '    pub fn mark_failed(')
p.write_text(s)

p, s = load('pipeline/worker.rs', '1ff1cad4df528c40b8cbe5447cd19353dcb55022')
s = replace(s, 'use crate::pipeline::session_chunker::{chunk_for_llm, save_chunks};', 'use crate::pipeline::session_chunker::{chunk_for_llm, save_chunks_guarded};\nuse crate::service::SupersededRevision;\nuse anyhow::Context;')
start = s.index('                let durable_repo = PipelineJobRepo::new(&task_db);')
end = s.index('                    Err(e) => {', start)
s = s[:start] + '''                let durable_repo = PipelineJobRepo::new(&task_db);
                let result = result.and_then(|()| durable_repo.mark_succeeded(&durable_job_id));
                match result {
                    Ok(()) => {}
                    Err(e) if e.downcast_ref::<SupersededRevision>().is_some() => {
                        if let Err(error) = durable_repo.mark_superseded(&durable_job_id) {
                            error!(job_id = %durable_job_id, error = %error,
                                "[PIPELINE] Failed to persist superseded revision");
                        }
                    }
''' + s[end:]
s = replace(s, '    let session = match input {', '    let mut revision_fence = None;\n    let session = match input {')
s = replace(s, '            Ok((session, _fence)) => session,', '''            Ok((session, fence)) => {
                fence.check_current(db)?;
                revision_fence = Some(fence);
                session
            },''')
s = replace(s, 'repo.mark_finished(run_id, "RAW_ONLY")?;', 'repo.mark_finished_guarded(run_id, "RAW_ONLY", revision_fence.as_ref())?;', 3)
s = replace(s, 'repo.mark_finished(run_id, "READY")?;', 'repo.mark_finished_guarded(run_id, "READY", revision_fence.as_ref())?;')
s = replace(s, '        .index_session(SessionIndexInput {', '        .index_session_guarded(SessionIndexInput {')
s = replace(s, '''            normalized_text,
        })
        .await
        .map_err(|error| anyhow::anyhow!("SESSION_INDEX: {error}"))?;''', '''            normalized_text,
        }, revision_fence.as_ref())
        .await
        .context("SESSION_INDEX")?;''')
s = replace(s, '    save_chunks(db, &chunk_result.chunks)?;', '    save_chunks_guarded(db, &chunk_result.chunks, revision_fence.as_ref())?;')
s = replace(s, '    let item_count = match ai_stage\n        .run(', '    let item_count = match ai_stage\n        .run_guarded(')
s = replace(s, '            job.project_name.as_deref(),\n        )', '            job.project_name.as_deref(),\n            revision_fence.as_ref(),\n        )')
s = replace(s, '''        Ok(n) => n,
        Err(e) => {''', '''        Ok(n) => n,
        Err(e) if e.downcast_ref::<SupersededRevision>().is_some() => return Err(e),
        Err(e) => {''')
p.write_text(s)

p, s = load('model/pipeline.rs', '3b9ce6baa090c7578a975bd111b2cefd6efb767b')
s = replace(s, '    RawOnly,\n    Failed,', '    RawOnly,\n    Superseded,\n    Failed,')
s = replace(s, '            PipelineStage::RawOnly => "RAW_ONLY",', '            PipelineStage::RawOnly => "RAW_ONLY",\n            PipelineStage::Superseded => "SUPERSEDED",')
s = replace(s, '            "RAW_ONLY" => PipelineStage::RawOnly,', '            "RAW_ONLY" => PipelineStage::RawOnly,\n            "SUPERSEDED" => PipelineStage::Superseded,')
p.write_text(s)
print('Applied exact Task 5 candidate; canonical branch tests remain the acceptance gate.')
