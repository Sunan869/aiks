# 第二轮审查匿名复现附件

基线：`7c76486af0f0dfa1bf4c8474d7e6b1fc4482d4eb`，日期：2026-09-13。

对应[主报告](2026-09-13-project-audit-round2.md)。以下测试断言的是当前错误行为，10 个测试通过表示成功复现缺陷，不是修复验收通过。

复现时将下方代码保存到 `crates/aiks-core/tests/review_round2_temp.rs`，在项目根目录执行：

```powershell
cargo test -p aiks-core --test review_round2_temp -- --nocapture
```

本轮结果：10 passed，0 failed。两个 UTF-8 panic 被 catch_unwind 有意捕获，因此日志有 panic 提示但测试通过。默认 SQL 实际错误为 `InvalidParameterCount(3, 2)`；初次探针错误地断言为 `(4, 2)`，修正为验证真实返回错误后重跑，10 项均通过。没有通过更改业务代码影响结果。

全部使用临时 SQLite、虚构 Provider、localhost Mock 和假凭据；不访问真实 Session 或外部服务。HTTP Mock 只验证代码选择的请求路径及返回处理，不能证明真实 SiYuan 对同路径创建的具体行为。SQL 探针复现实际命令的 SQL/绑定组合，没有启动 Tauri 窗口。

## 复现源码

```rust
// Round 2 audit probes: assert observed bugs, not desired behavior.
use std::{collections::HashMap,sync::Arc};
use aiks_core::{config::Config,model::*,providers::*,storage::{StateDb,SourceSessionRepo,SyncTargetRepo,SyncStatus},sync::{SyncEngine,SyncOptions},sink::SiYuanSink};
use aiks_core::pipeline::{session_chunker::{chunk_for_llm,save_chunks},ai_stage::AiStage,repo::PipelineRepo};
use async_trait::async_trait;
fn message(block:ContentBlock)->NormalizedMessage {NormalizedMessage{external_id:"m".into(),parent_id:None,role:MessageRole::User,created_at:None,model:None,blocks:vec![block],usage:None,metadata:HashMap::new()}}
fn session(text:&str)->NormalizedSession {NormalizedSession{source:SourceKind::ClaudeCode,external_session_id:"audit2".into(),title:Some("audit".into()),project_name:None,project_path:None,source_path:None,started_at:None,updated_at:None,model:None,messages:vec![message(ContentBlock::Text{text:text.into()})],usage:None,metadata:HashMap::new()}}
struct Provider{fail:bool}
#[async_trait] impl SessionProvider for Provider {
 fn source(&self)->SourceKind{SourceKind::ClaudeCode}
 fn parser_version(&self)->&'static str{"v1"}
 async fn health_check(&self)->ProviderHealth{ProviderHealth::Ok}
 async fn discover_sessions(&self)->anyhow::Result<Vec<SessionSummary>> {
  if self.fail {anyhow::bail!("audit provider unavailable");}
  Ok(vec![SessionSummary{source:SourceKind::ClaudeCode,external_session_id:"audit2".into(),title:None,project_name:None,project_path:None,source_path:None,started_at:None,updated_at:None,message_count:1}])
 }
 async fn load_session(&self,_:&SessionSummary)->anyhow::Result<NormalizedSession>{Ok(session("new"))}
}
fn insert(db:&StateDb,hash:&str)->i64{SourceSessionRepo::new(db).upsert("claude_code","audit2",None,None,None,None,None,Some(hash),Some("v1")).unwrap()}
fn seeded(db:&StateDb)->i64{let id=insert(db,"old");let repo=SyncTargetRepo::new(db);repo.upsert_pending(id,"siyuan").unwrap();repo.mark_synced(id,"siyuan","existing-doc","/old","old","target-old").unwrap();id}
async fn server(attr_fail:bool)->(String,Arc<std::sync::Mutex<Vec<String>>>,tokio::task::JoinHandle<()>) {
 use tokio::io::{AsyncReadExt,AsyncWriteExt};
 let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
 let url=format!("http://{}",listener.local_addr().unwrap());let seen=Arc::new(std::sync::Mutex::new(vec![]));let copy=seen.clone();
 let handle=tokio::spawn(async move {loop{let(mut socket,_)=listener.accept().await.unwrap();let mut buf=vec![];loop{let mut b=[0;4096];let n=socket.read(&mut b).await.unwrap();if n==0{break;}buf.extend_from_slice(&b[..n]);if let Some(i)=buf.windows(4).position(|w|w==b"\r\n\r\n"){let header=String::from_utf8_lossy(&buf[..i]);let len=header.lines().find_map(|l|l.to_lowercase().strip_prefix("content-length:").and_then(|s|s.trim().parse::<usize>().ok())).unwrap_or(0);if buf.len()>=i+4+len {break;}}}
 let req=String::from_utf8_lossy(&buf);let path=req.split_whitespace().nth(1).unwrap_or("").to_string();copy.lock().unwrap().push(path.clone());
 let body=if path.contains("lsNotebooks"){serde_json::json!({"code":0,"msg":"","data":{"notebooks":[{"id":"nb","name":"audit"}]}})}
 else if path.contains("createDocWithMd"){serde_json::json!({"code":0,"msg":"","data":"new-doc"})}
 else if path.contains("setBlockAttrs")&&attr_fail{serde_json::json!({"code":-1,"msg":"audit attr failure","data":null})}
 else if path.contains("completions"){serde_json::json!({"choices":[{"message":{"content":"not valid JSON"}}]})}
 else{serde_json::json!({"code":0,"msg":"","data":null})};
 let body=body.to_string();socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
 }});
 (url,seen,handle)
}
#[tokio::test] async fn r2_update_uses_create_despite_saved_target(){
 let dir=tempfile::tempdir().unwrap();let db=StateDb::open(&dir.path().join("db")).unwrap();seeded(&db);
 let(url,seen,h)=server(false).await;
 let stats=SyncEngine::new(Arc::new(Config::default())).run_sync(&db,&ProviderRegistry::new(vec![Box::new(Provider{fail:false})]),&SiYuanSink::embedded(url,"audit").unwrap(),&SyncOptions::default()).await.unwrap();h.abort();
 assert_eq!(stats.updated_count,1);let paths=seen.lock().unwrap();assert!(paths.iter().any(|p|p.contains("createDocWithMd")));assert!(!paths.iter().any(|p|p.contains("updateBlock")));
}
#[tokio::test] async fn r2_existing_target_dry_run_swallows_update(){
 let dir=tempfile::tempdir().unwrap();let db=StateDb::open(&dir.path().join("db")).unwrap();let id=seeded(&db);
 let engine=SyncEngine::new(Arc::new(Config::default()));let reg=ProviderRegistry::new(vec![Box::new(Provider{fail:false})]);let sink=SiYuanSink::embedded("http://127.0.0.1:1","audit").unwrap();
 let dry=engine.run_sync(&db,&reg,&sink,&SyncOptions{dry_run:true,..Default::default()}).await.unwrap();
 let real=engine.run_sync(&db,&reg,&sink,&SyncOptions::default()).await.unwrap();
 assert_eq!(dry.updated_count,1);assert_eq!(real.unchanged_count,1);assert_eq!(SyncTargetRepo::new(&db).find(id,"siyuan").unwrap().unwrap().synced_hash.as_deref(),Some("old"));
}
#[tokio::test] async fn r2_attribute_failure_marked_synced(){
 let dir=tempfile::tempdir().unwrap();let db=StateDb::open(&dir.path().join("db")).unwrap();let(url,_,h)=server(true).await;
 let stats=SyncEngine::new(Arc::new(Config::default())).run_sync(&db,&ProviderRegistry::new(vec![Box::new(Provider{fail:false})]),&SiYuanSink::embedded(url,"audit").unwrap(),&SyncOptions::default()).await.unwrap();h.abort();
 let id=SourceSessionRepo::new(&db).find_by_source_and_id("claude_code","audit2").unwrap().unwrap().id;
 assert_eq!(stats.new_count,1);assert_eq!(SyncTargetRepo::new(&db).find(id,"siyuan").unwrap().unwrap().status,SyncStatus::Synced);
}
#[tokio::test] async fn r2_provider_failure_marks_missing(){
 let dir=tempfile::tempdir().unwrap();let db=StateDb::open(&dir.path().join("db")).unwrap();insert(&db,"old");
 let count=SyncEngine::new(Arc::new(Config::default())).mark_missing_sessions(&db,&ProviderRegistry::new(vec![Box::new(Provider{fail:true})])).await.unwrap();assert_eq!(count,1);
}
#[test] fn r2_cleaner_cjk_panics(){
 let result=std::panic::catch_unwind(||aiks_core::pipeline::cleaner::clean_messages(vec![message(ContentBlock::ToolResult{id:None,content:"中".repeat(3000),is_error:false})]));assert!(result.is_err());
}
#[test] fn r2_typed_ai_error_cjk_panics(){
 assert!(std::panic::catch_unwind(||aiks_core::pipeline::ai_stage::parse_v3_result_typed(&"中".repeat(40))).is_err());
}
#[test] fn r2_cjk_chunk_still_over_budget_and_tail_lost(){
 let chunks=chunk_for_llm(1,&[message(ContentBlock::Text{text:format!("{}AUDIT_TAIL","中".repeat(80000))})]);
 assert!(chunks.chunks[0].token_count>20000);assert!(!chunks.chunks.iter().any(|c|c.content.contains("AUDIT_TAIL")));
}
#[tokio::test] async fn r2_ai_stage_malformed_still_success(){
 let dir=tempfile::tempdir().unwrap();let db=StateDb::open(&dir.path().join("db")).unwrap();let id=insert(&db,"hash");let run=PipelineRepo::new(&db).upsert_pipeline_run(id,Some("hash"),"v3").unwrap();
 save_chunks(&db,&chunk_for_llm(id,&session("hello").messages).chunks).unwrap();
 let(url,_,h)=server(false).await;let mut cfg=aiks_core::ai::config::AiModelConfig::default();cfg.base_url=url;
 let result=AiStage::new(cfg).unwrap().run(&db,&run,id,None,None).await;h.abort();assert_eq!(result.unwrap(),0);
 let status:String=db.conn().query_row("SELECT status FROM pipeline_stage_run WHERE pipeline_run_id=?1 AND stage='AI_EXTRACTED'",[&run],|r|r.get(0)).unwrap();assert_eq!(status,"SUCCESS");
}
#[test] fn r2_title_secret_not_sanitized(){
 let mut s=session("ordinary");s.title=Some("password=AUDITSECRET123".into());let md=aiks_core::renderer::MarkdownRenderer::new(Default::default(),true).render(&s);assert!(md.contains("AUDITSECRET123"));
}
#[test] fn r2_default_knowledge_query_parameter_mismatch(){
 let dir=tempfile::tempdir().unwrap();let db=StateDb::open(&dir.path().join("db")).unwrap();let conn=db.conn();
 let mut stmt=conn.prepare("SELECT id, source_session_id, project_name, title, category, summary, tags, confidence, created_at, updated_at FROM knowledge_item ORDER BY updated_at DESC LIMIT ?1 OFFSET ?2").unwrap();
 let result=stmt.query_map(rusqlite::params![50i64,0i64,"",""],|r|r.get::<_,String>(0));
 match result { Err(e) => println!("default knowledge query rejected: {e:?}"), Ok(_) => panic!("expected surplus bindings to fail") };
}

```

