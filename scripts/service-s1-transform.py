"""Exact Task7 reuse of ingestion transaction and legacy read projection."""
from pathlib import Path
import subprocess,re

def load(path,sha):
 p=Path(path);assert subprocess.check_output(['git','hash-object',str(p)],text=True).strip()==sha,path
 return p,p.read_text()
def sub(s,old,new,n=1):
 assert s.count(old)==n,(old[:80],s.count(old),n)
 return s.replace(old,new)

p,s=load('crates/aiks-core/src/service/ingestion.rs','1df70891b47022cc39fe921763fad4af19f13af9')
a=s.index('        let registered: bool = tx.query_row(');b=s.index('\nfn find_receipt(',a)
shared=s[a:b];assert shared.endswith('    }\n}\n');shared=shared[:-8]
shared,n=re.subn(r'^ +tx\.commit\(\)\?;\n','',shared,flags=re.MULTILINE);assert n==3
shared=shared.replace('&tx,','tx,')
s=s[:a]+'''        let result = accept_in_tx(&tx, context, snapshot)?;
        tx.commit()?;
        Ok(result)
    }
}

pub(crate) fn accept_in_tx(
    tx: &Transaction<'_>,
    context: &LocalContext,
    snapshot: &ValidatedSnapshot,
) -> Result<(SnapshotReceipt, bool, bool), ServiceError> {
    let input = &snapshot.submission;
'''+shared+'}\n'+s[b:];p.write_text(s)

p,s=load('crates/aiks-core/src/storage/db.rs','ac2a3fb9134c9585f7de0956cf657db9c74d44f7')
s=sub(s,'.context("run V16 per-knowledge provenance migration")?;','''.context("run V16 per-knowledge provenance migration")?;
        tx.execute_batch(include_str!("../../migrations/016_service_knowledge_binding.sql"))
            .context("run V17 standalone knowledge ownership migration")?;''');p.write_text(s)

p,s=load('crates/aiks-core/src/service/query.rs','3c435708d38af460f3c1c03638a05562e6c9a28f')
a=s.index('pub fn session(db:');b=s.index('\npub fn receipt(',a)
s=s[:a]+'''pub fn session(db: &StateDb, ctx: &LocalContext, id: &str) -> Result<Value, ServiceError> {
    let id: i64 = id.parse().map_err(|_| ServiceError::NotFound)?;
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    let (revision, title, source): (u32, String, String) = tx.query_row(
        "SELECT b.current_revision, substr(COALESCE(ss.title,''),1,4096),ss.source
         FROM service_session_binding b JOIN source_session ss ON ss.id=b.session_id
         WHERE b.session_id=?1 AND b.principal_id=?2 AND b.space_id=?3",
        params![id,ctx.principal_id(),ctx.space_id()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional()?.ok_or(ServiceError::NotFound)?;
    if revision == 0 {
        let projection: Option<Option<String>> = tx.query_row(
            "SELECT CASE WHEN length(CAST(content AS BLOB))<=1048576 THEN content END
             FROM session_search_fts WHERE session_id=?1 LIMIT 1", [id], |r| r.get(0),
        ).optional()?;
        return Ok(json!({"session_id":id.to_string(),"title":title,"source":source,
            "revision":null,"session":null,"snapshot_state":"source_unavailable",
            "content_state":if projection.is_some(){"legacy_projection"}else{"unavailable"},
            "content":projection.flatten(),"index_current":false}));
    }
    let bytes: Option<Vec<u8>> = tx.query_row(
        "SELECT CASE WHEN length(canonical_json)<=16777216 THEN canonical_json END
         FROM service_session_snapshot WHERE session_id=?1 AND revision=?2",
        params![id,revision], |r| r.get(0),
    ).optional()?.ok_or(ServiceError::Internal)?;
    let mut session: NormalizedSession = serde_json::from_slice(&bytes.ok_or(ServiceError::TooLarge)?)
        .map_err(|_| ServiceError::Internal)?;
    session.source_path=None;
    session.project_path=None;
    session.metadata.clear();
    for message in &mut session.messages { message.metadata.clear(); }
    Ok(json!({"session_id":id.to_string(),"revision":revision,"session":session,"snapshot_state":"ready"}))
}
''' + s[b:]
s=sub(s,'WHERE b.principal_id=?1 AND b.space_id=?2 AND ss.is_missing=0','WHERE b.principal_id=?1 AND b.space_id=?2')
s=sub(s,'"completed_revision":row.get::<_,Option<u32>>(5)?,"index_current":indexed==Some(revision)',
    '"completed_revision":row.get::<_,Option<u32>>(5)?,"index_current":revision>0 && indexed==Some(revision),"snapshot_state":if revision==0 {"source_unavailable"} else {"ready"}')
s=sub(s,'d.revision,b.current_revision','d.revision,COALESCE(b.current_revision,0)',2)
s=sub(s,'FROM knowledge_item ki JOIN service_session_binding b ON b.session_id=ki.source_session_id',
'''FROM knowledge_item ki LEFT JOIN service_session_binding b ON b.session_id=ki.source_session_id
         LEFT JOIN service_knowledge_binding kb ON kb.knowledge_id=ki.id AND ki.source_session_id IS NULL''',2)
s=sub(s,"WHERE ki.id=?1 AND ki.status='active' AND b.principal_id=?2 AND b.space_id=?3",
    "WHERE ki.id=?1 AND ki.status='active' AND ((b.principal_id=?2 AND b.space_id=?3) OR (kb.principal_id=?2 AND kb.space_id=?3))")
s=sub(s,"WHERE ki.status='active' AND b.principal_id=?1 AND b.space_id=?2",
    "WHERE ki.status='active' AND ((b.principal_id=?1 AND b.space_id=?2) OR (kb.principal_id=?1 AND kb.space_id=?2))")
p.write_text(s)
