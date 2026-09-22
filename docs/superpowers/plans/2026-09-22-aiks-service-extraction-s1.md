# AIKS Service Extraction S1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交付可独立运行、仅本机访问的 AIKS Service，通过持久化会话快照驱动现有 Pipeline，并让桌面端第一条采集、处理状态与查询链路使用相同的 HTTP 业务接口。

**Architecture:** 保留 Rust Core，新增无 Tauri 依赖的 `apps/aiks-service`。采集器上传标准化快照；Service 独占业务库与快照 Worker，客户端只保留采集状态和发送队列。S1 只允许本地单用户，团队监听、共享权限、受控内容编辑与完整 RAG 分别在设计的 S2—S4 交付，不能因服务可启动就开启团队模式。

**Tech Stack:** 现有 Rust 2021、Tokio、Serde、rusqlite 0.31、reqwest 0.12；服务入口采用 Axum 0.8；文件级运行所有权使用 fs2 0.4；现有 React/Vite/Tauri。新增版本以实际生成并验证的 Cargo.lock 固定，不更换数据库、不引入 Redis/RabbitMQ、不手写 HTTP 解析器。

**Spec:** `docs/superpowers/specs/2026-09-22-aiks-service-extraction-design.md`

**Branch / baseline:** `feature/aiks-service-extraction`，计划前 HEAD `ab814594d900dbe53a4e4f83f009d61f98f2d46b`，产品基线 `e0f2d8a28938b3f0e4bcc64cec11a1b42ae2e740`。用户已确认书面设计并要求推进。此计划提交不包含产品代码，不表示功能或测试完成；实施方法建议为当前会话逐任务执行。计划审阅通过后开始下列 RED/GREEN 周期。

## Global Constraints

以下约束从已确认设计原文摘录，对所有任务生效：

- 完整本地个人版，无需用户另搭服务器、注册团队账户或安装 Docker；离线资源预置后可以断网启动。
- 已集成的 AI 工具与现有会话、知识、搜索回归；不重写解析器，不清空已有数据库。
- 团队个人私有、项目共享、部门共享；本地私有资料不因登录公司或切换服务地址自动上传。
- **服务端思源不直接开放给普通用户。普通用户仅通过经过认证和授权的 AIKS 业务接口读写，不获得思源管理员 Token，不开放任意思源接口代理。**
- Service 必须能在没有图形桌面、没有任何 AI 工具安装的机器上消费已接收快照。
- 服务化完成但多人授权未完成的构建只允许本机单用户验证，不得作为团队服务开放。不能把改监听地址当成启用团队权限。
- 首期保留 SQLite 和现有正式 migration，运行单个业务 Service 写入者。不得把业务 SQLite 放到客户端共享网络目录，也不宣称已有多实例水平扩容。
- 生成答案不是发布知识，写入共享空间是独立受控动作。首批服务抽离先保留现有搜索/AI Assist；完整问答作为后续里程碑，不能以一个聊天框宣布完成。
- `main` 不修改，不自动合并、不发布安装包。所有测试只用合成会话、临时目录和回环测试服务，不连接用户的真实模型或思源工作区。

## Review Focus

- HTTP 响应丢失后重传、同一提交 ID 换内容：只能获得同一回执或冲突，不能重复建会话/任务。归属 Task 3、Task 8。
- 旧版本模型调用晚于新快照完成：不能覆盖新索引、知识或当前状态；检查必须与写入处于同一事务。归属 Task 4、Task 5。
- 服务已启动而旧 Desktop/CLI 也打开同一业务库：必须拒绝第二个写入者，不能只在 Service 自己内部加锁。归属 Task 2、Task 9。
- 切换服务地址但队列仍有私有内容：原队列目标身份不变，不能上传到新服务；同端口换成陌生服务也不能信任。归属 Task 6、Task 8、Task 9。
- 老库没有会话源文件、网络关闭或模型未配置：保留旧 ID、映射及可读数据，明确报告无法重新处理；不得删除记录、卡死启动或转用云模型。归属 Task 7、Task 9、Task 10。

---

## 0. 交付分段与状态口径

本计划完整覆盖设计 S1：可执行服务、接收/持久快照、真实 Worker、个人查询接口、首条 Desktop HTTP 链路及受控升级。先完成 Tasks 1—6 的无界面闭环，再完成 Tasks 7—10 的个人版接线与回归；同一分支内分批提交，不把第一批称为整个 S1 完成。

后续设计目标不删减，但各自形成独立实施计划：

| 设计范围 | 交付归属 | S1 必须留下的边界 |
| --- | --- | --- |
| 主体/实例/空间上下文 | S1 提供本地唯一主体和私有空间；S2 实现多人 | DTO 不允许自报 owner/role；所有业务操作先解析受信上下文 |
| 项目/部门角色、跨成员授权、审计 | S2 | `team=false`，外部地址启动失败，不开放共享写入 |
| 内部思源的文档/块/资产写入与 outbox | S2 | S1 不提供通用代理、原生思源 UI、直接写入或下载任意 URL |
| 脱离 Desktop 的完整内容变更生命周期 | S2；S1 只读端进行版本校验 | 不依赖 Tauri 事件作为服务事实源，不返回失效缓存冒充现行正文 |
| 全部桌面业务迁移、sidecar 发布包、远程部署 | S3；S1 提供开发用本机启动与首条链路 | 只关闭自己启动的进程；不发布尚未验收的离线安装包 |
| 权限内 RAG、引用与追问 | S4 | 复用 Service/Core 搜索与模型，不用 AI Assist 假装问答 |
| 共享发布、披露检查、撤销 | S2 | 本地旧数据不自动上传，接收队列绑定实例/来源/空间 |

S1 的知识读取、查询、AI Assist 复用现有 Core；文档创建/编辑/发布 API 在 S2 的持久内容意图与授权一起完成。S1 服务模式的界面必须明确禁用尚未迁移的写入口，不能偷偷回退到直接写本地业务库。完整旧版界面作为独立 legacy 模式保留，不与 Service 同时写同一库。

## 1. 文件与职责

新建文件：

```text
crates/aiks-core/src/service/mod.rs            服务业务入口导出；不依赖 HTTP/Tauri
crates/aiks-core/src/service/contracts.rs      有版本的 DTO、能力与稳定错误码
crates/aiks-core/src/service/validation.rs     会话结构、完整性、大小和指纹校验
crates/aiks-core/src/service/repo.rs           主体/来源登记、快照/回执事务
crates/aiks-core/src/service/revision.rs       当前修订检查与事务内写入保护
crates/aiks-core/src/service/runtime.rs        无 Provider 的业务启动、停止
crates/aiks-core/src/service/query.rs          本地身份授权后的业务读取与搜索
crates/aiks-core/src/storage/ownership.rs      单写入者文件锁，所有运行入口共同遵守
crates/aiks-core/src/pipeline/input.rs         legacy Provider 与快照加载边界
crates/aiks-core/migrations/012_service_snapshots.sql
apps/aiks-service/Cargo.toml
apps/aiks-service/src/{main,lib,config,bootstrap,auth,error,routes}.rs
apps/aiks-service/tests/{http_contract,lifecycle,offline_flow}.rs
crates/aiks-core/tests/service_{contracts,storage,worker,revisions,migration}.rs
crates/aiks-core/tests/support/service_fixture.rs
apps/aiks-desktop/src-tauri/src/service_client/{mod,transport,supervisor,collector,outbox}.rs
apps/aiks-desktop/src/api/service.ts
apps/aiks-desktop/src/api/desktop-platform.ts
apps/aiks-desktop/src/api/service.test.ts
apps/aiks-desktop/src/pages/ServiceStatusPage.tsx
apps/aiks-desktop/src/pages/ServiceStatusPage.test.tsx
docs/implementation/aiks-service-s1.md
```

定点修改，禁止全仓无关重构：

```text
Cargo.toml / Cargo.lock
crates/aiks-core/Cargo.toml
crates/aiks-core/src/lib.rs
crates/aiks-core/src/storage/{mod,db,repo}.rs
crates/aiks-core/src/pipeline/{worker,job_repo,repo,ai_stage,knowledge_repo,session_chunker}.rs
crates/aiks-core/src/indexing/session.rs
apps/aiks-cli/src/main.rs
apps/aiks-desktop/src-tauri/{Cargo.toml,src/lib.rs,src/lifecycle.rs,src/app_state.rs}
apps/aiks-desktop/src/api/{index,client,types}.ts
apps/aiks-desktop/src/App.tsx
.github/workflows/ci.yml
```

现有 TypeScript 业务 API 与 desktop window API 分离，但第一批不批量移动页面。现有 Provider、ModelService、UnifiedSearchService、KnowledgeRepo 是唯一算法实现。

## 2. 锁定的协议与安全参数

S1 HTTP 前缀 `/api/v1`，只允许 `127.0.0.1` 或 `::1` 数值监听地址；默认 `127.0.0.1:0`。启动配置的 `mode` 只能为 `personal`；`team` 和非 loopback 地址返回明确配置错误，不能自动降级。

所有业务 ID 以字符串输出；revision 为 0 到 u32::MAX 的整数，0 表示尚无版本。`expected_revision` 使用 CAS，不能用客户端时间戳判先后。超过 u32::MAX 返回 `revision_exhausted`。

新服务限制：请求体 16 MiB、解码后最多 20,000 条消息、每条最多 256 个 block、所有块/元数据总大小仍受 16 MiB 约束；不接受 Content-Encoding 压缩，避免解压预算旁路。bootstrap 帧最多 4 KiB。分页 limit 默认 30、最大 100。上传/请求读取超时 30 秒；AI 长任务只排队，不占上传响应。搜索继续保留现有 8 秒查询向量预算。S1 首条状态进度用 1 秒轮询，不新增第二套任务队列。

| 方法/路由 | 语义 |
| --- | --- |
| GET `/healthz` | 仅 `{"status":"ok"}`，无版本、路径、Token 或诊断 |
| GET `/api/v1/capabilities` | 认证后返回 instance_id、api_version=1、本地 space_id 和能力；team/content_write/rag=false |
| POST `/api/v1/source-registrations` | 在本地受信上下文内登记来源实例；同一个客户端 registration_key 幂等 |
| POST `/api/v1/session-snapshots` | 原子接收、创建 revision/job/receipt；新接收 202，重复回执 200 |
| GET `/api/v1/receipts/{receipt_id}` | 回执与对应任务身份，不把接收等同提炼成功 |
| GET `/api/v1/sessions`、`/sessions/{session_id}` | 分页元数据/受控快照详情，scope 来自服务上下文 |
| GET `/api/v1/jobs/{job_id}` | 当前任务状态、阶段和脱敏错误，不返回模型地址/Token |
| POST `/api/v1/search` | 原有 UnifiedSearch，语料/source/project 是业务过滤不是授权 |
| GET `/api/v1/knowledge`、`/knowledge/{knowledge_id}` | 原有知识身份与规范正文读取；未发布候选明确标记 |
| POST `/api/v1/knowledge/{knowledge_id}/assist` | 从固定内容适配器取授权正文，调用现有 AI Assist；不写正文 |

没有 `/proxy/*`、`/api/sql/*`、`/api/file/*`、任意 upstream/path/url 参数、远程关机接口、原生思源页面或全库导出。未实现 route 返回 404，不转发到思源。

认证失败 401；未存在/不属于当前上下文的资源均 404；版本/幂等冲突 409；超限 413；不完整快照 422；依赖不可用 503。所有错误采用 `{"error":{"code":"...","request_id":"...","message":"..."}}`，业务 message 来自固定安全文案，不回传 anyhow/SQL/reqwest 原始错误。

## Task 1: 定义可测试的快照协议，不改变原有模型

**Files:** Create `service/{mod,contracts,validation}.rs`、`tests/service_contracts.rs`、`tests/support/service_fixture.rs`；Modify Core `lib.rs`。

**Interfaces:** 复用 `NormalizedSession` 和 `SourceKind::as_str()`。新 DTO 定义如下；不重命名现有 source key，不改变 `NormalizedSession` 序列化来迁就 HTTP。

```rust
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotSubmission {
    pub api_version: u16,
    pub submission_id: String,
    pub service_instance_id: String,
    pub space_id: String,
    pub source_registration_id: String,
    pub expected_revision: u32,
    pub complete: bool,
    pub parser_version: String,
    pub session: crate::model::NormalizedSession,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotReceipt {
    pub receipt_id: String,
    pub session_id: String,
    pub snapshot_id: String,
    pub revision: u32,
    pub job_id: String,
    pub pipeline_run_id: String,
    pub state: String,
}
pub struct ValidatedSnapshot {
    pub submission: SnapshotSubmission,
    pub canonical_json: Vec<u8>,
    pub request_hash: String,
    pub content_hash: String,
}
// validate_submission(&SnapshotSubmission) -> Result<ValidatedSnapshot, ServiceError>
// ServiceError::code(&self) -> &'static str
```

`request_hash` 覆盖整个提交中除 submission_id 外的字段；`content_hash` 覆盖 canonical session 和 parser_version，因此只改标题或升级 parser 也生成新版本。递归按键排序 JSON object、保持数组顺序后 SHA-256，不能用 HashMap 迭代顺序或压缩前字节作内容指纹。客户端不声明自己的权限或所有者；服务身份在 Task 3 解析。

- [ ] **Step 1: 写 RED 测试与共享样例。** 共享样例函数 `fixture_session(text: &str) -> NormalizedSession` 构造 source=Continue、external_session_id=`synthetic-1`、一条 User/Text 消息，其余 Option 为 None、metadata 空；`submission(space, instance, registration, id, expected, text)` 返回上述完整 DTO。仅 metadata 参数顺序不同不改变 hash。

```rust
#[test]
fn incomplete_snapshot_is_explicitly_rejected() {
    let mut input = submission("s", "i", "r", "u1", 0, "synthetic text");
    input.complete = false;
    assert_eq!(validate_submission(&input).unwrap_err().code(), "incomplete_snapshot");
}
#[test]
fn title_only_change_is_a_new_snapshot() {
    let a = submission("s", "i", "r", "u1", 0, "synthetic text");
    let mut b = a.clone();
    b.session.title = Some("changed title".into());
    assert_ne!(validate_submission(&a).unwrap().content_hash,
               validate_submission(&b).unwrap().content_hash);
}
```

追加显式用例：api_version=2、空 ID、20,001 条消息、257 个 block、16 MiB+1、嵌套元数据超过 serde 默认递归界限、body 自报 owner_id。Windows/POSIX source_path 仅作元数据，不触发任何文件读取；附件 URL 不下载。`ValidatedSnapshot` 为测试实现脱敏 Debug，禁止直接打印正文。

- [ ] **Step 2:** `cargo test -p aiks-core --test service_contracts`，确认 RED 指向尚不存在的契约或校验，不接受网络故障为 RED。
- [ ] **Step 3:** 实现上述 DTO、稳定错误码、排序哈希与有界校验；保留 Unknown blocks，不猜测角色和工具结果。
- [ ] **Step 4:** 同命令 GREEN；运行 `cargo test -p aiks-core model` 验证旧模型兼容。
- [ ] **Step 5:** 提交 `feat(core): define versioned snapshot ingestion contracts`，只勾选看过输出的步骤。

## Task 2: 服务实例、空间及独占写入锁，安全追加 migration

**Files:** Create `migrations/012_service_snapshots.sql`、`storage/ownership.rs`、`service/repo.rs`、`tests/service_storage.rs`；Modify `storage/{mod,db}.rs`、Core Cargo。

**Interfaces:** `BusinessDbLease::acquire(db_path: &Path) -> anyhow::Result<BusinessDbLease>`；`StateDb::open_exclusive(path: &Path) -> anyhow::Result<StateDb>` 保存 lease 至关闭；`ServiceStore::open(Arc<StateDb>) -> Result<ServiceStore, ServiceError>`；`ServiceStore::local_context() -> LocalContext`。`LocalContext` 包含 instance_id、principal_id、space_id，字段对外只读，不能从 HTTP JSON 直接反序列化。

采用独立映射表，不改已发布 source_session/knowledge ID 或 UNIQUE 约束。migration 负责结构，服务首次事务用 UUID 插入单实例/单本地主体/单私有空间，禁止在 SQL 中写固定团队 owner。

```sql
CREATE TABLE IF NOT EXISTS service_instance (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    instance_id TEXT NOT NULL UNIQUE,
    principal_id TEXT NOT NULL,
    personal_space_id TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS service_source_registration (
    id TEXT PRIMARY KEY,
    principal_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    source TEXT NOT NULL,
    registration_key TEXT NOT NULL,
    UNIQUE(principal_id, space_id, source, registration_key)
);
CREATE TABLE IF NOT EXISTS service_session_binding (
    session_id INTEGER PRIMARY KEY REFERENCES source_session(id),
    principal_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    registration_id TEXT NOT NULL REFERENCES service_source_registration(id),
    upstream_id TEXT NOT NULL,
    current_revision INTEGER NOT NULL DEFAULT 0 CHECK(current_revision >= 0),
    UNIQUE(space_id, registration_id, upstream_id)
);
CREATE TABLE IF NOT EXISTS service_session_snapshot (
    id TEXT PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES service_session_binding(session_id),
    revision INTEGER NOT NULL CHECK(revision > 0),
    parser_version TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    canonical_json BLOB NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(session_id, revision)
);
CREATE TABLE IF NOT EXISTS service_job_input (
    pipeline_run_id TEXT PRIMARY KEY REFERENCES pipeline_run(id),
    snapshot_id TEXT NOT NULL REFERENCES service_session_snapshot(id),
    durable_job_id TEXT NOT NULL UNIQUE REFERENCES pipeline_job(id)
);
CREATE TABLE IF NOT EXISTS service_ingest_receipt (
    id TEXT PRIMARY KEY,
    principal_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    registration_id TEXT NOT NULL REFERENCES service_source_registration(id),
    upstream_id TEXT NOT NULL,
    submission_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    snapshot_id TEXT NOT NULL REFERENCES service_session_snapshot(id),
    pipeline_run_id TEXT NOT NULL REFERENCES pipeline_run(id),
    durable_job_id TEXT NOT NULL REFERENCES pipeline_job(id),
    created_at TEXT NOT NULL,
    UNIQUE(principal_id, space_id, registration_id, upstream_id, submission_id)
);
```

- [ ] **Step 1:** 写测试，反复 open 保持 instance_id；旧库原 ID/knowledge/siyuan_doc_id/sync_target 不变；外键启用；未获得 lease 前不运行任何迁移。

```rust
#[test]
fn only_one_business_writer_can_open_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let first = StateDb::open_exclusive(&path).unwrap();
    assert!(StateDb::open_exclusive(&path).is_err());
    drop(first);
    assert!(StateDb::open_exclusive(&path).is_ok());
}
```

- [ ] **Step 2:** `cargo test -p aiks-core --test service_storage`，确认 RED。
- [ ] **Step 3:** 引入 `fs2::FileExt::try_lock_exclusive`，使用规范化数据库父目录下固定 sidecar 锁文件，lease 生命周期覆盖数据库句柄。不得把文件“存在”当锁，不按 PID 单独判断，不删除别人锁文件；显式拒绝符号链接数据库路径歧义。现有非独占 `StateDb::open` 仅用于内部测试/兼容调用，Task 9 将所有产品运行入口切到独占打开。migration 仅追加，失败事务回滚。
- [ ] **Step 4:** GREEN，并验证两个独立子进程的争锁、Windows 大小写/规范路径别名和崩溃释放。fs2 属协作锁，不能宣称阻止任意第三方 SQLite 程序。
- [ ] **Step 5:** 提交 `feat(storage): persist service identities and enforce writer ownership`。

## Task 3: 快照、回执和旧持久队列原子接收

**Files:** Modify `service/repo.rs`、`pipeline/{repo,job_repo}.rs`、`storage/repo.rs`；Test `tests/service_storage.rs`。

**Interfaces:** `ServiceStore::register_source(ctx, source, registration_key) -> Result<String, ServiceError>`；`ServiceStore::accept(ctx, &ValidatedSnapshot) -> Result<(SnapshotReceipt, bool), ServiceError>`，bool 表示本次新接收。新增已有 Repo 的 `*_in_tx(&rusqlite::Transaction, ...)` 内部方法，公开老方法仍创建事务后委托同一实现。不能在持有 `db.conn()` 时再调用会二次获取同一 Mutex 的 Repo 方法。

- [ ] **Step 1:** 建真实临时 StateDb 和上述 LocalContext，写如下测试；注册值来自 `register_source`，不可伪造 registration_id 跳过登记。

```rust
let (first, inserted) = store.accept(&ctx, &validated).unwrap();
let (retry, inserted_retry) = store.accept(&ctx, &validated).unwrap();
assert!(inserted);
assert!(!inserted_retry);
assert_eq!(first, retry);
// 再直接查询真实表：snapshot=1、receipt=1、pipeline_run=1、pipeline_job=1。
```

同一 submission_id 改正文/目标/CAS 要 409；另一个 submission_id 携带未变化内容应关联同一当前快照/任务并保存自己的回执，不重复运行模型；先检查幂等回执，再检查 CAS，让响应丢失后的老 revision 重试能够取回原回执。

- [ ] **Step 2:** RED：`cargo test -p aiks-core --test service_storage`。
- [ ] **Step 3:** 在一个 `TransactionBehavior::Immediate` 中依次校验上下文/登记、查回执、校验 expected_revision、写 source_session/binding/snapshot、创建 pipeline_run、enqueue_in_tx、写 job_input/receipt 后 commit。唤醒仅发生在 commit 后，唤醒丢失由现有轮询恢复。

新会话使用服务生成的内部 external_session_id（`svc:` 加随机 UUID）；原始 upstream_id 保留在 binding/不可变快照中。不能拼接未转义输入后假设唯一，更不能靠标题识别同一会话。旧会话显式迁移见 Task 7，保持旧 external ID。

```rust
let mut conn = db.conn();
let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
// 身份、幂等、CAS、快照和现有队列写入都使用 &tx。
// 所有 fallible 写入结束后才提交，不能先提交快照再单独 enqueue。
tx.commit()?;
// 这里只能尝试 capacity-1 wake，回执代表持久接收，不是 READY。
```

SQL 注入失败点采用测试事务/受控 trigger：在 pipeline_job 或 receipt INSERT 时强制 abort，断言 session/binding/snapshot/run/job/receipt 无半提交。没有模型或思源仍应完成接收。

- [ ] **Step 4:** GREEN；并发相同提交只得到一组身份；乱序版本、半写输入及失败后重试均不损坏旧版本。
- [ ] **Step 5:** 提交 `feat(core): atomically ingest snapshots into the durable pipeline`。

## Task 4: Worker 从绑定快照读取，不依赖员工目录

**Files:** Create `pipeline/input.rs`；Modify `pipeline/{worker,job_repo}.rs`；Test `tests/service_worker.rs`。

**Interfaces:** `PipelineInputSource` 包含 `LegacyProviders(Arc<ProviderRegistry>)` 与 `PersistedSnapshots`。`PipelineWorker::start_from_snapshots(db, ai, embedding) -> PipelineWorker`；原 `start`/`start_with_limit` 签名保留。`SnapshotInput::load_for_run(db, run_id) -> Result<(NormalizedSession, RevisionFence)>`，RevisionFence 为 `{ session_id: i64, snapshot_id: String, revision: u32 }`。

新服务通过此入口启动，绝不调用 `build_registry`、`dirs::home_dir` 或 Provider.discover。按 pipeline_run_id 查 `service_job_input`，校验 snapshot 所属 session/registration/source；不存在输入是明确任务错误，禁止回退去读本地路径。

快照路径只是来源描述：原始快照不可变；供内部旧链路使用的副本可以用 binding 中的 canonical source/external ID，保证 `SessionIndexService` 的身份校验仍成立，不删除该校验。

- [ ] **Step 1:** 写测试：从真实 Continue 临时文件经 Provider 取得 NormalizedSession、accept 后删除整个 Provider 根，再启动 snapshot Worker；AI/Embedding 明确关闭；等待对应任务终态，断言 `RAW_ONLY` 且当前真实全文索引能检索唯一短语。

```rust
let worker = PipelineWorker::start_from_snapshots(db.clone(), ai_off(), embedding_off());
// ai_off()/embedding_off() 在 support/service_fixture.rs 中从默认配置构造后显式设置 enabled=false。
// 通过 PipelineJobRepo/PipelineRepo 有界轮询最多 10 秒，不用固定 sleep 假定完成。
let detail = PipelineRepo::new(&db).get_run_detail(&receipt.pipeline_run_id)?.unwrap();
assert_eq!(detail.status, "RAW_ONLY");
```

再写“先停止 Worker、重新打开库、恢复已接收任务”的测试，确认不需要再次接收或客户端存在。snapshot Worker 只 claim 有 service_job_input 的任务；旧未绑定任务在迁移报告中可见，不被快照模式当坏任务消费。

- [ ] **Step 2:** `cargo test -p aiks-core --test service_worker` RED。
- [ ] **Step 3:** 抽出输入加载，不复制清洗/提炼算法；为 claim 增加明确的输入类型过滤，保留现有 lease、重试和同 session 串行约束。增加 Worker 停止信号与可等待的 drain；达到有界关闭时限后不错误标记成功，下次按 lease 恢复。
- [ ] **Step 4:** GREEN；完整原 Provider tests 继续通过；用本地 mock 模型跑一条真实 AiStage 提炼，断言存在真实 KnowledgeItem 而不是只看 RUN 状态。
- [ ] **Step 5:** 提交 `refactor(pipeline): process persisted snapshots without provider discovery`。

## Task 5: 在实际写入事务内阻止旧 revision 覆盖

**Files:** Create `service/revision.rs`；Modify `pipeline/{worker,job_repo,ai_stage,knowledge_repo,session_chunker}.rs`、`indexing/session.rs`；Test `tests/service_revisions.rs`。

**Interfaces:** `RevisionFence::check_in_tx(&Transaction) -> Result<(), SupersededRevision>`。为现有写入增加可选 fence 的内部实现，公开 legacy 包装传 None：`KnowledgeRepo::save_items_guarded`、`SessionIndexService::index_session_guarded`、`save_chunks_guarded`。旧 job 的状态记录仅写自身 run，不能改变当前 session 状态。

- [ ] **Step 1:** 模型测试服务先阻塞 revision 1 请求；接收 revision 2 后再释放旧请求。旧结果必须记为 superseded，而非保存旧知识或以 READY 冒充当前版本。检查知识内容、session FTS、向量 state、current_revision 和 run 状态。
- [ ] **Step 2:** `cargo test -p aiks-core --test service_revisions` RED。
- [ ] **Step 3:** 在同一个实际写事务中执行 fence 与写入，所有入口覆盖：index 的可跳过快捷路径、begin_rebuild、finish_rebuild、chunk 保存、AI 返回后的知识保存、结束状态。仅在调用模型前检查一次不合格。

```rust
let current: u32 = tx.query_row(
    "SELECT current_revision FROM service_session_binding WHERE session_id=?1",
    [self.session_id], |row| row.get(0),
)?;
if current != self.revision {
    return Err(SupersededRevision);
}
// 同一个 &tx 中执行此阶段的实际变更。
```

将正常被新版本替代与失败重试区分；沿用 pipeline_job 的 SUPERSEDED，run 使用经现有 schema/前端状态契约支持的明确 superseded 表示，必须同步测试，不伪装 FAILED 后无限重试。若需要新 status 约束，追加 migration，不改旧文件。

模型 await 期间不持 SQLite 事务或全局网络锁。已发布的旧知识可作为旧版本保留并显示过期，但不能把晚到旧结果标成最新。Task 3 接收新版本要记录派生数据过期，Task 6 查询返回其版本状态，不混用旧 snippet 与新 title。

- [ ] **Step 4:** GREEN；补“模型失败后 retry 时已有新 revision”“进程崩溃旧 lease 恢复”“相同正文只改标题”测试。验证合法新版本仍能完成，不把全部结果一律丢弃当作修复。
- [ ] **Step 5:** 提交 `fix(pipeline): fence derived writes by accepted snapshot revision`。

## Task 6: 可执行的本机服务与有认证的查询 API

**Files:** Create `apps/aiks-service/` 上述所有文件、`service/{runtime,query}.rs`、`apps/aiks-service/tests/{http_contract,lifecycle}.rs`；Modify workspace manifests/lock。

**Interfaces:** `ServiceRuntime::open(config) -> Result<ServiceRuntime>` 持独占 StateDb、ServiceStore、snapshot Worker、ModelService 和现有查询服务；`ServiceRuntime::shutdown().await`；`build_router(Arc<ServiceRuntime>, LocalAuth) -> axum::Router`；`LocalAuth` 保存 token 的 SHA-256、实际 bound authority 和随机启动 nonce，不实现泄露字段的 Debug。

启动参数只有 `--config`、`--listen`、`--bootstrap-stdin`；Token 不进入命令行/URL。客户端生成 32 随机字节，经单个 stdin JSON 帧交给服务；服务 stdout 仅回传 `{api_version, instance_id, boot_nonce, address}`，无 Token。关闭 stdin 不作为关服命令，以便接受任务后客户端退出仍继续处理。显式退出由进程所有者执行有界 shutdown，不能远程任意结束服务。

业务请求须携带 Bearer Token 与 `X-AIKS-Instance-ID`；Host 只接受实际绑定的数值 loopback authority。S1 通过 Tauri 的 Rust reqwest 客户端访问，因此拒绝所有带 Origin 的普通业务请求，不开放 CORS；浏览器接入在 S2 设计认证与 origin allowlist 后再启用。拒绝 Forwarded/X-Forwarded-* 作为身份依据，不接受 token query 参数。

- [ ] **Step 1:** tests 通过 `TcpListener::bind("127.0.0.1:0")` 和真实 router 启动测试服务，使用 reqwest 发请求，不用纯函数替代 HTTP。缺失/错误 Token、恶意 Host/Origin、错误 instance_id、POST `/api/sql/querySql` 必须拒绝；依赖关闭不影响 `/healthz`。

```rust
let response = client.post(format!("{base}/api/v1/session-snapshots"))
    .json(&submission).send().await?;
assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
// 再携有效认证完成同样请求，断言 202 且有可查询的真实回执。
```

测试 helper `RunningService` 在 `tests/support/mod.rs` 内用真实 `ServiceRuntime::open` 和 router；字段 `base: String, token: String, instance_id: String`；`stop(self).await` 发送 oneshot 并 join；不自己模拟收到快照后造成功 JSON。

- [ ] **Step 2:** `cargo test -p aiks-service` RED；基础 crate manifest 随这个任务加入，不单独提交“只有 healthz”的功能。
- [ ] **Step 3:** 实现白名单 routes、总请求体限制、读取时限、分页限制与安全错误转换。用 Axum 提供服务生命周期；数据库操作放 spawn_blocking，不能持连接 await。核心启动不调用旧 AiksEngine::initialize 来意外创建 Provider/第二个 Worker。

```rust
let listener = tokio::net::TcpListener::bind(config.listen).await?;
let bound = listener.local_addr()?;
let app = build_router(runtime.clone(), local_auth.with_authority(bound));
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal)
    .await?;
runtime.shutdown().await?;
```

查询从本地授权上下文限定 binding；未绑定的 legacy 数据只经 Task 7 迁移后暴露。搜索复用 UnifiedSearchService 的关键词/语义融合与 8 秒预算，不能另写第二套全文扫描。单用户上下文不是允许客户端选择任意 scope；接收、job、knowledge、search 都验证当前实例与唯一空间。任务/查询结果中的内部端点、完整路径和 raw errors 不下发。

思源只读详情与 AI Assist 的 URL 来自管理员配置，禁止跳转到新 host；已发布知识从内容适配器取正文，非规范缓存不可充当最新正文。尚未发布候选显式返回 draft/candidate。AI 关闭返回 `ai_disabled`，不尝试默认公网模型。

- [ ] **Step 4:** GREEN；执行真实二进制测试：空临时 HOME 下不创建工具目录；`--listen 0.0.0.0:0` 和 mode=team 均非零退出；关闭 SiYuan 时会话接收与会话关键词搜索可用，知识正文读取返回 `content_unavailable`。
- [ ] **Step 5:** 提交 `feat(service): expose authenticated local ingestion and query APIs`；提交前 fmt/check，新增依赖只写必要 feature。

## Task 7: 旧库受控接管，不丢旧身份与思源映射

**Files:** Create `service/adoption.rs`；Modify `service/{mod,repo,runtime}.rs`；Test `tests/service_migration.rs`。

**Interfaces:** `AdoptionManifest` 含本地目标 instance_id、旧 session ID 与明确 source/upstream/registration 对应、可用规范快照；`adopt_local_state(&StateDb, &LocalContext, &AdoptionManifest) -> AdoptionReport`。此方法只由本机迁移协调器调用，不设网络管理 API。report 分为 bound、snapshot_ready、source_unavailable、conflict，不把缺源当删除。

- [ ] **Step 1:** 从当前旧 schema 建库，插入真实关联的 source_session、knowledge_item、sync_target/siyuan_doc_id、用户内容基线、pending job；保留文件副本做对照。迁移后断言所有旧主键、source key、思源 ID、正文基线仍相同。
- [ ] **Step 2:** `cargo test -p aiks-core --test service_migration` RED。
- [ ] **Step 3:** 旧 Worker/CLI 停止后备份一致的业务 DB 和思源工作区，再在独占 lease 内采用 legacy ID。禁止按相似标题/内容猜 mapping。已有知识的空间归属建立关联；快照缺失的会话仍可读旧记录但不伪造可重处理状态。旧未绑定 pending job 保留，并报告/受控重建到明确快照；不能丢掉或令新 snapshot worker碰运气读 Provider。

迁移幂等，运行时断电可重试；回滚使用成套备份，不用旧程序直接打开不兼容的新状态。普通连接地址修改不触发 adoption。为 S1 保留显式实验开关，默认 legacy 模式直到本阶段端到端验收通过。

- [ ] **Step 4:** GREEN；测试部分源不存在、只存在已发布知识、再次迁移、故障中断后恢复、错误目标 instance_id。
- [ ] **Step 5:** 提交 `feat(core): adopt personal state without changing canonical identities`。

## Task 8: 采集 outbox 与有界 HTTP 上传

**Files:** Create Desktop `service_client/{mod,transport,collector,outbox}.rs`；Modify Tauri manifest；unit tests 在各模块内部。

**Interfaces:** `ServiceConnection { instance_id, base_url, credential_handle }`；`PendingSubmission { target_instance_id, target_space_id, submission, payload_hash }`；`CollectorOutbox::enqueue`、`next_for(instance_id)`、`record_receipt`；`ServiceClient::{capabilities,register_source,submit_snapshot,get_receipt,get_job,search}`。所有方法与 Task 1/6 DTO 相同，不把 raw `source_path` 当上传内容。

outbox 使用独立的采集数据库，schema 在 `outbox.rs` 内以单独版本管理；不放进服务业务 migration 链，也不共享 StateDb 句柄。queue 行持久保存完整 payload、目标 instance/space、submission_id、retry_at、attempt 和 ack；界面切换只改当前显示连接，不改排队数据。

- [ ] **Step 1:** 写测试，A 服务响应丢失后重新发送获得同 receipt；切到 B 时不能发送 A 队列；同一个 URL 返回不同 instance_id 时停止上传并提示目标已变化。

```rust
outbox.enqueue(&pending_for_a)?;
assert!(outbox.next_for("service-b")?.is_none());
let retried = outbox.next_for("service-a")?.unwrap();
assert_eq!(retried.submission.submission_id, original_submission_id);
assert_eq!(retried.payload_hash, original_payload_hash);
```

- [ ] **Step 2:** `cargo test -p aiks-desktop service_client` RED（执行前核对 Tauri Cargo package 名；若不是 aiks-desktop，用实际 package 名，不改产品名）。
- [ ] **Step 3:** Collector 仍用 build_registry/选定 source，只向显式目标登记并发送；发送前完成排除/脱敏、complete 检查。阻断私密信息策略不能用正则保证绝对无泄漏。429/503/断线指数退避并保留队列；409 不盲目把 expected_revision 改为最新重发，先取回执和头版本，防止旧内容逆序覆盖。压缩/分片先不提供，超限明确报告，不截断历史。
- [ ] **Step 4:** GREEN；最多一个 in-flight 提交/同会话，其余目标独立；重复扫描相同内容不追加无限 outbox；暂停/禁用采集不删除已接收数据。
- [ ] **Step 5:** 提交 `feat(desktop): queue immutable uploads by service identity`。

## Task 9: 首条 Desktop HTTP 链路、运行所有权与关闭语义

**Files:** Create `service_client/supervisor.rs`、前述 TS service/platform/status 页面与 tests；Modify `lifecycle.rs`、`app_state.rs`、`lib.rs`、API client/index/types、App.tsx、CLI main。

**Interfaces:** 仅新增受控 Tauri actions：`service_status`、`service_collect_selected`、`service_get_receipt`、`service_get_job`、`service_search`。前端不能传任意 URL、进程命令或思源方法。`DesktopPlatformApi` 承担窗口/目录操作；ServiceApi 承担此阶段业务。ServiceStatusPage 展示实例身份、连接/接收/处理状态，不能把 202 显示成提炼成功。

S1 提供实验配置 `backend.mode = "legacy" | "service_local"`，default=legacy；未知值拒绝。`service_local` 的 supervisor 只启动固定开发/打包资源目录里的目标二进制，不接受页面指定程序路径；S3 再交付各平台安装包与默认迁移。

- [ ] **Step 1:** 前端测试：真实 adapter 合约的 202 显示“已接收/待处理”；任务失败显示错误；搜索空结果与连接失败不同；service 模式未实现的知识写动作显示不可用，不调旧命令。无 Tauri 且未显式 mock 的生产环境不能返回 mock 成功。
- [ ] **Step 2:** 在 `apps/aiks-desktop` 执行 `npm test -- --run` RED；运行 supervisor Rust tests RED。
- [ ] **Step 3:** legacy 与 service_local 初始化明确互斥。service_local 分支不能调用 AiksEngine::initialize、旧后台同步任务或旧 Worker；CLI 的直接业务模式及 legacy Desktop 都先获得 Task 2 lease，杜绝与 Service 双写。旧 CLI 在 Service 运行时清晰拒绝，不悄悄绕过 lease。

supervisor stdin 传一次 bootstrap secret、核对 stdout api_version/instance_id/nonce 后才连接；初始能力查询也带凭据。端口占用由 OS 临时端口解决，不连接占位服务。保留最后 instance_id，在下次有意连接新实例前需显式确认队列处理方式。

关闭窗口入托盘保留服务与采集。彻底退出先停止采集/接收新请求，再有界 drain 本地自有服务；未完成任务保留并在重启后恢复。远程连接不发送 shutdown、不 kill 非子进程；独立 Service 由独立所有者运行时，关闭客户端不会停止其后台任务。测试应分别覆盖独立服务和随 Desktop 托管服务，不能混淆“任务不依赖客户端”与“应用退出也永远不关本地进程”。

- [ ] **Step 4:** GREEN，`npm run build`；服务 restart、端口陌生进程、双开 Desktop、进入托盘/彻底退出分别测试。恢复旧模式必须先完全停止 Service 再打开业务库，不能同一进程里放两个活跃引擎。
- [ ] **Step 5:** 提交 `feat(desktop): route the first personal workflow through local service`。

## Task 10: 无客户端文件的端到端证明与交付记录

**Files:** Create `apps/aiks-service/tests/offline_flow.rs`、`docs/implementation/aiks-service-s1.md`；Modify `.github/workflows/ci.yml`；按实际实现更新 README/AGENTS，不改写旧 Provider 支持范围。

- [ ] **Step 1:** 集成测试通过真实二进制/临时目录完成：bootstrap → 来源登记 → 上传 Continue 合成快照 → 客户端退出并移除源文件 → Worker 提炼（回环 mock AI）→ 持久 KnowledgeItem → 会话搜索；再使用现有 publisher 和测试 SiYuan 内容服务走发布/知识索引/查询，验证 Core 未分叉。HTTP 文档写 route 仍不存在，不以测试内部发布声明 S2 已完成。
- [ ] **Step 2:** 同一 payload 在两个独立服务实例走相同业务 API，分别验证实例隔离；S1 都在 loopback，禁止称为远程团队验收。
- [ ] **Step 3:** 服务停机重启、响应丢失、版本乱序、模型禁用/超时、SiYuan不可用、旧库源缺失、只读目录和业务库锁冲突运行完整套件。外部网络请求使用拦截或不可达代理作为负测，允许所需 loopback；不只将 AI enabled=false 就声称离线 AI 验收。
- [ ] **Step 4:** 执行并保存不可变 SHA 的结果：

```bash
cargo fmt --all --check
cargo test -p aiks-core
cargo test -p aiks-service
cargo check -p aiks-cli
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/aiks-desktop
npm ci
npm test -- --run
npm run build
```

CI 沿用现有 Linux Tauri 依赖和 CI-only 思源资源占位；另外在 Windows 真正执行 service_storage、service_worker、service_revisions、service_migration、HTTP/lifecycle 合约测试，不只 cargo check。新增 CI 无写分支权限、不用 workflow 自动生成提交修复格式、不在代码里削弱断言换绿灯。

- [ ] **Step 5:** 记录成功/失败、验证 SHA、未覆盖项；文档列出 S1 route/安全限制、legacy 回滚方式和后续 S2—S4，不把只通过 mock 的模型/思源测试写成真机验收。不自动合并 main 或发布安装包。
- [ ] **Step 6:** 提交 `test(service): verify durable offline ingestion and desktop handoff`，进行整分支 diff 自检；确无独立 reviewer 环境时如实记录，不虚构代理审查。

## 3. 自检与执行记录规则

开始每个任务前再次核对分支 HEAD，发现别人更新先读差异，不 force push。一个任务一个可审阅提交；格式检查先于推送，测试 RED/GREEN 记录实际原因和输出。失败先定位，不通过放宽网络范围、关闭权限、伪造成功、返回空数组或修改模型默认值解决。

本计划新增定义与已有接口边界：`NormalizedSession`、`SourceKind`、`StateDb`、`PipelineJob/Repo`、`PipelineRepo`、`KnowledgeRepo`、`SessionIndexService`、`ModelService`、`UnifiedSearchService` 复用现有实现。ServiceStore/LocalContext/ValidatedSnapshot/RevisionFence/ServiceRuntime/LocalAuth/RunningService/CollectorOutbox/ServiceClient/AdoptionManifest 分别在 Tasks 1—9 定义；测试支撑只负责启动真实实现与创建合成输入。

规格覆盖复核：设计 §§1—5、9、11 的 S1 约束由 Tasks 1—9 落地；§6 的团队授权由 S2 承担，S1 本地能力与默认拒绝由 Task 6 落地；§7 内部思源原则在所有 S1 HTTP 路由强制保持，受控写入/编辑和 §8 outbox 属 S2；§10 显式共享属 S2，连接队列保护由 Task 8 完成；§12 RAG 属 S4；§§13—14 的分段/验证由 Task 10 与本计划范围表约束。没有将 S2—S4 当作本计划已交付内容。

依赖原始文档：Axum 0.8 `serve/with_graceful_shutdown`、fs2 0.4 `FileExt` 官方 crate 文档。版本已核对为可用系列，不表示依赖与本仓库已编译成功；以实施产生的 Cargo.lock、实际 CI 和平台测试为准。

```text
https://docs.rs/axum/0.8.9/axum/serve/struct.Serve.html
https://docs.rs/fs2/0.4.3/fs2/trait.FileExt.html
```
