//! Real service binary + canonical client/provider/publisher, only loopback fixtures.
#[path = "../../aiks-desktop/src-tauri/src/service_client/mod.rs"]
pub mod service_client;
use service_client::{supervisor::OwnedService,CollectorOutbox,collector::{collect_provider,deliver_one,CollectionPolicy}};
use aiks_core::{ai::{config::AiModelConfig,ModelService},config::{ExternalProviderConfig,SiYuanConfig},
    indexing::{KnowledgeIndexInput,KnowledgeIndexService},knowledge::publisher::publish_knowledge_to_siyuan,
    model::SourceKind,pipeline::EmbeddingConfig,providers::native::NativeProvider,sink::SiYuanSink,storage::StateDb};
use axum::{extract::State,http::Uri,Json,Router};
use serde_json::{json,Value};
use std::{collections::HashMap,path::Path,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},time::Duration};
use tokio::sync::Semaphore;

struct Fixtures {
    documents:Mutex<HashMap<String,String>>,
    model_seen:AtomicBool,
    model_gate:Semaphore,
}
async fn fixture_request(State(state):State<Arc<Fixtures>>,uri:Uri,Json(input):Json<Value>)->Json<Value>{
    let path=uri.path();
    let response=match path {
        "/v1/chat/completions"=>{
            assert!(input.to_string().contains("OFFLINE_CANONICAL_SESSION"));
            state.model_seen.store(true,Ordering::Release);
            state.model_gate.acquire().await.unwrap().forget();
            let content=json!({"session_summary":"Offline fixture","knowledge_score":1.0,"worth_extracting":true,
                "items":[{"title":"OFFLINE_EXTRACTED_KNOWLEDGE","category":"implementation","summary":"Offline extracted fixture",
                "content":"# OFFLINE_EXTRACTED_KNOWLEDGE\nVerified through the actual model stage.","problem":null,"root_causes":null,
                "solutions":null,"key_commands":null,"key_files":null,"decisions":null,"tags":["offline"],"confidence":1.0}]}).to_string();
            json!({"choices":[{"message":{"content":content}}]})
        }
        "/v1/embeddings"=>{
            let count=input["input"].as_array().map_or(1,Vec::len);
            json!({"data":(0..count).map(|i|json!({"index":i,"embedding":[0.1,0.2,0.3]})).collect::<Vec<_>>()})
        }
        "/api/notebook/lsNotebooks"=>json!({"code":0,"data":{"notebooks":[{"id":"kn","name":"AI Knowledge","closed":false}]}}),
        "/api/query/sql"=>{
            let text=input["stmt"].as_str().unwrap();
            let id=text.split("id = '").nth(1).and_then(|s|s.split('\'').next());
            let found=id.is_some_and(|id|state.documents.lock().unwrap().contains_key(id));
            json!({"code":0,"data":if found{vec![json!({"box":"kn"})]}else{vec![]}})
        }
        "/api/filetree/createDocWithMd"=>{
            let mut docs=state.documents.lock().unwrap();
            let id=format!("fixture-doc-{}",docs.len());
            docs.insert(id.clone(),input["markdown"].as_str().unwrap().into());
            json!({"code":0,"data":id})
        }
        "/api/block/getBlockKramdown"=>{
            let id=input["id"].as_str().unwrap();
            let body=state.documents.lock().unwrap().get(id).cloned().expect("only mapped documents may be fetched");
            json!({"code":0,"data":{"id":id,"kramdown":body}})
        }
        "/api/attr/getBlockAttrs"=>json!({"code":0,"data":{}}),
        "/api/attr/setBlockAttrs"=>json!({"code":0,"data":null}),
        other=>panic!("unexpected fixture endpoint {other}"),
    };
    let mut response=response;
    if path.starts_with("/api/"){response["msg"]=json!("");}
    Json(response)
}

#[tokio::test]
async fn actual_binary_keeps_processing_without_source_and_reuses_canonical_publisher_after_restart(){
    if std::env::var_os("AIKS_TEST_ISOLATED_NETWORK").is_some(){
        let external=tokio::time::timeout(Duration::from_millis(500),tokio::net::TcpStream::connect("192.0.2.1:80")).await;
        assert!(matches!(external,Ok(Err(_))),"offline gate requires a namespace with no external route");
    }
    let fixture=Arc::new(Fixtures{documents:Mutex::new(HashMap::new()),model_seen:AtomicBool::new(false),model_gate:Semaphore::new(0)});
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin=format!("http://{}",listener.local_addr().unwrap());
    let app=Router::new().fallback(fixture_request).with_state(fixture.clone());
    let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap();});
    let root=tempfile::tempdir().unwrap();
    let database=root.path().join("state.db");
    let config=root.path().join("service.toml");
    std::fs::write(&config,format!("database={}\n[ai]\nenabled=true\nbase_url={:?}\nmodel='offline-fixture'\n[embedding]\nenabled=true\nbase_url={:?}\nmodel='offline-vector'\ndimensions=3\n[siyuan]\nbase_url={:?}\n",serde_json::to_string(&database).unwrap(),format!("{origin}/v1"),format!("{origin}/v1"),origin)).unwrap();
    let binary=Path::new(env!("CARGO_BIN_EXE_aiks-service"));
    let mut owner=OwnedService::start(binary,&config,None).await.unwrap();
    let client=owner.client();
    let identity=client.connection().instance_id().to_owned();
    let source=root.path().join("continue");
    std::fs::create_dir_all(source.join("sessions")).unwrap();
    std::fs::write(source.join("sessions/one.json"),json!({"sessionId":"offline-one","title":"Offline fixture","history":[{"message":{"role":"user","content":"OFFLINE_CANONICAL_SESSION: explain the persisted source."}}]}).to_string()).unwrap();
    let provider=NativeProvider::new(SourceKind::Continue,&ExternalProviderConfig{enabled:true,path:source.to_string_lossy().into(),paths:vec![]}).unwrap();
    let queue=Arc::new(CollectorOutbox::open(&root.path().join("collector.db")).unwrap());
    assert_eq!(collect_provider(&provider,&client,queue.clone(),&CollectionPolicy::default()).await.unwrap().queued,1);
    let receipt=deliver_one(&client,queue,0).await.unwrap().unwrap();
    std::fs::remove_dir_all(source).unwrap();
    drop(client);drop(provider);
    fixture.model_gate.add_permits(1);
    let observer=owner.client();
    tokio::time::timeout(Duration::from_secs(20),async{
        loop {
            let job=observer.get_job(&receipt.job_id).await.unwrap();
            if job["status"]=="DONE"{assert_eq!(job["pipeline_status"],"READY");break;}
            assert_ne!(job["status"],"FAILED");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.unwrap();
    assert!(fixture.model_seen.load(Ordering::Acquire));
    assert!(observer.session(&receipt.session_id).await.unwrap().to_string().contains("OFFLINE_CANONICAL_SESSION"));
    let items=observer.knowledge_list().await.unwrap();
    let id=items["items"][0]["id"].as_str().unwrap().to_owned();
    assert!(!owner.shutdown().await.unwrap());drop(owner);drop(observer);
    // Internal publisher integration, not a delivered HTTP write API.
    let db=Arc::new(StateDb::open_exclusive(&database).unwrap());
    let sink=SiYuanSink::new(SiYuanConfig{base_url:origin.clone(),..Default::default()}).unwrap();
    let published=publish_knowledge_to_siyuan(&db,&sink,&id,false).await.unwrap();
    let doc=published.target_id.unwrap();
    let markdown=sink.get_document_markdown(&doc).await.unwrap();
    assert!(markdown.contains("OFFLINE_EXTRACTED_KNOWLEDGE"));
    let models=Arc::new(ModelService::new(AiModelConfig{enabled:false,..Default::default()},EmbeddingConfig{enabled:true,base_url:format!("{origin}/v1"),model:"offline-vector".into(),dimensions:Some(3),..Default::default()}).unwrap());
    KnowledgeIndexService::new(db.clone(),models).index_document(KnowledgeIndexInput{knowledge_id:id.clone(),siyuan_doc_id:doc,markdown}).await.unwrap();
    drop(db);
    let mut restarted=OwnedService::start(binary,&config,Some(&identity)).await.unwrap();
    let client=restarted.client();
    let result=client.search("OFFLINE_EXTRACTED_KNOWLEDGE").await.unwrap();
    assert!(result["hits"].as_array().unwrap().iter().any(|hit|hit["entity_id"]==id&&hit["corpus"]=="knowledge"));
    let body=client.knowledge(&id).await.unwrap();
    assert_eq!(body["content_state"],"published");
    assert!(body["content"].as_str().unwrap().contains("OFFLINE_EXTRACTED_KNOWLEDGE"));
    assert!(!restarted.shutdown().await.unwrap());server.abort();
}
