use std::time::Duration;
use reqwest::StatusCode;
use serde_json::{json, Value};
mod support;
use support::RunningService;

#[path = "../../../crates/aiks-core/tests/support/service_fixture.rs"]
mod fixture;

#[tokio::test]
async fn authenticated_upload_replay_worker_and_keyword_search_form_a_real_offline_flow() {
    let s = RunningService::start().await;
    let reg = s.registration().await;
    assert_eq!(reg, s.registration().await);
    let mut input = fixture::submission(&s.space_id, &s.instance_id, &reg, "first", 0, "HTTP_OFFLINE_UNIQUE");
    let endpoint = format!("{}/api/v1/session-snapshots", s.base);
    let first = s.auth(s.client.post(&endpoint)).json(&input).send().await.unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let receipt: Value = first.json().await.unwrap();
    let retry = s.auth(s.client.post(&endpoint)).json(&input).send().await.unwrap();
    assert_eq!(retry.status(), StatusCode::OK);
    assert_eq!(retry.json::<Value>().await.unwrap(), receipt);
    input.submission_id = "different-receipt".into();
    input.expected_revision = 1;
    let unchanged = s.auth(s.client.post(&endpoint)).json(&input).send().await.unwrap();
    assert_eq!(unchanged.status(), StatusCode::ACCEPTED, "new receipt is not an exact replay");
    let unchanged: Value = unchanged.json().await.unwrap();
    assert_ne!(unchanged["receipt_id"], receipt["receipt_id"]);
    assert_eq!(unchanged["job_id"], receipt["job_id"]);
    let job = format!("{}/api/v1/jobs/{}", s.base, receipt["job_id"].as_str().unwrap());
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let value: Value = s.auth(s.client.get(&job)).send().await.unwrap().json().await.unwrap();
            if value["status"] == "DONE" { break; }
            assert_ne!(value["status"], "FAILED");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.unwrap();
    let results: Value = s.auth(s.client.post(format!("{}/api/v1/search", s.base)))
        .json(&json!({"query":"HTTP_OFFLINE_UNIQUE","limit":10,"corpora":["session"]}))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(results["hits"].as_array().unwrap().len(), 1);
    assert_eq!(results["hits"][0]["entity_id"], receipt["session_id"]);
    assert_eq!(results["hits"][0]["revision"], 1);
    let detail: Value = s.auth(s.client.get(format!("{}/api/v1/sessions/{}", s.base, receipt["session_id"].as_str().unwrap())))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(detail["revision"], 1);
    assert!(detail.to_string().contains("HTTP_OFFLINE_UNIQUE"));
    let stored: Value = s.auth(s.client.get(format!("{}/api/v1/receipts/{}", s.base, receipt["receipt_id"].as_str().unwrap())))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(stored, receipt);
    let db = rusqlite::Connection::open(&s.path).unwrap();
    for table in ["service_session_snapshot", "pipeline_job"] {
        let count: i64 = db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }
    drop(db);
    s.stop().await;
}

#[tokio::test]
async fn authentication_host_origin_instance_and_route_boundaries_are_enforced() {
    let s = RunningService::start().await;
    let url = format!("{}/api/v1/capabilities", s.base);
    let requests = vec![
        s.client.get(&url),
        s.client.get(&url).bearer_auth("incorrect"),
        s.client.get(&url).bearer_auth(&s.token),
        s.client.get(&url).bearer_auth(&s.token).header("X-AIKS-Instance-ID", "wrong-instance"),
        s.auth(s.client.get(&url)).header("Origin", "https://untrusted.invalid"),
        s.auth(s.client.get(&url)).header("Host", "untrusted.invalid"),
        s.auth(s.client.get(&url)).header("Host", "localhost"),
    ];
    for request in requests {
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["error"]["code"], "unauthorized");
        assert!(body["error"]["request_id"].is_string());
        assert!(!body.to_string().contains(&s.token));
    }
    let caps: Value = s.auth(s.client.get(&url)).send().await.unwrap().json().await.unwrap();
    assert_eq!(caps["instance_id"], s.instance_id);
    assert_eq!(caps["team"], false);
    assert_eq!(caps["content_write"], false);
    assert_eq!(caps["rag"], false);
    assert!(!caps.to_string().contains("token"));
    for path in ["/api/query/sql", "/proxy/api/query/sql", "/api/v1/files", "/api/v1/shutdown"] {
        let response = s.auth(s.client.post(format!("{}{path}", s.base))).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.json::<Value>().await.unwrap()["error"]["code"], "not_found");
    }
    assert_eq!(s.auth(s.client.get(format!("{url}?token=forbidden"))).send().await.unwrap().status(), StatusCode::BAD_REQUEST);
    let health: Value = s.client.get(format!("{}/healthz", s.base)).send().await.unwrap().json().await.unwrap();
    assert_eq!(health, json!({"status":"ok"}));
    s.stop().await;
}

#[tokio::test]
async fn invalid_requests_are_bounded_and_do_not_echo_private_payloads() {
    let s = RunningService::start().await;
    let reg = s.registration().await;
    let endpoint = format!("{}/api/v1/session-snapshots", s.base);
    let input = fixture::submission(&s.space_id, &s.instance_id, &reg, "u1", 0, "PRIVATE_SYNTHETIC_NEEDLE");
    for (change, status, code) in [
        (json!({"complete":false}), 422, "incomplete_snapshot"),
        (json!({"space_id":"wrong-space"}), 404, "not_found"),
        (json!({"api_version":999}), 400, "unsupported_version"),
        (json!({"unexpected":"PRIVATE_SYNTHETIC_NEEDLE"}), 400, "invalid_input"),
    ] {
        let mut value = serde_json::to_value(&input).unwrap();
        value.as_object_mut().unwrap().extend(change.as_object().unwrap().clone());
        let response = s.auth(s.client.post(&endpoint)).json(&value).send().await.unwrap();
        assert_eq!(response.status().as_u16(), status);
        let error: Value = response.json().await.unwrap();
        assert_eq!(error["error"]["code"], code);
        assert!(!error.to_string().contains("PRIVATE_SYNTHETIC_NEEDLE"));
    }
    let response = s.auth(s.client.post(&endpoint)).header("Content-Encoding", "gzip").json(&input).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = s.auth(s.client.post(&endpoint)).header("Content-Type", "application/json")
        .body(vec![b' '; 16 * 1024 * 1024 + 1]).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(response.json::<Value>().await.unwrap()["error"]["code"], "too_large");
    let response = s.auth(s.client.get(format!("{}/api/v1/sessions?limit=101", s.base))).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    s.stop().await;
}
