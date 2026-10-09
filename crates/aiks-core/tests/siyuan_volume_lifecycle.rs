//! End-to-end volume lifecycle against an in-memory SiYuan HTTP test double.
//! No external SiYuan installation or model server is required.
use aiks_core::{
    sink::{SessionVolumeRequest, SiYuanSink},
    storage::StateDb,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Default)]
struct Remote {
    next_id: usize,
    creates: usize,
    documents: HashMap<String, (String, String)>, // id => (hpath, markdown)
}

async fn serve_siyuan() -> (String, Arc<Mutex<Remote>>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let state = Arc::new(Mutex::new(Remote::default()));
    let shared = Arc::clone(&state);
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let shared = Arc::clone(&shared);
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                let mut header_end = None;
                let mut content_length = 0;
                loop {
                    let mut buf = [0u8; 16384];
                    let Ok(n) = socket.read(&mut buf).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if header_end.is_none() {
                        if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                            let header = String::from_utf8_lossy(&bytes[..at]);
                            content_length = header
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .and_then(|value| value.trim().parse::<usize>().ok())
                                })
                                .unwrap_or(0);
                            header_end = Some(at + 4);
                        }
                    }
                    if let Some(offset) = header_end {
                        if bytes.len() >= offset + content_length {
                            break;
                        }
                    }
                }
                let request = String::from_utf8_lossy(&bytes);
                let endpoint = request.split_whitespace().nth(1).unwrap_or("").to_string();
                let payload: Value = serde_json::from_slice(&bytes[header_end.unwrap()..]).unwrap();
                let reply = {
                    let mut state = shared.lock().unwrap();
                    let mut code = 0;
                    let mut message = "";
                    let data = match endpoint.as_str() {
                        "/api/query/sql" => {
                            let sql = payload["stmt"].as_str().unwrap_or("");
                            if sql.contains("hpath = '") {
                                let path = sql.split('\'').nth(3).unwrap_or("");
                                json!(state
                                    .documents
                                    .iter()
                                    .filter(|(_, (p, _))| p == path)
                                    .map(|(id, _)| json!({"id": id}))
                                    .collect::<Vec<_>>())
                            } else if sql.contains("WHERE id = '") {
                                let id = sql.split('\'').nth(1).unwrap_or("");
                                if state.documents.contains_key(id) {
                                    json!([{"box":"box-1"}])
                                } else {
                                    json!([])
                                }
                            } else {
                                json!([])
                            }
                        }
                        "/api/filetree/createDocWithMd" => {
                            state.next_id += 1;
                            state.creates += 1;
                            let id = format!("20261009-test-{:06}", state.next_id);
                            state.documents.insert(
                                id.clone(),
                                (
                                    payload["path"].as_str().unwrap().to_owned(),
                                    payload["markdown"].as_str().unwrap().to_owned(),
                                ),
                            );
                            json!(id)
                        }
                        "/api/block/getBlockKramdown" => {
                            let id = payload["id"].as_str().unwrap();
                            match state.documents.get(id) {
                                Some((_, md)) => json!({"kramdown": md}),
                                None => {
                                    code = -1;
                                    message = "block not found";
                                    Value::Null
                                }
                            }
                        }
                        "/api/block/updateBlock" => {
                            let id = payload["id"].as_str().unwrap();
                            if let Some((_, md)) = state.documents.get_mut(id) {
                                *md = payload["data"].as_str().unwrap().to_owned();
                            }
                            Value::Null
                        }
                        "/api/attr/setBlockAttrs" => Value::Null,
                        _ => Value::Null,
                    };
                    json!({"code":code, "msg":message, "data":data}).to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.len(), reply
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    (base, state, task)
}

#[tokio::test]
async fn volume_create_retry_conflict_and_confirmed_delete_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let db = StateDb::open(&temp.path().join("state.db")).unwrap();
    let (base, remote, server) = serve_siyuan().await;
    let sink = SiYuanSink::embedded(base, "AI Knowledge").unwrap();
    let large = "content\n".repeat(800_000);
    let request = || SessionVolumeRequest {
        db: &db,
        session_db_id: 700,
        source: "codex",
        external_id: "rollout-long-session",
        parser_version: "codex-v1",
        notebook_id: "box-1",
        base_path: "/10 AI Sessions/Codex/2026/10/test",
        markdown: &large,
    };
    let index = sink.sync_session_volumes(request()).await.unwrap();
    assert!(index.contains("第 1 部分"));
    assert!(index.contains("第 2 部分"));
    let created = remote.lock().unwrap().creates;
    assert!(created >= 2);
    assert_eq!(sink.sync_session_volumes(request()).await.unwrap(), index);
    assert_eq!(
        remote.lock().unwrap().creates,
        created,
        "retry created duplicate volumes"
    );

    let first_id = {
        let state = remote.lock().unwrap();
        state
            .documents
            .iter()
            .find(|(_, (path, _))| path.ends_with("Part 0001"))
            .unwrap()
            .0
            .clone()
    };
    {
        let mut state = remote.lock().unwrap();
        state
            .documents
            .get_mut(&first_id)
            .unwrap()
            .1
            .push_str("\nuser edit");
    }
    let conflict = sink.sync_session_volumes(request()).await.unwrap_err();
    assert!(conflict.to_string().contains("edited"));
    assert_eq!(
        remote.lock().unwrap().creates,
        created,
        "conflict wrote new documents"
    );

    remote.lock().unwrap().documents.remove(&first_id);
    let recovered = sink.sync_session_volumes(request()).await.unwrap();
    assert_ne!(
        recovered, index,
        "restored volume should reference its new document id"
    );
    assert_eq!(remote.lock().unwrap().creates, created + 1);
    assert_eq!(
        sink.sync_session_volumes(request()).await.unwrap(),
        recovered
    );
    server.abort();
}
