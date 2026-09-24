mod team_support;

use reqwest::StatusCode;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use team_support::TeamTestService;

async fn search(service: &TeamTestService, token: &str, query: &str) -> Value {
    let response = service
        .auth(
            token,
            service
                .client
                .post(format!("{}/api/v1/search", service.base)),
        )
        .json(&json!({"query":query,"limit":10,"corpora":["session"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    response.json().await.unwrap()
}

#[tokio::test]
async fn trusted_team_context_partitions_ingest_ids_reads_and_search_before_candidate_limits() {
    let service = TeamTestService::start().await;
    let owner = service
        .ingest(
            &service.owner_token,
            &service.owner_space,
            "scopeprobe OWNER_ONLY",
        )
        .await;
    let reader = service
        .ingest(
            &service.reader_token,
            &service.reader_space,
            "scopeprobe READER_ONLY",
        )
        .await;
    assert_ne!(owner["session_id"], reader["session_id"]);
    service
        .wait_job(&service.owner_token, owner["job_id"].as_str().unwrap())
        .await;
    service
        .wait_job(&service.reader_token, reader["job_id"].as_str().unwrap())
        .await;

    for (token, foreign) in [
        (&service.owner_token, &reader),
        (&service.reader_token, &owner),
    ] {
        for (path, field) in [
            ("sessions", "session_id"),
            ("receipts", "receipt_id"),
            ("jobs", "job_id"),
        ] {
            let response = service
                .auth(
                    token,
                    service.client.get(format!(
                        "{}/api/v1/{path}/{}",
                        service.base,
                        foreign[field].as_str().unwrap()
                    )),
                )
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    let owner_list: Value = service
        .auth(
            &service.owner_token,
            service
                .client
                .get(format!("{}/api/v1/sessions", service.base)),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(owner_list["items"].as_array().unwrap().len(), 1);
    assert_eq!(owner_list["items"][0]["session_id"], owner["session_id"]);

    {
        let mut db = Connection::open(&service.path).unwrap();
        let tx = db.transaction().unwrap();
        for n in 0..1100 {
            tx.execute(
                "INSERT INTO source_session(source,external_session_id,title,last_seen_at,created_at,updated_at)
                 VALUES ('continue',?1,'scopeprobe PRIVATE_LEGACY','test','test','test')",
                [format!("legacy-{n}")],
            )
            .unwrap();
            let id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO session_search_fts(session_id,external_id,source,title,content)
                 VALUES (?1,?2,'continue','scopeprobe PRIVATE_LEGACY','scopeprobe PRIVATE_LEGACY')",
                params![id, format!("legacy-{n}")],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let result = search(&service, &service.owner_token, "scopeprobe").await;
    let hits = result["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["entity_id"], owner["session_id"]);
    assert!(!result.to_string().contains("PRIVATE_LEGACY"));
    assert!(
        search(&service, &service.owner_token, "READER_ONLY").await["hits"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let unauthenticated = service
        .client
        .get(format!("{}/api/v1/sessions", service.base))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    service.stop().await;
}
