use aiks_core::team::{
    provider::ProviderFuture, DirectorySnapshot, DirectoryUser, ExternalLogin, IdentityProvider,
    Membership, OrgRecord, TeamError,
};
use aiks_service::{
    team::{config::validate_static, server::TeamServer},
    ServiceConfig,
};
use reqwest::{header, Client, StatusCode, Url};
use serde_json::{json, Value};
use sha2::Digest;
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::oneshot;

struct SyntheticProvider;

impl IdentityProvider for SyntheticProvider {
    fn exchange_code<'a>(&'a self, code: &'a str) -> ProviderFuture<'a, ExternalLogin> {
        Box::pin(async move {
            let (external_user_id, union_id, display_name) = match code {
                "owner-code" => ("employee-owner", "union-owner", "Owner"),
                "reader-code" => ("employee-reader", "union-reader", "Reader"),
                _ => return Err(TeamError::Unauthorized),
            };
            Ok(ExternalLogin {
                corp_id: "synthetic-corp".into(),
                external_user_id: external_user_id.into(),
                union_id: union_id.into(),
                display_name: display_name.into(),
            })
        })
    }

    fn directory<'a>(&'a self, scope: &'a [String]) -> ProviderFuture<'a, DirectorySnapshot> {
        Box::pin(async move {
            if scope.len() != 1 || scope[0] != "1" {
                return Err(TeamError::InvalidInput);
            }
            Ok(DirectorySnapshot {
                complete: true,
                scope: vec!["1".into()],
                users: vec![
                    DirectoryUser {
                        external_user_id: "employee-owner".into(),
                        union_id: "union-owner".into(),
                        display_name: "Owner".into(),
                        active: true,
                    },
                    DirectoryUser {
                        external_user_id: "employee-reader".into(),
                        union_id: "union-reader".into(),
                        display_name: "Reader".into(),
                        active: true,
                    },
                ],
                orgs: vec![OrgRecord {
                    id: "1".into(),
                    parent_id: None,
                    name: "Company".into(),
                }],
                memberships: vec![
                    Membership {
                        user_id: "employee-owner".into(),
                        org_id: "1".into(),
                    },
                    Membership {
                        user_id: "employee-reader".into(),
                        org_id: "1".into(),
                    },
                ],
                observed_at: now(),
            })
        })
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn config(database: &std::path::Path) -> ServiceConfig {
    toml::from_str(&format!(
        r#"
mode = "team"
database = {}
listen = "127.0.0.1:28081"

[team]
enabled = true
public_base_url = "https://aiks.example.test"

[team.dingtalk]
enabled = true
corp_id = "synthetic-corp"
client_id = "synthetic-client"
redirect_uri = "https://aiks.example.test/api/v1/auth/dingtalk/callback"
client_secret_env = "AIKS_DINGTALK_CLIENT_SECRET"

[team.directory]
root_department_ids = ["1"]
refresh_interval_seconds = 300
max_stale_seconds = 900

[siyuan]
base_url = "http://127.0.0.1:6806"
"#,
        serde_json::to_string(database).unwrap()
    ))
    .unwrap()
}

async fn wait_for_directory(database: &std::path::Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let path = database.to_owned();
            let generation = tokio::task::spawn_blocking(move || {
                let conn = rusqlite::Connection::open(path).unwrap();
                conn.query_row(
                    "SELECT directory_generation FROM team_company WHERE singleton=1",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap_or(0)
            })
            .await
            .unwrap();
            if generation > 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

async fn login(client: &Client, base: &str, code: &str) -> (String, String) {
    let label = code.trim_end_matches("-code");
    let verifier = hex::encode(sha2::Sha256::digest(format!("verifier:{label}").as_bytes()));
    let verifier_hash = hex::encode(sha2::Sha256::digest(verifier.as_bytes()));
    let start = client
        .post(format!("{base}/api/v1/auth/dingtalk/start"))
        .header(header::HOST, "aiks.example.test")
        .json(&json!({"verifier_hash":verifier_hash}))
        .send()
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::OK);
    let start: Value = start.json().await.unwrap();
    let attempt = start["attempt_id"].as_str().unwrap().to_owned();
    let handoff = Url::parse(start["authorize_url"].as_str().unwrap()).unwrap();
    let handoff_url = format!("{base}{}?{}", handoff.path(), handoff.query().unwrap());

    let browser = client
        .get(handoff_url)
        .header(header::HOST, "aiks.example.test")
        .send()
        .await
        .unwrap();
    assert!(browser.status().is_redirection());
    let cookie = browser
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let authorize = Url::parse(
        browser
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
    )
    .unwrap();
    let state = authorize
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();

    let callback = client
        .get(format!(
            "{base}/api/v1/auth/dingtalk/callback?state={state}&authCode={code}"
        ))
        .header(header::HOST, "aiks.example.test")
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(callback.status(), StatusCode::OK);

    let exchange = client
        .post(format!("{base}/api/v1/auth/dingtalk/exchange"))
        .header(header::HOST, "aiks.example.test")
        .json(&json!({"attempt_id":attempt,"verifier":verifier}))
        .send()
        .await
        .unwrap();
    assert_eq!(exchange.status(), StatusCode::OK);
    let tokens: Value = exchange.json().await.unwrap();
    (
        tokens["access_token"].as_str().unwrap().to_owned(),
        tokens["identity"]["user_id"].as_str().unwrap().to_owned(),
    )
}

#[tokio::test]
async fn team_server_enforces_perimeter_auth_acl_and_private_imports() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("team.db");
    let config = config(&database);
    let settings = validate_static(&config.team).unwrap();
    let server = TeamServer::build(&config, settings, Arc::new(SyntheticProvider)).unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let base = format!("http://{address}");
    let (stop, stopped) = oneshot::channel();
    let app = server.router();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = stopped.await;
        })
        .await
        .unwrap();
    });
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    wait_for_directory(&database).await;

    let health = client
        .get(format!("{base}/healthz"))
        .header(header::HOST, "aiks.example.test")
        .send()
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    let health_body: Value = health.json().await.unwrap();
    assert_eq!(health_body, json!({"status":"ok","mode":"team"}));

    let spoofed = client
        .get(format!("{base}/healthz"))
        .header(header::HOST, "evil.example.test")
        .send()
        .await
        .unwrap();
    assert_eq!(spoofed.status(), StatusCode::UNAUTHORIZED);
    for (header_name, value) in [
        (
            header::HeaderName::from_static("x-forwarded-host"),
            "aiks.example.test",
        ),
        (
            header::HeaderName::from_static("forwarded"),
            "host=aiks.example.test",
        ),
    ] {
        let response = client
            .get(format!("{base}/healthz"))
            .header(header::HOST, "aiks.example.test")
            .header(header_name, value)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let (owner_token, _owner_id) = login(&client, &base, "owner-code").await;
    let (reader_token, reader_id) = login(&client, &base, "reader-code").await;

    let imported = client
        .post(format!("{base}/api/v1/knowledge/import"))
        .header(header::HOST, "aiks.example.test")
        .bearer_auth(&owner_token)
        .json(&json!({
            "operation_id":"deployment-import-1",
            "title":"Deployment private",
            "markdown":"PRIVATE_TEAM_BODY",
            "source_fingerprint":"deployment-source-1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(imported.status(), StatusCode::CREATED);
    let imported: Value = imported.json().await.unwrap();
    let knowledge_id = imported["knowledge_id"].as_str().unwrap().to_owned();

    let reader_private = client
        .get(format!("{base}/api/v1/knowledge/{knowledge_id}"))
        .header(header::HOST, "aiks.example.test")
        .bearer_auth(&reader_token)
        .send()
        .await
        .unwrap();
    assert_eq!(reader_private.status(), StatusCode::NOT_FOUND);

    let shared = client
        .put(format!("{base}/api/v1/knowledge/{knowledge_id}/shares"))
        .header(header::HOST, "aiks.example.test")
        .bearer_auth(&owner_token)
        .json(&json!({
            "expected_grant_version":0,
            "grants":[{
                "target_type":"user",
                "target_id":reader_id,
                "include_descendants":false,
                "permission":"read"
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(shared.status(), StatusCode::OK);

    let reader_shared = client
        .get(format!("{base}/api/v1/knowledge/{knowledge_id}"))
        .header(header::HOST, "aiks.example.test")
        .bearer_auth(&reader_token)
        .send()
        .await
        .unwrap();
    assert_eq!(reader_shared.status(), StatusCode::OK);
    let body: Value = reader_shared.json().await.unwrap();
    assert_eq!(body["content"], "PRIVATE_TEAM_BODY");
    assert_eq!(body["can_manage"], false);

    let reader_edit = client
        .put(format!("{base}/api/v1/knowledge/{knowledge_id}/content"))
        .header(header::HOST, "aiks.example.test")
        .bearer_auth(&reader_token)
        .json(&json!({
            "base_revision":1,
            "operation_id":"reader-edit",
            "title":"No",
            "markdown":"NO"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(reader_edit.status(), StatusCode::FORBIDDEN);

    for path in ["/api/query/sql", "/proxy/anything", "/api/file/getFile"] {
        let response = client
            .get(format!("{base}{path}"))
            .header(header::HOST, "aiks.example.test")
            .bearer_auth(&owner_token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }

    stop.send(()).unwrap();
    task.await.unwrap();
    server.shutdown(Duration::from_secs(2)).await.unwrap();
}
