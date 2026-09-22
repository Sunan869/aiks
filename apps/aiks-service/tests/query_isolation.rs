use serde_json::{json, Value};
use rusqlite::{params, Connection};
mod support;
use support::RunningService;

async fn search(s: &RunningService, query: &str) -> Value {
    let response = s.auth(s.client.post(format!("{}/api/v1/search", s.base)))
        .json(&json!({"query":query,"limit":10,"corpora":["session"]})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    response.json().await.unwrap()
}

#[tokio::test]
async fn unbound_legacy_rows_cannot_fill_the_candidate_budget_or_leak_via_fallbacks() {
    let s = RunningService::start().await;
    {
        let mut db = Connection::open(&s.path).unwrap();
        let tx = db.transaction().unwrap();
        for n in 0..1100 {
            tx.execute("INSERT INTO source_session(source,external_session_id,title,last_seen_at,created_at,updated_at)
                        VALUES ('continue',?1,'scopeprobe PRIVATE_LEGACY','test','test','test')", [format!("legacy-{n}")]).unwrap();
            let id = tx.last_insert_rowid();
            tx.execute("INSERT INTO session_search_fts(session_id,external_id,source,title,content)
                        VALUES (?1,?2,'continue','scopeprobe PRIVATE_LEGACY','scopeprobe PRIVATE_LEGACY')", params![id,format!("legacy-{n}")]).unwrap();
        }
        tx.commit().unwrap();
    }
    let receipt = s.ingest_ready("scopeprobe AUTHORIZED_BODY").await;
    let result = search(&s, "scopeprobe").await;
    assert_eq!(result["hits"].as_array().unwrap().len(), 1);
    assert_eq!(result["hits"][0]["entity_id"], receipt["session_id"]);
    assert!(!result.to_string().contains("PRIVATE_LEGACY"));
    assert!(search(&s, "PRIVATE_LEGACY").await["hits"].as_array().unwrap().is_empty());
    let db = Connection::open(&s.path).unwrap();
    db.execute("UPDATE source_session SET title='scopeprobe Current title' WHERE id=?1", [receipt["session_id"].as_str().unwrap()]).unwrap();
    db.execute_batch("DROP TABLE session_search_fts").unwrap();
    let result = search(&s, "scopeprobe").await;
    assert_eq!(result["hits"].as_array().unwrap().len(), 1);
    assert_eq!(result["degraded"], true);
    assert!(!result.to_string().contains("PRIVATE_LEGACY"));
    db.execute("UPDATE service_derived_state SET indexed_revision=NULL", []).unwrap();
    assert!(search(&s, "scopeprobe").await["hits"].as_array().unwrap().is_empty());
    drop(db);
    s.stop().await;
}

#[tokio::test]
async fn stale_index_never_combines_old_body_with_current_metadata() {
    let s = RunningService::start().await;
    let receipt = s.ingest_ready("STALE_BODY_NEEDLE").await;
    let db = Connection::open(&s.path).unwrap();
    db.execute("UPDATE service_derived_state SET indexed_revision=NULL", []).unwrap();
    db.execute("UPDATE source_session SET title='New unindexed title' WHERE id=?1", [receipt["session_id"].as_str().unwrap()]).unwrap();
    let response = search(&s, "STALE_BODY_NEEDLE").await;
    assert!(response["hits"].as_array().unwrap().is_empty());
    let list: Value = s.auth(s.client.get(format!("{}/api/v1/sessions", s.base))).send().await.unwrap().json().await.unwrap();
    assert_eq!(list["items"][0]["index_current"], false);
    assert_eq!(list["items"][0]["title"], "New unindexed title");
    drop(db);
    s.stop().await;
}

#[tokio::test]
async fn foreign_instance_receipts_jobs_and_unbound_knowledge_are_not_readable() {
    let a = RunningService::start().await;
    let receipt = a.ingest_ready("PRIVATE_INSTANCE_A").await;
    let b = RunningService::start().await;
    for (path, field) in [("receipts","receipt_id"),("jobs","job_id"),("sessions","session_id")] {
        let url = format!("{}/api/v1/{path}/{}", b.base, receipt[field].as_str().unwrap());
        assert_eq!(b.auth(b.client.get(&url)).send().await.unwrap().status(), 404);
        assert_eq!(b.client.get(&url).bearer_auth(&a.token).header("X-AIKS-Instance-ID", &a.instance_id).send().await.unwrap().status(), 401);
    }
    assert!(search(&b, "PRIVATE_INSTANCE_A").await["hits"].as_array().unwrap().is_empty());
    b.stop().await;
    a.stop().await;
}
