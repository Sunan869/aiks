use std::{collections::VecDeque, sync::{Arc,atomic::{AtomicUsize,Ordering}}, time::{Duration,SystemTime,UNIX_EPOCH}};
use aiks_core::{storage::StateDb,team::{provider::ProviderFuture,DirectorySnapshot,DirectoryUser,ExternalLogin,IdentityProvider,Membership,OrgRecord,TeamError,TeamStore}};
use aiks_service::team::directory_worker::{DirectoryPolicy,DirectoryWorker};
use tokio::sync::Mutex;

fn snapshot(complete:bool)->DirectorySnapshot {
    DirectorySnapshot {complete,scope:vec!["1".into()],orgs:vec![OrgRecord{id:"1".into(),parent_id:None,name:"Synthetic company".into()}],
        users:vec![DirectoryUser{external_user_id:"employee".into(),union_id:"union-employee".into(),display_name:"Employee".into(),active:true}],
        memberships:vec![Membership{user_id:"employee".into(),org_id:"1".into()}],
        observed_at:SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()}
}
struct Provider {replies:Mutex<VecDeque<Result<DirectorySnapshot,TeamError>>>,active:AtomicUsize,calls:AtomicUsize}
struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_>{fn drop(&mut self){self.0.fetch_sub(1,Ordering::SeqCst);}}
impl IdentityProvider for Provider {
    fn exchange_code<'a>(&'a self,_:&'a str)->ProviderFuture<'a,ExternalLogin>{Box::pin(async{Err(TeamError::Unauthorized)})}
    fn directory<'a>(&'a self,scope:&'a[String])->ProviderFuture<'a,DirectorySnapshot>{Box::pin(async move {
        assert_eq!(scope, &["1"]);
        self.calls.fetch_add(1,Ordering::SeqCst);self.active.fetch_add(1,Ordering::SeqCst);let _active=Active(&self.active);
        let reply=self.replies.lock().await.pop_front();
        match reply {Some(reply)=>reply,None=>std::future::pending().await}
    })}
}
fn fixture()->(tempfile::TempDir,Arc<TeamStore>,Arc<Provider>){
    let root=tempfile::tempdir().unwrap();let store=Arc::new(TeamStore::bind(Arc::new(StateDb::open_exclusive(&root.path().join("company.db")).unwrap()),"synthetic-corp","synthetic-app").unwrap());
    let provider=Arc::new(Provider{replies:Mutex::new(VecDeque::new()),active:AtomicUsize::new(0),calls:AtomicUsize::new(0)});
    (root,store,provider)
}
fn policy()->DirectoryPolicy {DirectoryPolicy{scope:vec!["1".into()],refresh_seconds:300,max_stale_seconds:900}}
async fn settled(worker:&DirectoryWorker,attempt:u64){
    tokio::time::timeout(Duration::from_secs(5),async{
        loop {let state=worker.status();if state.attempt>=attempt && !state.running{break;}tokio::time::sleep(Duration::from_millis(10)).await;}
    }).await.unwrap();
}

#[tokio::test]
async fn failed_and_partial_refresh_preserve_the_last_complete_directory(){
    let (_root,store,provider)=fixture();
    provider.replies.lock().await.extend([Ok(snapshot(true)),Err(TeamError::Unavailable),Ok(snapshot(false)),Ok(snapshot(true))]);
    let worker=DirectoryWorker::start(store.clone(),provider.clone(),policy()).unwrap();
    settled(&worker,1).await;assert_eq!(store.directory_generation().unwrap(),1);assert!(worker.status().error_code.is_none());
    let id=store.user_by_union("union-employee").unwrap().unwrap().id;
    for attempt in [2,3] {
        worker.request_refresh();settled(&worker,attempt).await;
        assert_eq!(store.directory_generation().unwrap(),1);assert_eq!(worker.status().error_code,Some("directory_refresh_failed"));
        assert!(store.user_by_union("union-employee").unwrap().unwrap().active);
    }
    worker.request_refresh();settled(&worker,4).await;
    assert_eq!(store.directory_generation().unwrap(),2);assert_eq!(store.user_by_union("union-employee").unwrap().unwrap().id,id);
    assert!(worker.status().error_code.is_none());
    let failed:i64=store.db().conn().query_row("SELECT COUNT(*) FROM team_audit_event WHERE action='directory_refresh_failed'",[],|r|r.get(0)).unwrap();
    assert_eq!(failed,2);
    worker.shutdown(Duration::from_secs(2)).await.unwrap();
    assert_eq!(provider.active.load(Ordering::SeqCst),0);
}

#[tokio::test]
async fn refresh_is_serial_coalesced_and_shutdown_cancels_an_inflight_upstream_call(){
    let (_root,store,provider)=fixture();
    let worker=DirectoryWorker::start(store.clone(),provider.clone(),policy()).unwrap();
    tokio::time::timeout(Duration::from_secs(2),async{
        while provider.active.load(Ordering::SeqCst)==0 {tokio::task::yield_now().await;}
    }).await.unwrap();
    for _ in 0..100 {worker.request_refresh();}
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(provider.calls.load(Ordering::SeqCst),1);
    worker.shutdown(Duration::from_secs(2)).await.unwrap();
    assert_eq!(provider.active.load(Ordering::SeqCst),0);assert_eq!(store.directory_generation().unwrap(),0);
    worker.shutdown(Duration::from_secs(2)).await.unwrap();
}

#[tokio::test]
async fn invalid_policy_or_mismatched_scope_cannot_publish_another_scope(){
    let (_root,store,provider)=fixture();
    let mut wrong=policy();wrong.refresh_seconds=0;
    assert!(DirectoryWorker::start(store.clone(),provider.clone(),wrong).is_err());
    assert_eq!(provider.calls.load(Ordering::SeqCst),0);
    let mut mismatched=snapshot(true);mismatched.scope=vec!["2".into()];
    provider.replies.lock().await.push_back(Ok(mismatched));
    let worker=DirectoryWorker::start(store.clone(),provider,policy()).unwrap();
    settled(&worker,1).await;assert_eq!(store.directory_generation().unwrap(),0);
    assert_eq!(worker.status().error_code,Some("directory_refresh_failed"));
    worker.shutdown(Duration::from_secs(2)).await.unwrap();
}
