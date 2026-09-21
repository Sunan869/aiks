use aiks_core::{
    ai::ModelService,
    config::Config,
    indexing::{KnowledgeIndexInput, KnowledgeIndexService},
    knowledge::publisher::publish_knowledge_to_siyuan,
    model::{ContentBlock, NormalizedSession},
    pipeline::{PipelineJob, PipelineOrchestrator, PipelineWorker},
    providers::{build_registry, ProviderRegistry},
    search::{SearchCorpus, UnifiedSearchFilter, UnifiedSearchService},
    sink::SiYuanSink,
    storage::{SourceSessionRepo, StateDb},
    sync::engine::{SyncEngine, SyncOptions},
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Services {
    url: String,
    task: tokio::task::JoinHandle<()>,
    creates: Arc<AtomicUsize>,
    actual_prompt_seen: Arc<AtomicBool>,
}
impl Drop for Services {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn services(needle: String, knowledge_needle: String) -> Services {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let documents = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    let creates = Arc::new(AtomicUsize::new(0));
    let actual_prompt_seen = Arc::new(AtomicBool::new(false));
    let counter = creates.clone();
    let prompt_seen = actual_prompt_seen.clone();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let mut request = Vec::new();
            let header_end = loop {
                let mut buf = [0_u8; 8192];
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0, "mock service received incomplete HTTP request");
                request.extend_from_slice(&buf[..n]);
                assert!(
                    request.len() < 4 * 1024 * 1024,
                    "fixture request exceeded test budget"
                );
                if let Some(end) = request.windows(4).position(|v| v == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let size = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + size {
                        break end;
                    }
                }
            };
            let header = String::from_utf8_lossy(&request[..header_end]);
            let path = header.split_whitespace().nth(1).unwrap_or("");
            let payload: Value =
                serde_json::from_slice(&request[header_end + 4..]).unwrap_or(Value::Null);
            let mut response = if path.ends_with("/chat/completions") {
                if payload.to_string().contains(&needle) {
                    prompt_seen.store(true, Ordering::SeqCst);
                }
                let content = json!({"session_summary":"fixture summary","knowledge_score":0.99,"worth_extracting":true,"items":[{"title":knowledge_needle,"category":"implementation","summary":knowledge_needle,"content":format!("# {knowledge_needle}\n\nVerified fixture knowledge from the actual provider messages."),"problem":null,"root_causes":null,"solutions":null,"key_commands":null,"key_files":null,"decisions":null,"tags":["provider-fixture"],"confidence":0.99}]}).to_string();
                json!({"choices":[{"message":{"content":content}}]})
            } else if path.ends_with("/embeddings") {
                let n = payload["input"].as_array().map_or(1, Vec::len);
                json!({"data":(0..n).map(|i|json!({"index":i,"embedding":[0.1,0.2,0.3]})).collect::<Vec<_>>()})
            } else if path.ends_with("/lsNotebooks") {
                json!({"code":0,"data":{"notebooks":[{"id":"kn","name":"AI Knowledge","closed":false},{"id":"sn","name":"AI Session Archive","closed":false}]}})
            } else if path.ends_with("/createDocWithMd") {
                let id = format!("fixture-doc-{}", counter.fetch_add(1, Ordering::SeqCst));
                documents
                    .lock()
                    .unwrap()
                    .insert(id.clone(), payload["markdown"].as_str().unwrap().to_owned());
                json!({"code":0,"data":id})
            } else if path.ends_with("/getBlockKramdown") {
                let id = payload["id"].as_str().unwrap();
                let content = documents
                    .lock()
                    .unwrap()
                    .get(id)
                    .cloned()
                    .unwrap_or_default();
                json!({"code":0,"data":{"id":id,"kramdown":content}})
            } else if path.ends_with("/updateBlock") {
                let id = payload["id"].as_str().unwrap().to_owned();
                documents.lock().unwrap().insert(
                    id.clone(),
                    payload["data"].as_str().unwrap_or_default().to_owned(),
                );
                json!({"code":0,"data":[{"doOperations":[{"id":id}]}]})
            } else if path.ends_with("/getBlockAttrs") {
                json!({"code":0,"data":{}})
            } else if path.ends_with("/query/sql") {
                let stmt = payload["stmt"].as_str().unwrap();
                // Mirror the sink's existence query; an absent document must
                // stay absent, rather than returning a blanket successful row.
                let id = stmt
                    .split("id = '")
                    .nth(1)
                    .and_then(|s| s.split('\'').next());
                let found = id.is_some_and(|id| documents.lock().unwrap().contains_key(id));
                json!({"code":0,"data":if found { vec![json!({"box":"kn"})] } else { Vec::<Value>::new() }})
            } else if path.ends_with("/setBlockAttrs") {
                json!({"code":0,"data":null})
            } else {
                panic!("unexpected mock endpoint: {path}");
            };
            if path.starts_with("/api/") {
                response["msg"] = json!("");
            }
            let body = response.to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
        }
    });
    Services {
        url,
        task,
        creates,
        actual_prompt_seen,
    }
}

pub async fn verify(config: &Config, session: &NormalizedSession) {
    let source = session.source.as_str();
    let needle = session
        .messages
        .iter()
        .flat_map(|m| &m.blocks)
        .find_map(|b| match b {
            ContentBlock::Text { text } => text.split_whitespace().next().map(str::to_owned),
            _ => None,
        })
        .unwrap();
    let knowledge_needle = format!("PROVIDERFLOW{}", source.replace('_', ""));
    let services = services(needle.clone(), knowledge_needle.clone()).await;
    let data = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&data.path().join("state.db")).unwrap());
    let mut config = config.clone();
    config.archive.enabled = false;
    config.siyuan.base_url = services.url.clone();
    config.ai.enabled = true;
    config.ai.base_url = format!("{}/v1", services.url);
    config.ai.model = "provider-fixture-ai".into();
    config.embedding.enabled = true;
    config.embedding.base_url = format!("{}/v1", services.url);
    config.embedding.model = "provider-fixture-vector".into();
    config.embedding.dimensions = Some(3);
    let registry: Arc<ProviderRegistry> = Arc::new(build_registry(&config));
    let sink = SiYuanSink::new(config.siyuan.clone()).unwrap();
    let sync = SyncEngine::new(Arc::new(config.clone()));
    let options = SyncOptions {
        source_filter: Some(source.into()),
        ..Default::default()
    };
    let first = sync
        .run_sync(&db, &registry, &sink, &options)
        .await
        .unwrap();
    assert_eq!(first.failed_count, 0, "{source} failed actual sync");
    assert_eq!(
        first.new_count, 1,
        "{source} was not imported through canonical sync"
    );
    let row = SourceSessionRepo::new(&db)
        .find_by_source_and_id(source, &session.external_session_id)
        .unwrap()
        .unwrap();
    let second = sync
        .run_sync(&db, &registry, &sink, &options)
        .await
        .unwrap();
    assert_eq!(second.new_count, 0);
    assert_eq!(second.unchanged_count, 1);
    assert_eq!(
        services.creates.load(Ordering::SeqCst),
        1,
        "repeat sync created a duplicate remote archive"
    );
    assert_eq!(
        SourceSessionRepo::new(&db)
            .find_by_source_and_id(source, &session.external_session_id)
            .unwrap()
            .unwrap()
            .id,
        row.id
    );
    let worker = PipelineWorker::start_with_limit(
        db.clone(),
        registry.clone(),
        config.ai.clone(),
        config.embedding.clone(),
        1,
    );
    let run_id = PipelineOrchestrator::new(db.clone())
        .enqueue(row.id, row.content_hash.as_deref())
        .unwrap();
    worker
        .submit(PipelineJob {
            pipeline_run_id: run_id.clone(),
            session_id: row.id,
            session_external_id: session.external_session_id.clone(),
            source: source.into(),
            session_title: session.title.clone(),
            project_name: session.project_name.clone(),
        })
        .unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let (status, error): (String, Option<String>) = db
                .conn()
                .query_row(
                    "SELECT status,error_stage FROM pipeline_run WHERE id=?1",
                    [&run_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            if status == "READY" {
                break;
            }
            assert!(
                !matches!(status.as_str(), "FAILED" | "RAW_ONLY"),
                "{source} did not complete actual pipeline: {status}, {error:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("provider pipeline timed out");
    assert!(
        services.actual_prompt_seen.load(Ordering::SeqCst),
        "AI stage did not receive the actual source prompt"
    );
    let knowledge_id: String = db
        .conn()
        .query_row(
            "SELECT id FROM knowledge_item WHERE source_session_id=?1 AND status='active'",
            [row.id],
            |r| r.get(0),
        )
        .unwrap();
    let published = publish_knowledge_to_siyuan(&db, &sink, &knowledge_id, false)
        .await
        .unwrap();
    let doc_id = published.target_id.unwrap();
    let markdown = sink.get_document_markdown(&doc_id).await.unwrap();
    assert!(markdown.contains(&knowledge_needle));
    let models = Arc::new(ModelService::new(config.ai.clone(), config.embedding.clone()).unwrap());
    let indexed = KnowledgeIndexService::new(db.clone(), models.clone())
        .index_document(KnowledgeIndexInput {
            knowledge_id: knowledge_id.clone(),
            siyuan_doc_id: doc_id,
            markdown,
        })
        .await
        .unwrap();
    assert!(
        indexed.embedded_count > 0,
        "{source} knowledge was not embedded by the canonical indexer"
    );
    let search = UnifiedSearchService::new(db.clone(), models);
    let knowledge = search
        .search(
            &knowledge_needle,
            10,
            UnifiedSearchFilter {
                corpora: vec![SearchCorpus::Knowledge],
                source: Some(source.into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(
        knowledge.hits.iter().any(|h| h.entity_id == knowledge_id),
        "{source} extracted knowledge is not searchable"
    );
    let raw = search
        .search(
            &needle,
            10,
            UnifiedSearchFilter {
                corpora: vec![SearchCorpus::Session],
                source: Some(source.into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(
        raw.hits.iter().any(|h| h.entity_id == row.id.to_string()),
        "{source} actual session is not searchable"
    );
    assert!(knowledge
        .hits
        .iter()
        .any(|h| h.match_types.iter().any(|m| m == "semantic")));
    drop(worker);
}
