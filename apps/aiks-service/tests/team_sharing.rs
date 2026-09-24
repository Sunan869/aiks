mod team_support;

use reqwest::StatusCode;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use team_support::TeamTestService;

#[tokio::test]
async fn owner_can_replace_read_grants_without_exposing_the_private_source_session() {
    let service = TeamTestService::start().await;
    let receipt = service
        .ingest(
            &service.owner_token,
            &service.owner_space,
            "PRIVATE_OWNER_SOURCE",
        )
        .await;
    service
        .wait_job(&service.owner_token, receipt["job_id"].as_str().unwrap())
        .await;
    let session_id: i64 = receipt["session_id"].as_str().unwrap().parse().unwrap();
    {
        let db = Connection::open(&service.path).unwrap();
        db.execute(
            "INSERT INTO knowledge_item(id,source_session_id,title,category,summary,content,tags,created_at,updated_at)
             VALUES ('shared-k',?1,'Shared title','implementation','Shared summary','SHARED_BODY','[]','test','test')",
            [session_id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO service_knowledge_revision(knowledge_id,session_id,revision) VALUES ('shared-k',?1,1)",
            [session_id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO team_knowledge_owner(company_id,knowledge_id,owner_user_id) VALUES (?1,'shared-k',?2)",
            params![service.store.company_id(), service.owner_id],
        )
        .unwrap();
    }

    let initial: Value = service
        .auth(
            &service.owner_token,
            service
                .client
                .get(format!("{}/api/v1/knowledge/shared-k/shares", service.base)),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(initial["grant_version"], 0);
    assert!(initial["grants"].as_array().unwrap().is_empty());

    let response = service
        .auth(
            &service.owner_token,
            service.client.put(format!(
                "{}/api/v1/knowledge/shared-k/shares",
                service.base
            )),
        )
        .json(&json!({
            "expected_grant_version":0,
            "grants":[
                {"target_type":"user","target_id":service.reader_id,"permission":"read"},
                {"target_type":"org","target_id":service.research_id,"include_descendants":false,"permission":"read"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.json::<Value>().await.unwrap()["grant_version"], 1);

    let reader = service
        .auth(
            &service.reader_token,
            service
                .client
                .get(format!("{}/api/v1/knowledge/shared-k", service.base)),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(reader.status(), 200);
    let reader_body: Value = reader.json().await.unwrap();
    assert_eq!(reader_body["content"], "SHARED_BODY");
    assert!(!reader_body.to_string().contains("PRIVATE_OWNER_SOURCE"));
    assert_eq!(reader_body["can_manage"], false);
    assert_eq!(reader_body["share_source"], "shared_to_me");

    let owner_body: Value = service
        .auth(
            &service.owner_token,
            service
                .client
                .get(format!("{}/api/v1/knowledge/shared-k", service.base)),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(owner_body["can_manage"], true);
    assert_eq!(owner_body["share_source"], "mine");

    let child = service
        .auth(
            &service.child_token,
            service
                .client
                .get(format!("{}/api/v1/knowledge/shared-k", service.base)),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(child.status(), StatusCode::NOT_FOUND);

    let raw_source = service
        .auth(
            &service.reader_token,
            service.client.get(format!(
                "{}/api/v1/sessions/{}",
                service.base,
                receipt["session_id"].as_str().unwrap()
            )),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(raw_source.status(), StatusCode::NOT_FOUND);

    let non_owner_share = service
        .auth(
            &service.reader_token,
            service
                .client
                .put(format!("{}/api/v1/knowledge/shared-k/shares", service.base)),
        )
        .json(&json!({"expected_grant_version":1,"grants":[]}))
        .send()
        .await
        .unwrap();
    assert_eq!(non_owner_share.status(), StatusCode::FORBIDDEN);

    let invalid = service
        .auth(
            &service.owner_token,
            service
                .client
                .put(format!("{}/api/v1/knowledge/shared-k/shares", service.base)),
        )
        .json(&json!({
            "expected_grant_version":1,
            "grants":[{"target_type":"user","target_id":service.reader_id,"permission":"edit"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let org_only = service
        .auth(
            &service.owner_token,
            service.client.put(format!(
                "{}/api/v1/knowledge/shared-k/shares",
                service.base
            )),
        )
        .json(&json!({
            "expected_grant_version":1,
            "grants":[{"target_type":"org","target_id":service.research_id,"include_descendants":false,"permission":"read"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(org_only.status(), 200);
    assert_eq!(org_only.json::<Value>().await.unwrap()["grant_version"], 2);
    assert_eq!(
        service
            .auth(
                &service.reader_token,
                service
                    .client
                    .get(format!("{}/api/v1/knowledge/shared-k", service.base))
            )
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        service
            .auth(
                &service.child_token,
                service
                    .client
                    .get(format!("{}/api/v1/knowledge/shared-k", service.base))
            )
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    let descendants = service
        .auth(
            &service.owner_token,
            service.client.put(format!("{}/api/v1/knowledge/shared-k/shares", service.base)),
        )
        .json(&json!({
            "expected_grant_version":2,
            "grants":[{"target_type":"org","target_id":service.research_id,"include_descendants":true,"permission":"read"}]
        }))
        .send().await.unwrap();
    assert_eq!(descendants.status(), 200);
    assert_eq!(
        descendants.json::<Value>().await.unwrap()["grant_version"],
        3
    );
    assert_eq!(
        service
            .auth(
                &service.child_token,
                service
                    .client
                    .get(format!("{}/api/v1/knowledge/shared-k", service.base))
            )
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let child_body: Value = service
        .auth(
            &service.child_token,
            service
                .client
                .get(format!("{}/api/v1/knowledge/shared-k", service.base)),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(child_body["can_manage"], false);
    assert_eq!(child_body["share_source"], "department");

    let reader_list: Value = service
        .auth(
            &service.reader_token,
            service
                .client
                .get(format!("{}/api/v1/knowledge", service.base)),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let shared = reader_list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "shared-k")
        .unwrap();
    assert_eq!(shared["can_manage"], false);
    assert_eq!(shared["share_source"], "shared_to_me");

    let private = service
        .auth(
            &service.owner_token,
            service
                .client
                .put(format!("{}/api/v1/knowledge/shared-k/shares", service.base)),
        )
        .json(&json!({"expected_grant_version":3,"grants":[]}))
        .send()
        .await
        .unwrap();
    assert_eq!(private.status(), 200);
    assert_eq!(private.json::<Value>().await.unwrap()["grant_version"], 4);
    assert_eq!(
        service
            .auth(
                &service.reader_token,
                service
                    .client
                    .get(format!("{}/api/v1/knowledge/shared-k", service.base))
            )
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    let stale = service
        .auth(
            &service.owner_token,
            service
                .client
                .put(format!("{}/api/v1/knowledge/shared-k/shares", service.base)),
        )
        .json(&json!({"expected_grant_version":3,"grants":[]}))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    service.stop().await;
}
