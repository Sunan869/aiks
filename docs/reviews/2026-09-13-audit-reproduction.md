# 2026-09-13 AIKS 审计复现附件

基线：49ae827dfa157fd015501564c22cbf4612ae83d7。

本附件保留本次实际运行的 13 个匿名审计探针。它们断言的是错误行为存在，13/13 通过表示成功复现，不表示业务正确。修复后应把相关断言改为期望业务行为，加入正式回归测试。

执行环境：Windows / PowerShell；在项目根目录运行：

    cargo test -p aiks-core --test review_repro -- --test-threads=1

结果：13 passed; 0 failed; 0 ignored，耗时 4.68s。此前 cargo test --workspace 的 84 个既有 core 测试全部通过；CLI 和 Desktop 没有单元测试。npm run build 也通过。

## 复现索引

| 探针 | 确认的问题 |
|---|---|
| repro_dry_run_poisoning | Core dry-run 后正式同步误报 UNCHANGED，target 数量为 0 |
| repro_failure_not_retried | 首次远端失败后第二次不重试 |
| repro_renderer_unicode_panic | Renderer 在中文 UTF-8 边界截断时 panic |
| repro_chunker_unicode_panic | V3 Chunker 在中文 UTF-8 边界截断时 panic |
| repro_secret_bypasses | Unknown 和常见 env/引号凭据未脱敏 |
| repro_reextraction_foreign_key_failure | 已有知识切片后重提炼出现外键失败 |
| repro_fts_keeps_deleted_items | 重提炼留下 FTS 孤儿，Core hybrid_search 返回已删除 ID |
| repro_hash_ignores_title_and_image_tail | 标题和图片尾部改变但 hash 不变 |
| repro_resync_id_without_source_does_nothing | 单独传 Session ID 时不重置 hash |
| repro_chunk_token_limit_exceeded | 单条长消息生成超过 20k 估算 token 的 chunk |
| repro_missing_not_marked_by_sync | 普通同步不标记 missing source |
| repro_siyuan_string_id_response_rejected | 思源正确字符串 ID 响应被当前客户端拒绝 |
| repro_worker_parse_error_stays_processing | Worker 的 Provider 错误后停留 PROCESSING |

## 重新运行

将下面 Rust 代码块完整保存为 E:/Project/Htrs/AISK/crates/aiks-core/tests/review_repro.rs；不存在 tests 目录时先创建。运行上述 cargo 命令即可。若该位置已有文件，不要覆盖，应改用另一个临时测试文件名并对应修改 --test 参数。

探针只使用匿名数据、临时 SQLite、临时目录和回环 HTTP，不调用真实 Provider、公司 AI 或真实思源。panic 由 catch_unwind 捕获。Worker 测试使用 Tokio 默认单线程 test runtime，不故意执行不安全的并发压力场景。

注意：完整代码中的断言刻意匹配审查基线的 Bug；后续版本修复后，探针失败可能意味着问题已经修复。它不是长期质量门禁的替代品。

## 完整探针源码
```rust
// Temporary audit probes. These assert the observed BUG, not desired behavior.
use std::{collections::HashMap, sync::Arc};
use aiks_core::{config::{Config, ContentConfig}, model::*, providers::*, renderer::MarkdownRenderer, sink::SiYuanSink, storage::{StateDb, SourceSessionRepo}, sync::{SyncEngine, SyncOptions}, util::SecretSanitizer};
use aiks_core::pipeline::{knowledge_repo::KnowledgeRepo, session_chunker::chunk_for_llm};
use async_trait::async_trait;

fn msg(block: ContentBlock) -> NormalizedMessage {
    NormalizedMessage { external_id: "m1".into(), parent_id: None, role: MessageRole::User, created_at: None, model: None, blocks: vec![block], usage: None, metadata: HashMap::new() }
}
fn session(block: ContentBlock) -> NormalizedSession {
    NormalizedSession { source: SourceKind::ClaudeCode, external_session_id: "audit-session".into(), title: Some("Audit".into()), project_name: None, project_path: None, source_path: None, started_at: None, updated_at: None, model: None, messages: vec![msg(block)], usage: None, metadata: HashMap::new() }
}
fn summary() -> SessionSummary {
    SessionSummary { source: SourceKind::ClaudeCode, external_session_id: "audit-session".into(), title: None, project_name: None, project_path: None, source_path: None, started_at: None, updated_at: None, message_count: 1 }
}
struct FakeProvider;
#[async_trait]
impl SessionProvider for FakeProvider {
    fn source(&self) -> SourceKind { SourceKind::ClaudeCode }
    fn parser_version(&self) -> &'static str { "audit-v1" }
    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> { Ok(vec![summary()]) }
    async fn load_session(&self, _: &SessionSummary) -> anyhow::Result<NormalizedSession> { Ok(session(ContentBlock::Text{text:"hello".into()})) }
    async fn health_check(&self) -> ProviderHealth { ProviderHealth::Ok }
}
fn insert(db: &StateDb) -> i64 {
    SourceSessionRepo::new(db).upsert("claude_code", "audit-session", None, None, None, None, None, Some("hash"), Some("audit-v1")).unwrap()
}
fn extraction() -> aiks_core::ai::schema_v3::V3ExtractionResult {
    serde_json::from_value(serde_json::json!({"session_summary":"audit", "knowledge_score":0.9,"worth_extracting":true,"items":[{"title":"auditneedle", "category":"general", "summary":"auditneedle", "content":"auditneedle", "tags":[], "confidence":0.9}]})).unwrap()
}
#[tokio::test]
async fn repro_dry_run_poisoning() {
    let dir=tempfile::tempdir().unwrap(); let db=StateDb::open(&dir.path().join("state.db")).unwrap();
    let engine=SyncEngine::new(Arc::new(Config::default())); let registry=ProviderRegistry::new(vec![Box::new(FakeProvider)]);
    let sink=SiYuanSink::embedded("http://127.0.0.1:1", "audit").unwrap();
    let dry=engine.run_sync(&db,&registry,&sink,&SyncOptions{dry_run:true,..Default::default()}).await.unwrap();
    let real=engine.run_sync(&db,&registry,&sink,&SyncOptions::default()).await.unwrap();
    assert_eq!(dry.new_count,1); assert_eq!(real.unchanged_count,1); assert_eq!(real.failed_count,0);
    assert_eq!(db.conn().query_row("SELECT COUNT(*) FROM sync_target",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}
#[tokio::test]
async fn repro_failure_not_retried() {
    let dir=tempfile::tempdir().unwrap(); let db=StateDb::open(&dir.path().join("state.db")).unwrap();
    let engine=SyncEngine::new(Arc::new(Config::default())); let registry=ProviderRegistry::new(vec![Box::new(FakeProvider)]);
    let sink=SiYuanSink::embedded("http://127.0.0.1:1", "audit").unwrap();
    let first=engine.run_sync(&db,&registry,&sink,&SyncOptions::default()).await.unwrap();
    let second=engine.run_sync(&db,&registry,&sink,&SyncOptions::default()).await.unwrap();
    assert_eq!(first.failed_count,1); assert_eq!(second.unchanged_count,1);
}
#[test]
fn repro_renderer_unicode_panic() {
    let s=session(ContentBlock::ToolResult{id:None,content:"中".repeat(4000),is_error:false});
    assert!(std::panic::catch_unwind(||MarkdownRenderer::new(ContentConfig::default(),true).render(&s)).is_err());
}
#[test]
fn repro_chunker_unicode_panic() {
    let m=msg(ContentBlock::ToolResult{id:None,content:"中".repeat(1000),is_error:false});
    assert!(std::panic::catch_unwind(||chunk_for_llm(1,&[m])).is_err());
}
#[test]
fn repro_secret_bypasses() {
    let s=session(ContentBlock::Unknown{raw:serde_json::json!({"password":"AUDIT_ONLY_PASSWORD"})});
    assert!(MarkdownRenderer::new(ContentConfig::default(),true).render(&s).contains("AUDIT_ONLY_PASSWORD"));
    for input in ["AWS_SECRET_ACCESS_KEY=AUDIT_ONLY_123456", "password=\"AUDIT_ONLY_123456\"", "SecretKey=AUDIT_ONLY_123456", "token = AUDIT_ONLY_123456"] {
        assert_eq!(SecretSanitizer::new().sanitize(input), input);
    }
}
#[test]
fn repro_reextraction_foreign_key_failure() {
    let dir=tempfile::tempdir().unwrap(); let db=StateDb::open(&dir.path().join("state.db")).unwrap(); let sid=insert(&db);
    let repo=KnowledgeRepo::new(&db); let ids=repo.save_items(sid,None,&extraction()).unwrap();
    repo.save_embedding_chunks(&ids[0],&[(None,"audit".into())]).unwrap();
    assert!(repo.save_items(sid,None,&extraction()).unwrap_err().to_string().contains("FOREIGN KEY"));
}
#[tokio::test]
async fn repro_fts_keeps_deleted_items() {
    let dir=tempfile::tempdir().unwrap(); let db=StateDb::open(&dir.path().join("state.db")).unwrap(); let sid=insert(&db);
    let repo=KnowledgeRepo::new(&db); let old=repo.save_items(sid,None,&extraction()).unwrap();
    repo.save_items(sid,None,&extraction()).unwrap();
    let hits=aiks_core::pipeline::hybrid_search(&db,"auditneedle",20,None).await.unwrap();
    assert_eq!(hits.len(),2); assert!(hits.iter().any(|h|h.knowledge_id==old[0])); assert!(repo.get_by_id(&old[0]).unwrap().is_none());
}
#[test]
fn repro_hash_ignores_title_and_image_tail() {
    let mut s=session(ContentBlock::Image{source:"a".repeat(130),media_type:None}); let original=aiks_core::model::hash::compute_session_hash(&s);
    s.title=Some("changed".into()); s.messages[0].blocks=vec![ContentBlock::Image{source:format!("{}bb","a".repeat(128)),media_type:None}];
    assert_eq!(aiks_core::model::hash::compute_session_hash(&s),original);
}
#[test]
fn repro_resync_id_without_source_does_nothing() {
    let dir=tempfile::tempdir().unwrap(); let db=StateDb::open(&dir.path().join("state.db")).unwrap(); insert(&db);
    db.reset_hashes_for_resync(None,&["audit-session".into()]).unwrap();
    assert_eq!(SourceSessionRepo::new(&db).find_by_source_and_id("claude_code","audit-session").unwrap().unwrap().content_hash.as_deref(),Some("hash"));
}
#[test]
fn repro_chunk_token_limit_exceeded() {
    let c=chunk_for_llm(1,&[msg(ContentBlock::Text{text:"x".repeat(100000)})]); assert!(c.chunks[0].token_count>20000);
}
#[tokio::test]
async fn repro_missing_not_marked_by_sync() {
    let dir=tempfile::tempdir().unwrap(); let db=StateDb::open(&dir.path().join("state.db")).unwrap(); insert(&db);
    SyncEngine::new(Arc::new(Config::default())).run_sync(&db,&ProviderRegistry::new(vec![]),&SiYuanSink::embedded("http://127.0.0.1:1","audit").unwrap(),&SyncOptions::default()).await.unwrap();
    assert!(!SourceSessionRepo::new(&db).find_by_source_and_id("claude_code","audit-session").unwrap().unwrap().is_missing);
}
#[tokio::test]
async fn repro_siyuan_string_id_response_rejected() {
    use tokio::io::{AsyncReadExt,AsyncWriteExt};
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(); let address=listener.local_addr().unwrap();
    let server=tokio::spawn(async move { let (mut stream,_)=listener.accept().await.unwrap(); let mut buf=[0;8192]; let _=stream.read(&mut buf).await.unwrap();
        let body=r#"{"code":0,"msg":"","data":"20260913000000-auditxx"}"#;
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap(); });
    let result=SiYuanSink::embedded(format!("http://{}",address),"audit").unwrap().create_document("notebook","/audit","hello").await;
    server.await.unwrap(); assert!(result.is_err()); assert!(format!("{:#}",result.unwrap_err()).contains("invalid type"));
}
#[tokio::test]
async fn repro_worker_parse_error_stays_processing() {
    use aiks_core::pipeline::{worker::{PipelineWorker,PipelineJob},orchestrator::PipelineOrchestrator};
    let dir=tempfile::tempdir().unwrap(); let db=Arc::new(StateDb::open(&dir.path().join("state.db")).unwrap()); let sid=insert(&db);
    let run=PipelineOrchestrator::new(db.clone()).enqueue(sid,Some("hash")).unwrap();
    let worker=PipelineWorker::start(db.clone(),Arc::new(ProviderRegistry::new(vec![])),Default::default(),Default::default());
    worker.submit(PipelineJob{pipeline_run_id:run.clone(),session_id:sid,session_external_id:"audit-session".into(),source:"claude_code".into(),session_title:None,project_name:None});
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let status:String=db.conn().query_row("SELECT status FROM pipeline_run WHERE id=?1",[&run],|r|r.get(0)).unwrap(); assert_eq!(status,"PROCESSING");
}

```
