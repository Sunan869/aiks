use axum::{routing::post, Json, Router};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};
mod support;
use support::RunningService;

struct Upstream { base: String, task: tokio::task::JoinHandle<()> }
impl Drop for Upstream { fn drop(&mut self) { self.task.abort(); } }
async fn upstream(app: Router) -> Upstream {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
    Upstream { base, task }
}

#[tokio::test]
async fn enabled_assist_uses_canonical_content_and_returns_an_unsaved_suggestion() {
    let seen = Arc::new(AtomicUsize::new(0));
    let count = seen.clone();
    let remote = upstream(Router::new()
        .route("/api/block/getBlockKramdown", post(|| async {
            Json(json!({"code":0,"msg":"","data":{"kramdown":"# CANONICAL_ASSIST_INPUT"}}))
        }))
        .route("/v1/chat/completions", post(move |Json(request): Json<Value>| {
            let number = count.fetch_add(1, Ordering::SeqCst);
            async move {
                let content = if number == 0 {
                    // The real snapshot Worker also uses this model. Its empty
                    // extraction must complete normally before assist is invoked.
                    json!({"session_summary":"Synthetic source","knowledge_score":0.0,"worth_extracting":false,"items":[]})
                } else {
                    let prompt = request.to_string();
                    assert!(prompt.contains("CANONICAL_ASSIST_INPUT"));
                    assert!(!prompt.contains("SQLITE_DRAFT_ONLY"));
                    json!({"summary":"Canonical summary suggestion","title":null,"tags":[],"category":null,"text":null})
                };
                Json(json!({"choices":[{"message":{"content":content.to_string()}}]}))
            }
        }))).await;
    let s = RunningService::configured(|config| {
        config.siyuan.base_url = remote.base.clone();
        config.ai.base_url = format!("{}/v1", remote.base);
        config.ai.model = "synthetic-assist-model".into();
        config.ai.enabled = true;
    }).await;
    let receipt = s.ingest_ready("Synthetic source").await;
    s.seed_knowledge(&receipt, "published", Some("mapped-doc"));
    let db = Connection::open(&s.path).unwrap();
    db.execute("INSERT INTO service_knowledge_revision(knowledge_id,session_id,revision) VALUES ('published',?1,1)",
        [receipt["session_id"].as_str().unwrap()]).unwrap();
    let response = s.auth(s.client.post(format!("{}/api/v1/knowledge/published/assist", s.base)))
        .json(&json!({"operation":"summary"})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let output: Value = response.json().await.unwrap();
    assert_eq!(output["saved"], false);
    assert_eq!(output["revision"], 1);
    assert_eq!(output["suggestion"]["summary"], "Canonical summary suggestion");
    assert_eq!(seen.load(Ordering::SeqCst), 2);
    let stored: String = db.query_row("SELECT summary FROM knowledge_item WHERE id='published'", [], |row| row.get(0)).unwrap();
    assert_eq!(stored, "Synthetic summary");
    drop(db);
    s.stop().await;
}

#[tokio::test]
async fn semantic_recall_scopes_both_corpora_before_the_vector_candidate_limit() {
    let remote = upstream(Router::new().route("/v1/embeddings", post(|Json(input): Json<Value>| async move {
        let data: Vec<Value> = input["input"].as_array().unwrap().iter().enumerate()
            .map(|(index, _)| json!({"index":index,"embedding":[1.0,0.0,0.0]})).collect();
        Json(json!({"data":data}))
    }))).await;
    let s = RunningService::configured(|config| {
        config.embedding.enabled = true;
        config.embedding.model = "synthetic-vector".into();
        config.embedding.base_url = format!("{}/v1", remote.base);
        config.embedding.dimensions = Some(3);
    }).await;
    let vector: Vec<u8> = [1.0_f32,0.0,0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    {
        let mut db = Connection::open(&s.path).unwrap();
        let tx = db.transaction().unwrap();
        tx.execute("INSERT INTO source_session(source,external_session_id,title,last_seen_at,created_at,updated_at)
                    VALUES ('continue','legacy','PRIVATE_VECTOR_SOURCE','test','test','test')", []).unwrap();
        let id = tx.last_insert_rowid();
        tx.execute("INSERT INTO session_index_state(session_id,status) VALUES (?1,'ready')", [id]).unwrap();
        tx.execute("INSERT INTO knowledge_item(id,source_session_id,title,summary,content,created_at,updated_at)
                    VALUES ('legacy-knowledge',?1,'PRIVATE_VECTOR_KNOWLEDGE','Secret','Secret','test','test')", [id]).unwrap();
        // 4096 is the existing per-corpus vector budget. Unauthorized rows
        // precede authorized data and must not displace it from either query.
        for n in 0..4100 {
            let key = format!("legacy-{n}");
            tx.execute("INSERT INTO session_search_chunk(id,session_id,chunk_index,text,content_hash,created_at)
                        VALUES (?1,?2,?3,'PRIVATE_VECTOR_SOURCE','hash','test')", params![key,id,n]).unwrap();
            tx.execute("INSERT INTO session_embedding_record(id,chunk_id,model,dimensions,vector,created_at)
                        VALUES (?1,?1,'synthetic-vector',3,?2,'test')", params![key,vector]).unwrap();
            tx.execute("INSERT INTO knowledge_chunk(id,knowledge_id,chunk_index,text,content_hash,created_at)
                        VALUES (?1,'legacy-knowledge',?2,'PRIVATE_VECTOR_KNOWLEDGE','hash','test')", params![key,n]).unwrap();
            tx.execute("INSERT INTO embedding_record(id,chunk_id,model,dimensions,vector,created_at)
                        VALUES (?1,?1,'synthetic-vector',3,?2,'test')", params![key,vector]).unwrap();
        }
        tx.commit().unwrap();
    }
    let receipt = s.ingest_ready("Authorized session body").await;
    s.seed_knowledge(&receipt, "authorized-knowledge", Some("mapped-doc"));
    let db = Connection::open(&s.path).unwrap();
    db.execute("INSERT INTO service_knowledge_revision(knowledge_id,session_id,revision) VALUES ('authorized-knowledge',?1,1)",
        [receipt["session_id"].as_str().unwrap()]).unwrap();
    db.execute("INSERT INTO knowledge_chunk(id,knowledge_id,chunk_index,text,content_hash,created_at)
                VALUES ('authorized-chunk','authorized-knowledge',0,'Authorized vector text','hash','test')", []).unwrap();
    db.execute("INSERT INTO embedding_record(id,chunk_id,model,dimensions,vector,created_at)
                VALUES ('authorized-vector','authorized-chunk','synthetic-vector',3,?1,'test')", [&vector]).unwrap();
    for stale in [false, true] {
        if stale { db.execute("DELETE FROM service_knowledge_revision WHERE knowledge_id='authorized-knowledge'", []).unwrap(); }
        let response = s.auth(s.client.post(format!("{}/api/v1/search", s.base)))
            .json(&json!({"query":"vectoronlyneedle","limit":10})).send().await.unwrap();
        assert_eq!(response.status(), 200);
        let output: Value = response.json().await.unwrap();
        let hits = output["hits"].as_array().unwrap();
        assert_eq!(hits.len(), if stale {1} else {2});
        assert!(hits.iter().any(|hit| hit["entity_id"] == receipt["session_id"] && hit["corpus"] == "session"));
        for hit in hits { assert!(hit["match_types"].as_array().unwrap().contains(&json!("semantic"))); }
        assert!(!output.to_string().contains("PRIVATE_VECTOR"));
    }
    drop(db);
    s.stop().await;
}
