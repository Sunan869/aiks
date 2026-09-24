mod team_support;

use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    Router,
};
use reqwest::StatusCode as ReqwestStatus;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use team_support::TeamTestService;
use tokio::sync::Semaphore;

struct SiYuanState {
    documents: Mutex<HashMap<String, String>>,
    hpaths: Mutex<HashMap<String, String>>,
    update_seen: Semaphore,
    update_release: Semaphore,
    create_seen: Semaphore,
    create_release: Semaphore,
    fail_create_response_once: Mutex<bool>,
}

impl Default for SiYuanState {
    fn default() -> Self {
        Self {
            documents: Mutex::new(HashMap::new()),
            hpaths: Mutex::new(HashMap::new()),
            update_seen: Semaphore::new(0),
            update_release: Semaphore::new(0),
            create_seen: Semaphore::new(0),
            create_release: Semaphore::new(0),
            fail_create_response_once: Mutex::new(false),
        }
    }
}

async fn siyuan_fixture(State(state): State<Arc<SiYuanState>>, request: Request<Body>) -> Response {
    let path = request.uri().path().to_owned();
    let bytes = axum::body::to_bytes(request.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let input: Value = if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    match path.as_str() {
        "/api/notebook/lsNotebooks" => JsonResponse(json!({
            "code":0,"msg":"","data":{"notebooks":[{"id":"kn","name":"AI Knowledge","closed":false}]}
        }))
        .into_response(),
        "/api/query/sql" => {
            let stmt = input["stmt"].as_str().unwrap_or_default();
            let hpath = stmt
                .split("hpath = '")
                .nth(1)
                .and_then(|rest| rest.split('\'').next());
            let data = hpath
                .and_then(|path| state.hpaths.lock().unwrap().get(path).cloned())
                .map(|id| vec![json!({"id":id})])
                .unwrap_or_default();
            JsonResponse(json!({"code":0,"msg":"","data":data})).into_response()
        }
        "/api/filetree/createDocWithMd" => {
            let id = "published-doc".to_string();
            let markdown = input["markdown"].as_str().unwrap().to_owned();
            let hpath = input["path"].as_str().unwrap().to_owned();
            state.documents.lock().unwrap().insert(id.clone(), markdown);
            state.hpaths.lock().unwrap().insert(hpath, id.clone());
            let fail = {
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
        }
        "/api/block/getBlockKramdown" => {
            let id = input["id"].as_str().unwrap();
            let body = state
                .documents
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .unwrap_or_default();
            JsonResponse(json!({"code":0,"msg":"","data":{"id":id,"kramdown":body}}))
                .into_response()
        }
        "/api/block/updateBlock" => {
            state.update_seen.add_permits(1);
            state.update_release.acquire().await.unwrap().forget();
            let id = input["id"].as_str().unwrap().to_owned();
            let body = input["data"].as_str().unwrap().to_owned();
            state.documents.lock().unwrap().insert(id.clone(), body);
            JsonResponse(json!({"code":0,"msg":"","data":[{"doOperations":[{"id":id}]}]}))
                .into_response()
        }
        "/api/attr/setBlockAttrs" => {
            JsonResponse(json!({"code":0,"msg":"","data":null})).into_response()
        }
        other => panic!("unexpected SiYuan fixture endpoint: {other}"),
    }
}

struct JsonResponse(Value);
impl IntoResponse for JsonResponse {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

async fn start_siyuan(state: Arc<SiYuanState>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().fallback(siyuan_fixture).with_state(state),
        )
        .await
        .unwrap();
    });
    (origin, task)
}

fn hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn seed_owned_knowledge(
    service: &TeamTestService,
    id: &str,
    session_id: i64,
    content: &str,
    doc_id: Option<&str>,
    remote_hash: Option<&str>,
) {
    let db = Connection::open(&service.path).unwrap();
    db.execute(
        "INSERT INTO knowledge_item(id,source_session_id,title,category,summary,content,tags,created_at,updated_at,siyuan_doc_id,current_remote_hash)
         VALUES (?1,?2,'Original title','implementation','Summary',?3,'[]','test','test',?4,?5)",
        params![id, session_id, content, doc_id, remote_hash],
    )
    .unwrap();
    db.execute(
        "INSERT INTO service_knowledge_revision(knowledge_id,session_id,revision) VALUES (?1,?2,1)",
        params![id, session_id],
    )
    .unwrap();
    db.execute(
        "INSERT INTO team_knowledge_owner(company_id,knowledge_id,owner_user_id,content_revision) VALUES (?1,?2,?3,1)",
        params![service.store.company_id(), id, service.owner_id],
    )
    .unwrap();
}

async fn operation(service: &TeamTestService, token: &str, id: &str) -> Value {
    let response = service
        .auth(
            token,
            service
                .client
                .get(format!("{}/api/v1/content-operations/{id}", service.base)),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), ReqwestStatus::OK);
    response.json().await.unwrap()
}

async fn wait_state(service: &TeamTestService, token: &str, id: &str, expected: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let value = operation(service, token, id).await;
            if value["state"] == expected {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn owner_content_operations_are_versioned_recoverable_and_assets_follow_document_acl() {
    let siyuan = Arc::new(SiYuanState::default());
    siyuan
        .documents
        .lock()
        .unwrap()
        .insert("mapped-doc".into(), "BASE_REMOTE".into());
    let (origin, server) = start_siyuan(siyuan.clone()).await;
    let service = TeamTestService::configured(|config| {
        config.siyuan.base_url = origin.clone();
    })
    .await;
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
    seed_owned_knowledge(
        &service,
        "editable",
        session_id,
        "SQLITE_OLD",
        Some("mapped-doc"),
        Some(&hash("BASE_REMOTE")),
    );

    let reader_write = service
        .auth(
            &service.reader_token,
            service.client.put(format!(
                "{}/api/v1/knowledge/editable/content",
                service.base
            )),
        )
        .json(&json!({"base_revision":1,"operation_id":"reader-op","title":"No","markdown":"NO"}))
        .send()
        .await
        .unwrap();
    assert_eq!(reader_write.status(), ReqwestStatus::FORBIDDEN);

    let first = service
        .auth(
            &service.owner_token,
            service
                .client
                .put(format!("{}/api/v1/knowledge/editable/content", service.base)),
        )
        .json(&json!({"base_revision":1,"operation_id":"op-one","title":"Updated title","markdown":"NEW_REMOTE"}))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), ReqwestStatus::ACCEPTED);

    tokio::time::timeout(Duration::from_secs(3), siyuan.update_seen.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();

    let pending_read = service
        .auth(
            &service.owner_token,
            service
                .client
                .get(format!("{}/api/v1/knowledge/editable", service.base)),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(pending_read.status(), ReqwestStatus::CONFLICT);
    assert_eq!(
        pending_read.json::<Value>().await.unwrap()["error"]["code"],
        "content_pending"
    );

    let concurrent = service
        .auth(
            &service.owner_token,
            service.client.put(format!(
                "{}/api/v1/knowledge/editable/content",
                service.base
            )),
        )
        .json(
            &json!({"base_revision":1,"operation_id":"op-two","title":"Other","markdown":"OTHER"}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(concurrent.status(), ReqwestStatus::CONFLICT);

    siyuan.update_release.add_permits(1);
    let done = wait_state(&service, &service.owner_token, "op-one", "done").await;
    assert_eq!(done["result_revision"], 2);

    let current: Value = service
        .auth(
            &service.owner_token,
            service
                .client
                .get(format!("{}/api/v1/knowledge/editable", service.base)),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(current["title"], "Updated title");
    assert_eq!(current["content"], "NEW_REMOTE");
    assert_eq!(current["content_revision"], 2);
    assert!(!current.to_string().contains("SQLITE_OLD"));

    let replay = service
        .auth(
            &service.owner_token,
            service
                .client
                .put(format!("{}/api/v1/knowledge/editable/content", service.base)),
        )
        .json(&json!({"base_revision":1,"operation_id":"op-one","title":"Updated title","markdown":"NEW_REMOTE"}))
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), ReqwestStatus::ACCEPTED);
    assert_eq!(replay.json::<Value>().await.unwrap()["state"], "done");

    let redefined = service
        .auth(
            &service.owner_token,
            service
                .client
                .put(format!("{}/api/v1/knowledge/editable/content", service.base)),
        )
        .json(&json!({"base_revision":1,"operation_id":"op-one","title":"Updated title","markdown":"DIFFERENT"}))
        .send()
        .await
        .unwrap();
    assert_eq!(redefined.status(), ReqwestStatus::CONFLICT);

    siyuan
        .documents
        .lock()
        .unwrap()
        .insert("mapped-doc".into(), "HUMAN_EDIT".into());
    let human_conflict = service
        .auth(
            &service.owner_token,
            service
                .client
                .put(format!("{}/api/v1/knowledge/editable/content", service.base)),
        )
        .json(&json!({"base_revision":2,"operation_id":"op-human","title":"Do not overwrite","markdown":"WOULD_OVERWRITE"}))
        .send()
        .await
        .unwrap();
    assert_eq!(human_conflict.status(), ReqwestStatus::ACCEPTED);
    wait_state(&service, &service.owner_token, "op-human", "conflict").await;
    assert_eq!(
        siyuan.documents.lock().unwrap().get("mapped-doc").unwrap(),
        "HUMAN_EDIT"
    );

    let share = service
        .auth(
            &service.owner_token,
            service
                .client
                .put(format!("{}/api/v1/knowledge/editable/shares", service.base)),
        )
        .json(&json!({"expected_grant_version":0,"grants":[{"target_type":"user","target_id":service.reader_id,"permission":"read"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(share.status(), ReqwestStatus::OK);

    let upload = service
        .auth(
            &service.owner_token,
            service.client.post(format!(
                "{}/api/v1/knowledge/editable/assets?filename=note.txt",
                service.base
            )),
        )
        .header(header::CONTENT_TYPE.as_str(), "text/plain; charset=utf-8")
        .body("ASSET_BYTES")
        .send()
        .await
        .unwrap();
    assert_eq!(upload.status(), ReqwestStatus::CREATED);
    let asset_id = upload.json::<Value>().await.unwrap()["asset_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let asset = service
        .auth(
            &service.reader_token,
            service.client.get(format!(
                "{}/api/v1/knowledge/editable/assets/{asset_id}",
                service.base
            )),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(asset.status(), ReqwestStatus::OK);
    assert_eq!(asset.headers()[header::CONTENT_DISPOSITION], "attachment");
    assert_eq!(asset.headers()["x-content-type-options"], "nosniff");
    assert_eq!(asset.text().await.unwrap(), "ASSET_BYTES");

    seed_owned_knowledge(&service, "private", session_id, "PRIVATE", None, None);
    let private_upload = service
        .auth(
            &service.owner_token,
            service.client.post(format!(
                "{}/api/v1/knowledge/private/assets?filename=private.txt",
                service.base
            )),
        )
        .header(header::CONTENT_TYPE.as_str(), "text/plain")
        .body("PRIVATE_ASSET")
        .send()
        .await
        .unwrap();
    let private_asset = private_upload.json::<Value>().await.unwrap()["asset_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        service
            .auth(
                &service.reader_token,
                service.client.get(format!(
                    "{}/api/v1/knowledge/private/assets/{private_asset}",
                    service.base
                )),
            )
            .send()
            .await
            .unwrap()
            .status(),
        ReqwestStatus::NOT_FOUND
    );

    let svg = service
        .auth(
            &service.owner_token,
            service.client.post(format!(
                "{}/api/v1/knowledge/editable/assets?filename=x.svg",
                service.base
            )),
        )
        .header(header::CONTENT_TYPE.as_str(), "image/svg+xml")
        .body("<svg></svg>")
        .send()
        .await
        .unwrap();
    assert_eq!(svg.status(), ReqwestStatus::BAD_REQUEST);

    service.stop().await;
    server.abort();
}

#[tokio::test]
async fn publish_reconciles_a_committed_document_after_the_create_response_is_lost() {
    let siyuan = Arc::new(SiYuanState::default());
    *siyuan.fail_create_response_once.lock().unwrap() = true;
    let (origin, server) = start_siyuan(siyuan.clone()).await;
    let service = TeamTestService::configured(|config| {
        config.siyuan.base_url = origin.clone();
    })
    .await;
    let receipt = service
        .ingest(&service.owner_token, &service.owner_space, "PUBLISH_SOURCE")
        .await;
    service
        .wait_job(&service.owner_token, receipt["job_id"].as_str().unwrap())
        .await;
    let session_id: i64 = receipt["session_id"].as_str().unwrap().parse().unwrap();
    seed_owned_knowledge(
        &service,
        "draft-publish",
        session_id,
        "# PUBLISH_ME",
        None,
        None,
    );

    let response = service
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
    assert_eq!(response.status(), ReqwestStatus::ACCEPTED);
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
    assert_eq!(done["result_revision"], 2);

    let knowledge: Value = service
        .auth(
            &service.owner_token,
            service
                .client
                .get(format!("{}/api/v1/knowledge/draft-publish", service.base)),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(knowledge["content"], "# PUBLISH_ME");
    assert_eq!(knowledge["content_state"], "published");
    assert_eq!(knowledge["content_revision"], 2);
    assert_eq!(siyuan.documents.lock().unwrap().len(), 1);
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
    server.abort();
}
