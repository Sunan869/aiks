mod team_support;

use reqwest::StatusCode;
use serde_json::Value;
use team_support::TeamTestService;

#[tokio::test]
async fn directory_search_returns_only_safe_current_targets() {
    let service = TeamTestService::start().await;

    let users = service
        .auth(
            &service.owner_token,
            service
                .client
                .get(format!("{}/api/v1/directory/search?q=Reader", service.base)),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(users.status(), StatusCode::OK);
    let users: Value = users.json().await.unwrap();
    assert_eq!(users["items"].as_array().unwrap().len(), 1);
    assert_eq!(users["items"][0]["target_type"], "user");
    assert_eq!(users["items"][0]["target_id"], service.reader_id);
    assert_eq!(users["items"][0]["display_name"], "Reader");
    let serialized = users.to_string();
    assert!(!serialized.contains("union-reader"));
    assert!(!serialized.contains("employee-reader"));

    let orgs = service
        .auth(
            &service.reader_token,
            service.client.get(format!(
                "{}/api/v1/directory/search?q=Research",
                service.base
            )),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(orgs.status(), StatusCode::OK);
    let orgs: Value = orgs.json().await.unwrap();
    assert_eq!(orgs["items"].as_array().unwrap().len(), 1);
    assert_eq!(orgs["items"][0]["target_type"], "org");
    assert_eq!(orgs["items"][0]["target_id"], service.research_id);

    let empty = service
        .auth(
            &service.owner_token,
            service
                .client
                .get(format!("{}/api/v1/directory/search?q=", service.base)),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::BAD_REQUEST);
    service.stop().await;
}
