mod team_support;

use reqwest::StatusCode;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use team_support::TeamTestService;

#[tokio::test]
async fn personal_import_is_private_owned_and_idempotent_without_identity_override() {
    let service = TeamTestService::start().await;
    let request = json!({
        "operation_id":"import-op-1",
        "title":"Imported knowledge",
        "markdown":"PRIVATE_IMPORTED_BODY",
        "source_fingerprint":"local-knowledge-fingerprint-1"
    });
    let first = service
        .auth(
            &service.owner_token,
            service
                .client
                .post(format!("{}/api/v1/knowledge/import", service.base)),
        )
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    let first: Value = first.json().await.unwrap();
    let knowledge_id = first["knowledge_id"].as_str().unwrap().to_owned();
    assert_eq!(first["operation_id"], "import-op-1");
    assert_eq!(first["content_revision"], 1);

    let replay = service
        .auth(
            &service.owner_token,
            service
                .client
                .post(format!("{}/api/v1/knowledge/import", service.base)),
        )
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(replay.json::<Value>().await.unwrap(), first);

    let duplicate_source = service
        .auth(
            &service.owner_token,
            service
                .client
                .post(format!("{}/api/v1/knowledge/import", service.base)),
        )
        .json(&json!({
            "operation_id":"import-op-2",
            "title":"Imported knowledge",
            "markdown":"PRIVATE_IMPORTED_BODY",
            "source_fingerprint":"local-knowledge-fingerprint-1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate_source.status(), StatusCode::OK);
    assert_eq!(duplicate_source.json::<Value>().await.unwrap(), first);

    let owner_read: Value = service
        .auth(
            &service.owner_token,
            service
                .client
                .get(format!("{}/api/v1/knowledge/{knowledge_id}", service.base)),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(owner_read["content"], "PRIVATE_IMPORTED_BODY");
    assert_eq!(owner_read["content_revision"], 1);

    let shares: Value = service
        .auth(
            &service.owner_token,
            service.client.get(format!(
                "{}/api/v1/knowledge/{knowledge_id}/shares",
                service.base
            )),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(shares["grant_version"], 0);
    assert!(shares["grants"].as_array().unwrap().is_empty());

    let reader = service
        .auth(
            &service.reader_token,
            service
                .client
                .get(format!("{}/api/v1/knowledge/{knowledge_id}", service.base)),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(reader.status(), StatusCode::NOT_FOUND);

    let conflict = service
        .auth(
            &service.owner_token,
            service
                .client
                .post(format!("{}/api/v1/knowledge/import", service.base)),
        )
        .json(&json!({
            "operation_id":"import-op-1",
            "title":"Imported knowledge",
            "markdown":"CHANGED",
            "source_fingerprint":"local-knowledge-fingerprint-1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);

    let identity_override = service
        .auth(
            &service.owner_token,
            service
                .client
                .post(format!("{}/api/v1/knowledge/import", service.base)),
        )
        .json(&json!({
            "operation_id":"import-op-3",
            "title":"No override",
            "markdown":"body",
            "source_fingerprint":"local-knowledge-fingerprint-3",
            "owner_user_id":service.reader_id,
            "company_id":"attacker-company"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(identity_override.status(), StatusCode::BAD_REQUEST);

    let db = Connection::open(&service.path).unwrap();
    let (owner, source_session, import_count): (String, Option<i64>, i64) = db
        .query_row(
            "SELECT o.owner_user_id,ki.source_session_id,
                    (SELECT COUNT(*) FROM team_knowledge_import i
                     WHERE i.company_id=o.company_id AND i.knowledge_id=o.knowledge_id)
             FROM team_knowledge_owner o
             JOIN knowledge_item ki ON ki.id=o.knowledge_id
             WHERE o.company_id=?1 AND o.knowledge_id=?2",
            params![service.store.company_id(), knowledge_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(owner, service.owner_id);
    assert!(source_session.is_none());
    assert_eq!(import_count, 1);
    drop(db);
    service.stop().await;
}

#[tokio::test]
async fn import_budget_and_source_fingerprint_conflicts_fail_closed() {
    let service = TeamTestService::start().await;
    let too_large = "x".repeat(aiks_core::team::MAX_TEAM_CONTENT_BYTES + 1);
    let response = service
        .auth(
            &service.owner_token,
            service
                .client
                .post(format!("{}/api/v1/knowledge/import", service.base)),
        )
        .json(&json!({
            "operation_id":"large-import",
            "title":"Large",
            "markdown":too_large,
            "source_fingerprint":"large-source"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let first = service
        .auth(
            &service.owner_token,
            service
                .client
                .post(format!("{}/api/v1/knowledge/import", service.base)),
        )
        .json(&json!({
            "operation_id":"fingerprint-1",
            "title":"One",
            "markdown":"body-one",
            "source_fingerprint":"same-source"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);

    let changed = service
        .auth(
            &service.owner_token,
            service
                .client
                .post(format!("{}/api/v1/knowledge/import", service.base)),
        )
        .json(&json!({
            "operation_id":"fingerprint-2",
            "title":"Two",
            "markdown":"body-two",
            "source_fingerprint":"same-source"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(changed.status(), StatusCode::CONFLICT);
    service.stop().await;
}
