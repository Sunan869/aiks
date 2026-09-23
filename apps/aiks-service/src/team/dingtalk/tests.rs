use super::*;
use std::{sync::{Arc, atomic::{AtomicUsize, Ordering}}, time::Duration};
use axum::{extract::{Request, State}, http::{header, StatusCode}, response::{IntoResponse, Response}, Json, Router};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use crate::team::{config::{validate_static, TeamSettings}, secrets::{resolve_secret_with, SecretSource}};

#[derive(Clone, Copy, PartialEq)]
enum Scenario { Valid, WrongCorp, Inactive, WrongUnion, Permission, PaginationLoop, InvalidJson, MissingField, Oversize, Slow, RateLimited, Redirect }
struct FixtureState { scenario: Scenario, seen: Mutex<Vec<(String, Value)>>, app_calls: AtomicUsize, redirected: AtomicUsize }
struct Fixture { base: String, state: Arc<FixtureState>, task: tokio::task::JoinHandle<()> }
impl Drop for Fixture { fn drop(&mut self) { self.task.abort(); } }
impl Fixture {
    async fn start(scenario: Scenario) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(FixtureState {scenario, seen: Mutex::new(Vec::new()), app_calls: AtomicUsize::new(0), redirected: AtomicUsize::new(0)});
        let app = Router::new().fallback(handle).with_state(state.clone());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
        Self {base,state,task}
    }
    fn client(&self) -> DingTalkClient {
        let settings: TeamSettings = toml::from_str(r#"
            enabled = true
            public_base_url = "https://aiks.example.com"
            [dingtalk]
            enabled = true
            corp_id = "synthetic-corp"
            client_id = "synthetic-app"
            redirect_uri = "https://aiks.example.com/api/v1/auth/dingtalk/callback"
            [directory]
            root_department_ids = ["1"]
        "#).unwrap();
        let source = SecretSource::Environment("AIKS_TEST_SECRET".into());
        let secret = resolve_secret_with(&source, |_| Some("synthetic-server-secret".into())).unwrap();
        DingTalkClient::with_test_origin(validate_static(&settings).unwrap(), secret, &self.base, Duration::from_millis(200)).unwrap()
    }
}

async fn handle(State(state): State<Arc<FixtureState>>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    if path == "/forbidden-redirect" {
        state.redirected.fetch_add(1, Ordering::SeqCst);
        return Json(json!({"unexpected":"never follow"})).into_response();
    }
    let method = request.method().clone();
    let query = request.uri().query().unwrap_or_default().to_owned();
    let headers = request.headers().clone();
    let bytes = axum::body::to_bytes(request.into_body(), 8192).await.unwrap();
    let body: Value = if bytes.is_empty() {Value::Null} else {serde_json::from_slice(&bytes).unwrap()};
    state.seen.lock().await.push((path.clone(), body.clone()));
    if state.scenario == Scenario::Slow { tokio::time::sleep(Duration::from_secs(2)).await; }
    if state.scenario == Scenario::Redirect {
        return (StatusCode::TEMPORARY_REDIRECT, [(header::LOCATION,"/forbidden-redirect")]).into_response();
    }
    if state.scenario == Scenario::RateLimited {
        return (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER,"0")], "PRIVATE_RATE_ERROR").into_response();
    }
    if state.scenario == Scenario::Oversize { return "x".repeat(2*1024*1024+1).into_response(); }
    if state.scenario == Scenario::InvalidJson { return "PRIVATE_INVALID_JSON".into_response(); }
    if state.scenario == Scenario::MissingField { return Json(json!({"unexpected":"PRIVATE_MISSING"})).into_response(); }
    if path.starts_with("/topapi/") {
        assert_eq!(method, "POST");
        assert_eq!(query, "access_token=synthetic-app-token");
        assert!(headers.get("x-acs-dingtalk-access-token").is_none());
        if state.scenario == Scenario::Permission { return Json(json!({"errcode":50002,"errmsg":"PRIVATE_NO_PERMISSION"})).into_response(); }
    }
    let member = |id: &str| json!({"userid":id,"unionid":format!("union-{id}"),"name":format!("Employee {id}"),"active":true,"dept_id_list":[1,2]});
    let result = match path.as_str() {
        "/v1.0/oauth2/userAccessToken" => {
            assert_eq!(method,"POST"); assert!(query.is_empty());
            assert_eq!(body,json!({"clientId":"synthetic-app","clientSecret":"synthetic-server-secret","code":"synthetic-code","grantType":"authorization_code"}));
            json!({"accessToken":"synthetic-user-token","expireIn":7200,"corpId":if state.scenario==Scenario::WrongCorp {"wrong-corp"} else {"synthetic-corp"}})
        }
        "/v1.0/contact/users/me" => {
            assert_eq!(method,"GET"); assert!(query.is_empty());
            assert_eq!(headers["x-acs-dingtalk-access-token"],"synthetic-user-token");
            json!({"unionId":"union-a","nick":"Employee a"})
        }
        "/v1.0/oauth2/synthetic-corp/token" => {
            assert_eq!(method,"POST"); assert!(query.is_empty());
            assert_eq!(body,json!({"client_id":"synthetic-app","client_secret":"synthetic-server-secret","grant_type":"client_credentials"}));
            state.app_calls.fetch_add(1,Ordering::SeqCst);
            json!({"access_token":"synthetic-app-token","expires_in":7200})
        }
        "/topapi/user/getbyunionid" => {
            assert_eq!(body,json!({"unionid":"union-a"}));
            json!({"errcode":0,"result":{"userid":"a","contact_type":0}})
        }
        "/topapi/v2/user/get" => {
            assert_eq!(body["userid"],"a");
            let mut user=member("a");
            if state.scenario==Scenario::Inactive {user["active"]=json!(false);}
            if state.scenario==Scenario::WrongUnion {user["unionid"]=json!("another-union");}
            json!({"errcode":0,"result":user})
        }
        "/topapi/v2/department/get" => {
            assert_eq!(body["dept_id"],1);
            json!({"errcode":0,"result":{"dept_id":1,"parent_id":0,"name":"Company"}})
        }
        "/topapi/v2/department/listsub" => {
            let children=if body["dept_id"]==1 {json!([{"dept_id":2,"parent_id":1,"name":"Research"}])} else {json!([])};
            json!({"errcode":0,"result":children})
        }
        "/topapi/v2/user/list" => {
            assert_eq!(body["size"],100);
            let (users, more, next)=if body["dept_id"]==1 && body["cursor"]==0 {
                (json!([member("a")]),true,if state.scenario==Scenario::PaginationLoop {0} else {100})
            } else if body["dept_id"]==1 {(json!([member("b")]),false,0)}
            else {(json!([member("a")]),false,0)};
            json!({"errcode":0,"result":{"list":users,"has_more":more,"next_cursor":next}})
        }
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    Json(result).into_response()
}

#[tokio::test]
async fn real_wire_verifies_company_union_and_membership_without_credential_forwarding() {
    let f=Fixture::start(Scenario::Valid).await;
    let client=f.client();
    let user=client.exchange_code("synthetic-code").await.unwrap();
    assert_eq!(user.corp_id,"synthetic-corp"); assert_eq!(user.union_id,"union-a"); assert_eq!(user.external_user_id,"a");
    let paths:Vec<String>=f.state.seen.lock().await.iter().map(|(p,_)|p.clone()).collect();
    assert_eq!(paths,vec!["/v1.0/oauth2/userAccessToken","/v1.0/contact/users/me","/v1.0/oauth2/synthetic-corp/token","/topapi/user/getbyunionid","/topapi/v2/user/get"]);
}

#[tokio::test]
async fn foreign_company_disabled_member_and_identity_mismatch_are_rejected() {
    for scenario in [Scenario::WrongCorp,Scenario::Inactive,Scenario::WrongUnion,Scenario::Permission] {
        let f=Fixture::start(scenario).await;
        assert!(f.client().exchange_code("synthetic-code").await.is_err());
        if scenario==Scenario::WrongCorp {assert_eq!(f.state.seen.lock().await.len(),1);}
    }
}

#[tokio::test]
async fn complete_directory_paginates_and_keeps_multi_department_memberships() {
    let f=Fixture::start(Scenario::Valid).await; let client=f.client();
    let snapshot=client.directory(&["1".into()]).await.unwrap();
    assert!(snapshot.complete); assert_eq!(snapshot.scope,vec!["1"]);
    assert_eq!(snapshot.orgs.len(),2); assert_eq!(snapshot.users.len(),2); assert_eq!(snapshot.memberships.len(),3);
    assert_eq!(f.state.app_calls.load(Ordering::SeqCst),1);
    assert_eq!(f.state.seen.lock().await.iter().filter(|(p,_)|p=="/topapi/v2/user/list").count(),3);
    let root=tempfile::tempdir().unwrap();
    let store=aiks_core::team::TeamStore::bind(Arc::new(aiks_core::storage::StateDb::open_exclusive(&root.path().join("team.db")).unwrap()),"synthetic-corp","synthetic-app").unwrap();
    let time=snapshot.observed_at; store.publish_directory(snapshot,time).unwrap();
    let user=store.user_by_union("union-a").unwrap().unwrap();
    let child=store.org_id_by_external("2").unwrap().unwrap();
    assert!(store.is_org_member(&user.id,&child,false).unwrap());
}

#[tokio::test]
async fn repeated_page_cursor_cannot_be_labeled_a_complete_directory() {
    let f=Fixture::start(Scenario::PaginationLoop).await;
    assert!(f.client().directory(&["1".into()]).await.is_err());
    assert!(f.state.seen.lock().await.len()<10);
}

#[tokio::test]
async fn app_token_refresh_is_singleflight() {
    let f=Fixture::start(Scenario::Valid).await; let client=Arc::new(f.client());
    let (a,b)=tokio::join!(client.exchange_code("synthetic-code"),client.exchange_code("synthetic-code"));
    assert!(a.is_ok() && b.is_ok());
    assert_eq!(f.state.app_calls.load(Ordering::SeqCst),1);
}

#[tokio::test]
async fn upstream_failures_are_bounded_and_never_echo_private_response_text() {
    for scenario in [Scenario::InvalidJson,Scenario::MissingField,Scenario::Oversize,Scenario::Slow,Scenario::RateLimited,Scenario::Redirect] {
        let f=Fixture::start(scenario).await;
        let error=f.client().exchange_code("synthetic-code").await.err().unwrap();
        let printable=format!("{error:?} {error}");
        for private in ["PRIVATE_","synthetic-server-secret","synthetic-user-token",&f.base] {assert!(!printable.contains(private));}
        assert_eq!(f.state.redirected.load(Ordering::SeqCst),0);
        assert!(f.state.seen.lock().await.len()<=3);
    }
}

#[tokio::test]
async fn empty_or_invalid_input_never_calls_upstream() {
    let f=Fixture::start(Scenario::Valid).await; let client=f.client();
    for code in ["".to_owned(),"invalid\ncode".into(),"x".repeat(4097)] {assert!(client.exchange_code(&code).await.is_err());}
    for scope in [vec![],vec!["0".into()],vec!["../outside".into()],vec!["1".into(),"1".into()]] {assert!(client.directory(&scope).await.is_err());}
    assert!(f.state.seen.lock().await.is_empty());
}
