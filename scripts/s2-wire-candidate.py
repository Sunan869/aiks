"""Apply SHA-guarded Task 8 publish-recovery hardening; CI emits formatted blobs only."""
from pathlib import Path
import hashlib

def blob_sha(data):
    return hashlib.sha1(b"blob "+str(len(data)).encode()+b"\0"+data).hexdigest()

def edit(path, expected, transform):
    p=Path(path)
    raw=p.read_bytes()
    actual=blob_sha(raw)
    assert actual==expected,(path,actual,expected)
    p.write_text(transform(raw.decode()))

def db(text):
    old='''        tx.execute_batch(include_str!(
            "../../migrations/020_team_content_intents.sql"
        ))
        .context("run team content intent migration")?;
'''
    new=old+'''        tx.execute_batch(include_str!("../../migrations/021_team_content_publish_target.sql"))
            .context("run stable team publish target migration")?;
'''
    assert text.count(old)==1
    return text.replace(old,new)

def core(text):
    def rep(old,new):
        nonlocal text
        assert text.count(old)==1,(old[:80],text.count(old))
        text=text.replace(old,new,1)
    rep('''    expected_remote_hash: Option<String>,
    target_doc_id: Option<String>,
    lease_token: String,
''','''    expected_remote_hash: Option<String>,
    target_doc_id: Option<String>,
    target_category: Option<String>,
    lease_token: String,
''')
    rep('''        let row: (u64, String, String, Option<String>) = tx
            .query_row(
                "SELECT o.content_revision,ki.title,ki.content,ki.siyuan_doc_id
''','''        let row: (u64, String, String, String, Option<String>) = tx
            .query_row(
                "SELECT o.content_revision,ki.title,ki.category,ki.content,ki.siyuan_doc_id
''')
    rep('''                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
''','''                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
''')
    rep('''        if row.3.is_some() {
            return Err(TeamError::Conflict);
        }
        validate_title(&row.1)?;
        validate_markdown(&row.2)?;
''','''        if let Some((_, existing, owner)) =
            read_operation_with_hash(&tx, self.company_id(), operation_id)?
        {
            if existing.knowledge_id == knowledge_id
                && existing.base_revision == base_revision
                && owner == ctx.user_id()
            {
                return Ok(existing);
            }
            return Err(TeamError::Conflict);
        }
        if row.4.is_some() {
            return Err(TeamError::Conflict);
        }
        validate_title(&row.1)?;
        validate_markdown(&row.3)?;
''')
    rep('''            &row.1,
            &row.2,
            row.0,
            None,
            None,
            now,
''','''            &row.1,
            &row.3,
            row.0,
            None,
            None,
            Some(&row.2),
            now,
''')
    rep('''            row.1.as_deref(),
            row.2.as_deref(),
            now,
''','''            row.1.as_deref(),
            row.2.as_deref(),
            None,
            now,
''')
    rep('''    target_doc_id: Option<&str>,
    expected_remote_hash: Option<&str>,
    now: u64,
''','''    target_doc_id: Option<&str>,
    expected_remote_hash: Option<&str>,
    target_category: Option<&str>,
    now: u64,
''')
    needle='''    tx.execute(
        "INSERT INTO team_audit_event(id,company_id,actor_user_id,action,resource_id,occurred_at)
'''
    assert text.count(needle)==1
    text=text.replace(needle,'''    if let Some(category) = target_category {
        tx.execute(
            "INSERT INTO team_content_publish_target(company_id,operation_id,category) VALUES (?1,?2,?3)",
            params![company_id, operation_id, category],
        )?;
    }
'''+needle,1)
    rep('''            "SELECT id,knowledge_id,owner_user_id,origin_session_id,kind,base_revision,directory_max_age,target_title,
                    target_markdown,target_hash,expected_remote_hash,target_doc_id,lease_token,attempt_count
             FROM team_content_operation WHERE company_id=?1 AND id=?2",
''','''            "SELECT o.id,o.knowledge_id,o.owner_user_id,o.origin_session_id,o.kind,o.base_revision,o.directory_max_age,o.target_title,
                    o.target_markdown,o.target_hash,o.expected_remote_hash,o.target_doc_id,o.lease_token,o.attempt_count,p.category
             FROM team_content_operation o
             LEFT JOIN team_content_publish_target p ON p.company_id=o.company_id AND p.operation_id=o.id
             WHERE o.company_id=?1 AND o.id=?2",
''')
    rep('''                    target_doc_id: r.get(11)?,
                    lease_token: r.get(12)?,
                    attempt_count: r.get(13)?,
''','''                    target_doc_id: r.get(11)?,
                    lease_token: r.get(12)?,
                    attempt_count: r.get(13)?,
                    target_category: r.get(14)?,
''')
    rep('''    let category = {
        let conn = store.db().conn();
        conn.query_row(
            "SELECT category FROM knowledge_item WHERE id=?1",
            [op.knowledge_id.as_str()],
            |r| r.get::<_, String>(0),
        )
        .map_err(|_| ProcessError::Unavailable)?
    };
''','''    let category = op
        .target_category
        .as_deref()
        .ok_or(ProcessError::Conflict)?;
''')
    rep('''    let path = sink.build_knowledge_path(&category, &op.knowledge_id, &op.target_title);
''','''    let path = sink.build_knowledge_path(category, &op.knowledge_id, &op.target_title);
''')
    rep('''        &category,
''','''        category,
''')
    return text

def fixture(text):
    def rep(old,new):
        nonlocal text
        assert text.count(old)==1,(old[:80],text.count(old))
        text=text.replace(old,new,1)
    rep('''    update_release: Semaphore,
    fail_create_response_once: Mutex<bool>,
''','''    update_release: Semaphore,
    create_seen: Semaphore,
    create_release: Semaphore,
    fail_create_response_once: Mutex<bool>,
''')
    rep('''            update_release: Semaphore::new(0),
            fail_create_response_once: Mutex::new(false),
''','''            update_release: Semaphore::new(0),
            create_seen: Semaphore::new(0),
            create_release: Semaphore::new(0),
            fail_create_response_once: Mutex::new(false),
''')
    rep('''            let mut fail = state.fail_create_response_once.lock().unwrap();
            if *fail {
                *fail = false;
                (StatusCode::SERVICE_UNAVAILABLE, "synthetic response loss").into_response()
            } else {
                JsonResponse(json!({"code":0,"msg":"","data":id})).into_response()
            }
''','''            let fail = {
                let mut flag = state.fail_create_response_once.lock().unwrap();
                let fail = *flag;
                *flag = false;
                fail
            };
            if fail {
                state.create_seen.add_permits(1);
                state.create_release.acquire().await.unwrap().forget();
                (StatusCode::SERVICE_UNAVAILABLE, "synthetic response loss").into_response()
            } else {
                JsonResponse(json!({"code":0,"msg":"","data":id})).into_response()
            }
''')
    rep('''    assert_eq!(response.status(), ReqwestStatus::ACCEPTED);
    let done = wait_state(&service, &service.owner_token, "publish-one", "done").await;
''','''    assert_eq!(response.status(), ReqwestStatus::ACCEPTED);
    tokio::time::timeout(Duration::from_secs(2), siyuan.create_seen.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();
    Connection::open(&service.path)
        .unwrap()
        .execute(
            "UPDATE knowledge_item SET category='changed-after-remote-commit' WHERE id='draft-publish'",
            [],
        )
        .unwrap();
    siyuan.create_release.add_permits(1);
    let done = wait_state(&service, &service.owner_token, "publish-one", "done").await;
''')
    rep('''    assert_eq!(siyuan.documents.lock().unwrap().len(), 1);

    service.stop().await;
''','''    assert_eq!(siyuan.documents.lock().unwrap().len(), 1);
    assert_eq!(siyuan.hpaths.lock().unwrap().len(), 1);

    let replay = service
        .auth(
            &service.owner_token,
            service.client.post(format!(
                "{}/api/v1/knowledge/draft-publish/publish",
                service.base
            )),
        )
        .json(&json!({"base_revision":1,"operation_id":"publish-one"}))
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), ReqwestStatus::ACCEPTED);
    assert_eq!(replay.json::<Value>().await.unwrap()["state"], "done");

    service.stop().await;
''')
    return text

edit("crates/aiks-core/src/storage/db.rs","64f8841b674303a7cdbb366af6f6eaf80944c3bd",db)
edit("crates/aiks-core/src/team/content.rs","780f865442015d1598a081486a39efe4254e0b20",core)
edit("apps/aiks-service/tests/team_content.rs","b774f8604b65513e66224d8bd63cc09feaf03b95",fixture)
print("Applied guarded stable publish target candidate; branch unchanged by this workflow.")
