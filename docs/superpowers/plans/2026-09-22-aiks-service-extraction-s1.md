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

本计划覆盖设计 S1：可执行服务、接收/持久快照、真实 Worker、个人查询接口、首条 Desktop HTTP 链路及受控升级。先完成 Tasks 1—6 的无界面闭环，再完成 Tasks 7—10 的个人版接线与回归；同一分支内分批提交，不把第一批称为整个 S1 完成。

后续目标不删减，各自形成独立实施计划：

| 设计范围 | 交付归属 | S1 必须留下的边界 |
| --- | --- | --- |
| 主体/实例/空间上下文 | S1 本地唯一主体和私有空间；S2 多人 | DTO 不允许自报 owner/role；业务操作使用受信上下文 |
| 项目/部门角色、跨成员授权、审计 | S2 | team=false；外部地址启动失败；不开放共享写入 |
| 内部思源文档/块/资产写入与 outbox | S2 | S1 无通用代理、原生思源 UI、直接写入或任意 URL 下载 |
| 完整内容变更生命周期 | S2；S1 只读端校验版本 | 不依赖 Tauri 事件作服务事实源，不用失效缓存冒充现行正文 |
| 全部桌面业务迁移、sidecar 发布包、远程部署 | S3；S1 开发用本机启动和首条链路 | 只关闭自己启动的进程；不发布未验收的离线安装包 |
| 权限内 RAG、引用、追问 | S4 | 复用 Service/Core 搜索模型，不用 AI Assist 假装问答 |
| 显式共享、披露检查与撤销 | S2 | 本地历史不自动上传；队列绑定实例/来源/空间 |

S1 的知识读取、查询、AI Assist 复用现有 Core；文档创建/编辑/发布 API 在 S2 的持久内容意图与授权一起完成。S1 服务界面明确禁用尚未迁移的写入口，不能偷偷回退到直接写本地业务库。完整旧版界面作为独立 legacy 模式保留，不与 Service 同时写同一库。

## 1. 文件与职责

新建：

```text
crates/aiks-core/src/service/mod.rs            服务业务导出；不依赖 HTTP/Tauri
crates/aiks-core/src/service/contracts.rs      版本化 DTO、能力与稳定错误码
crates/aiks-core/src/service/validation.rs     结构、完整性、大小、指纹
crates/aiks-core/src/service/repo.rs           主体/来源登记、快照/回执事务
crates/aiks-core/src/service/revision.rs       当前修订与事务内写入保护
crates/aiks-core/src/service/runtime.rs        无 Provider 业务启动、停止
crates/aiks-core/src/service/query.rs          本地授权后的读取和搜索
crates/aiks-core/src/service/adoption.rs       旧个人库显式接管
crates/aiks-core/src/storage/ownership.rs      所有运行入口共同遵守的单写入者锁
crates/aiks-core/src/pipeline/input.rs         legacy Provider / 快照加载边界
crates/aiks-core/migrations/012_service_snapshots.sql
apps/aiks-service/Cargo.toml
apps/aiks-service/src/{main,lib,config,bootstrap,auth,error,routes}.rs
apps/aiks-service/tests/{http_contract,lifecycle,offline_flow}.rs
apps/aiks-service/tests/support/mod.rs
crates/aiks-core/tests/service_{contracts,storage,worker,revisions,migration}.rs
crates/aiks-core/tests/support/service_fixture.rs
apps/aiks-desktop/src-tauri/src/service_client/{mod,transport,supervisor,collector,outbox}.rs
apps/aiks-desktop/src/api/{service,desktop-platform}.ts
apps/aiks-desktop/src/api/service.test.ts
apps/aiks-desktop/src/pages/{ServiceStatusPage.tsx,ServiceStatusPage.test.tsx}
docs/implementation/aiks-service-s1.md
```

定点修改：workspace/Core/Tauri Cargo 与 lock；Core lib、storage/mod/db/repo、pipeline/worker/job_repo/repo/ai_stage/knowledge_repo/session_chunker、indexing/session；CLI main；Desktop lifecycle/app_state/lib、api/index/client/types、App.tsx；现有 CI。禁止为了目录整洁重写 Provider、全文检索或 AI 算法。

## 2. HTTP 契约与预算

前缀 `/api/v1`，仅接受 `127.0.0.1` 或 `::1` 数值监听地址，默认 `127.0.0.1:0`。mode 只能为 personal；team/非 loopback 配置返回错误，不自动降级。公开 /healthz 也不例外扩大监听范围。

业务 ID 输出字符串；revision 是 0..u32::MAX 整数，0 表示尚无版本。expected_revision 使用 CAS，不根据客户端时钟判顺序；耗尽返回 revision_exhausted。

请求体 16 MiB，最多 20,000 条消息、每条最多 256 个 block，元数据和所有 block 仍受总字节预算限制。拒绝 Content-Encoding 压缩；bootstrap 帧最多 4 KiB；分页默认30/最大100；HTTP读取/上传时限30秒，模型长任务只排队；搜索保留现有8秒查询向量预算。S1 进度使用1秒轮询，不新增第二个队列。

| 方法/路由 | 语义 |
| --- | --- |
| GET /healthz | 仅 status=ok，无路径、Token 或诊断 |
| GET /api/v1/capabilities | 认证后返回 instance_id、api_version=1、本地 space_id；team/content_write/rag=false |
| POST /api/v1/source-registrations | 受信上下文登记来源实例；客户端 registration_key 幂等 |
| POST /api/v1/session-snapshots | 原子保存 revision/job/receipt；新接收202，重复回执200 |
| GET /api/v1/receipts/{receipt_id} | 回执与任务身份；接收不等于提炼成功 |
| GET /api/v1/sessions、/sessions/{session_id} | 分页元数据/受控快照详情 |
| GET /api/v1/jobs/{job_id} | 状态、阶段、脱敏错误 |
| POST /api/v1/search | 复用 UnifiedSearch；corpus/source/project 是业务筛选而非授权 |
| GET /api/v1/knowledge、/knowledge/{knowledge_id} | 知识身份与规范正文；未发布候选明确标记 |
| POST /api/v1/knowledge/{knowledge_id}/assist | 取规范正文后调用既有 AI Assist，不写正文 |

无 /proxy/*、原生 SQL/file API、任意 upstream/path/url 参数、远程关机、原生思源页面或全库导出。未知路由404，不转发。401=认证失败；未知/无权资源均404；版本/幂等冲突409；超限413；不完整422；依赖不可用503。错误为 `{error:{code,request_id,message}}`，message 使用固定安全文案，不直接回传 anyhow/SQL/reqwest 原始错误。

## Task 1: 固定快照协议和校验

**Files:** `service/{mod,contracts,validation}.rs`、`tests/service_contracts.rs`、`tests/support/service_fixture.rs`、Core lib。

**Interfaces:** 复用 NormalizedSession/SourceKind；新增结构如下。ServiceError 使用 thiserror 枚举，包含 InvalidInput、UnsupportedVersion、IncompleteSnapshot、TooLarge、Unauthorized、NotFound、Conflict、Unavailable、Internal；每项携安全 code，原始内部错误仅在脱敏日志中保存。`code(&self) -> &'static str` 提供稳定判断。

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
```

request_hash 覆盖除 submission_id 外的整个提交；content_hash 覆盖 canonical session+parser_version。JSON object 递归按键排序，数组不排序，之后 SHA-256；不能依赖 HashMap 顺序。只改标题/parser也产生不同hash。ValidatedSnapshot 自定义 Debug 不打印正文或元数据。

- [ ] **Step 1 / RED test:** 共享 `fixture_session(text)` 用现有 Rust 字段构造 Continue/synthetic-1、一条User/Text、空metadata/其它Option=None；`submission(space,instance,registration,id,expected,text)` 构造完整 DTO。定义 `ai_off`、`embedding_off` helper，显式设置 enabled=false，不依赖默认网络配置。

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

还覆盖api_version=2、空ID、20,001消息、257blocks、16MiB+1、过深嵌套、body自报owner_id、metadata键顺序变化。source_path/附件URL不触发读取或下载。

- [ ] **Step 2:** `cargo test -p aiks-core --test service_contracts`，确认RED来自契约/行为，不是网络。
- [ ] **Step 3:** 实现DTO、排序hash和预算校验，保留Unknown，不改老模型序列化与source key。
- [ ] **Step 4:** 同命令GREEN，`cargo test -p aiks-core model`。
- [ ] **Step 5:** commit `feat(core): define versioned snapshot ingestion contracts`。

## Task 2: 服务身份、正式migration与单写入者

**Files:** `012_service_snapshots.sql`、`storage/ownership.rs`、`service/repo.rs`、storage mod/db、Core Cargo、`tests/service_storage.rs`。

**Interfaces:** `BusinessDbLease::acquire(&Path) -> anyhow::Result<BusinessDbLease>`；`StateDb::open_exclusive(&Path) -> anyhow::Result<StateDb>`持lease；`ServiceStore::open(Arc<StateDb>) -> Result<ServiceStore,ServiceError>`；`local_context() -> LocalContext`。LocalContext保存instance_id/principal_id/space_id，仅服务内部创建，不从HTTP反序列化。

追加独立映射表，不能改旧ID和source_session的唯一约束。首次服务事务生成UUID身份；migration只建结构：

```sql
CREATE TABLE IF NOT EXISTS service_instance (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 instance_id TEXT NOT NULL UNIQUE,
 principal_id TEXT NOT NULL,
 personal_space_id TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS service_source_registration (
 id TEXT PRIMARY KEY, principal_id TEXT NOT NULL, space_id TEXT NOT NULL,
 source TEXT NOT NULL, registration_key TEXT NOT NULL,
 UNIQUE(principal_id,space_id,source,registration_key)
);
CREATE TABLE IF NOT EXISTS service_session_binding (
 session_id INTEGER PRIMARY KEY REFERENCES source_session(id),
 principal_id TEXT NOT NULL, space_id TEXT NOT NULL,
 registration_id TEXT NOT NULL REFERENCES service_source_registration(id),
 upstream_id TEXT NOT NULL,
 current_revision INTEGER NOT NULL DEFAULT 0 CHECK(current_revision>=0),
 UNIQUE(space_id,registration_id,upstream_id)
);
CREATE TABLE IF NOT EXISTS service_session_snapshot (
 id TEXT PRIMARY KEY,
 session_id INTEGER NOT NULL REFERENCES service_session_binding(session_id),
 revision INTEGER NOT NULL CHECK(revision>0), parser_version TEXT NOT NULL,
 content_hash TEXT NOT NULL, canonical_json BLOB NOT NULL, created_at TEXT NOT NULL,
 UNIQUE(session_id,revision)
);
CREATE TABLE IF NOT EXISTS service_job_input (
 pipeline_run_id TEXT PRIMARY KEY REFERENCES pipeline_run(id),
 snapshot_id TEXT NOT NULL REFERENCES service_session_snapshot(id),
 durable_job_id TEXT NOT NULL UNIQUE REFERENCES pipeline_job(id)
);
CREATE TABLE IF NOT EXISTS service_ingest_receipt (
 id TEXT PRIMARY KEY, principal_id TEXT NOT NULL, space_id TEXT NOT NULL,
 registration_id TEXT NOT NULL REFERENCES service_source_registration(id),
 upstream_id TEXT NOT NULL, submission_id TEXT NOT NULL, request_hash TEXT NOT NULL,
 snapshot_id TEXT NOT NULL REFERENCES service_session_snapshot(id),
 pipeline_run_id TEXT NOT NULL REFERENCES pipeline_run(id),
 durable_job_id TEXT NOT NULL REFERENCES pipeline_job(id), created_at TEXT NOT NULL,
 UNIQUE(principal_id,space_id,registration_id,upstream_id,submission_id)
);
```

- [ ] **Step 1:** 测试反复打开instance_id不变，旧session/knowledge/siyuan_doc_id/sync_target不变；未获lease不能迁移。

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

- [ ] **Step 2:** `cargo test -p aiks-core --test service_storage` RED。
- [ ] **Step 3:** 规范化数据库父目录，在固定sidecar锁文件上使用fs2 try_lock_exclusive，持文件句柄直到StateDb关闭；拒绝符号链接路径歧义。不以存在/PID代替锁，不删除他人锁文件。migration失败回滚。Task9将legacy Desktop/CLI一同切独占入口；这之前不能部署混合写入。
- [ ] **Step 4:** GREEN；两个独立子进程争锁、Windows路径别名、崩溃释放。fs2是协作锁，不承诺阻止外部任意SQLite程序。
- [ ] **Step 5:** commit `feat(storage): persist service identities and enforce writer ownership`。

## Task 3: 原子接收与幂等回执

**Files:** service/repo、pipeline/repo/job_repo、storage/repo、service_storage tests。

**Interfaces:** `register_source(ctx,source,registration_key) -> Result<String,ServiceError>`；`accept(ctx,&ValidatedSnapshot) -> Result<(SnapshotReceipt,bool),ServiceError>`；bool=本次新接收。新增已有Repo的事务内方法，旧公开方法仍创建事务后委托同一实现。具体边界为 `PipelineJobRepo::enqueue_in_tx(&Transaction,&PipelineJob) -> anyhow::Result<EnqueueResult>`，run/session创建同样采用传入事务。不得持db.conn后再次锁同一个连接。

- [ ] **Step 1:** 真实临时库先登记来源，再accept：

```rust
let (first, inserted) = store.accept(&ctx, &validated).unwrap();
let (retry, inserted_retry) = store.accept(&ctx, &validated).unwrap();
assert!(inserted);
assert!(!inserted_retry);
assert_eq!(first, retry);
// 直接查真实表，snapshot/receipt/run/job各一条。
```

同submission_id改正文/CAS要409；新submission_id但内容不变关联当前快照和任务、单独保存回执，不重复调用模型。先查回执再查CAS，允许响应丢失后的历史revision重取原回执。

- [ ] **Step 2:** service_storage RED。
- [ ] **Step 3:** 单IMMEDIATE事务完成上下文/登记校验、回执、CAS、source_session/binding/snapshot、pipeline_run、enqueue_in_tx、job_input/receipt；最后commit，再尝试capacity-1 wake，丢唤醒由旧轮询恢复。

新记录内部external_session_id=`svc:`+UUID，upstream_id在binding和快照保留；不拼接未转义用户数据猜唯一。旧记录由Task7显式采用，不改旧external ID。

```rust
let mut conn = db.conn();
let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
// 身份、快照和队列写入统一调用 *_in_tx(&tx, ...)。
// 任何失败在commit前返回，不能先写快照后另开事务enqueue。
tx.commit()?;
```

通过测试trigger在job或receipt插入时故意abort，核验所有新行均回滚；思源/模型关闭不影响接收。

- [ ] **Step 4:** GREEN，含并发重复、乱序、半写和失败重试；202仅表示持久接收。
- [ ] **Step 5:** commit `feat(core): atomically ingest snapshots into the durable pipeline`。

## Task 4: 快照驱动既有Worker

**Files:** pipeline/input、worker/job_repo、service_worker tests。

**Interfaces:** `PipelineInputSource::{LegacyProviders(Arc<ProviderRegistry>),PersistedSnapshots}`；`PipelineWorker::start_from_snapshots(db,ai,embedding) -> PipelineWorker`；原start/start_with_limit保留。`SnapshotInput::load_for_run(db,run_id) -> anyhow::Result<(NormalizedSession,RevisionFence)>`，fence字段session_id:i64/snapshot_id:String/revision:u32。

快照模式不build_registry、不扫描HOME。按run_id读取job_input并验证snapshot的会话/来源。缺快照是任务错误，不能回退读取本地路径。传给内部旧逻辑的会话副本使用binding的canonical source/external ID，原快照不可变；保留SessionIndexService的身份校验。

- [ ] **Step 1:** 用真实Continue Provider读临时会话，accept后删除来源目录；启动snapshot Worker，AI/Embedding明确关闭，10秒有界轮询终态，断言RAW_ONLY与真实全文索引唯一短语命中。

```rust
let worker = PipelineWorker::start_from_snapshots(db.clone(), ai_off(), embedding_off());
let detail = PipelineRepo::new(&db).get_run_detail(&receipt.pipeline_run_id)?.unwrap();
// 断言前按真实repo状态有界轮询，不能固定sleep后假设已结束。
assert_eq!(detail.status, "RAW_ONLY");
```

补接收后先关闭、重开库/Worker恢复；snapshot worker仅claim有service_job_input的任务，未迁移旧任务不当作坏输入消费。

- [ ] **Step 2:** `cargo test -p aiks-core --test service_worker` RED。
- [ ] **Step 3:** 抽离输入，不复制清洗/提炼；增加claim输入过滤，保留lease/retry/同session串行。Worker支持可等待停止和有界drain；未完成不记成功，下次按lease恢复。
- [ ] **Step 4:** GREEN；现有Provider回归；回环mock模型走真实AiStage并断言KnowledgeItem存在，不能只看run绿色。
- [ ] **Step 5:** commit `refactor(pipeline): process persisted snapshots without provider discovery`。

## Task 5: 事务内revision fencing

**Files:** service/revision；pipeline/worker/job_repo/ai_stage/knowledge_repo/session_chunker；indexing/session；service_revisions tests。

**Interfaces:** `RevisionFence::check_in_tx(&Transaction) -> anyhow::Result<()>`。增加可选fence内部实现：KnowledgeRepo.save_items_guarded、SessionIndexService.index_session_guarded、save_chunks_guarded；旧包装传None。数据库错误保持真实失败，仅typed SupersededRevision是正常版本替代。

```rust
#[derive(Debug, thiserror::Error)]
#[error("snapshot superseded")]
pub struct SupersededRevision;

impl RevisionFence {
    pub fn check_in_tx(&self, tx: &rusqlite::Transaction<'_>) -> anyhow::Result<()> {
        let current: u32 = tx.query_row(
            "SELECT current_revision FROM service_session_binding WHERE session_id=?1",
            [self.session_id], |row| row.get(0),
        )?;
        if current != self.revision {
            return Err(SupersededRevision.into());
        }
        Ok(())
    }
}
```

- [ ] **Step 1:** 回环模型阻塞revision1；接收revision2后释放旧结果。旧结果不得写当前知识、FTS/向量或READY；查真实表而不只查错误字符串。
- [ ] **Step 2:** `cargo test -p aiks-core --test service_revisions` RED。
- [ ] **Step 3:** 在实际写事务中check后写入，覆盖index shortcut/begin_rebuild/finish_rebuild、chunk保存、AI返回后的知识保存、当前结果状态。模型await期间不持事务/粗全局锁。Worker按downcast_ref识别SupersededRevision，不把数据库错误当版本替代；job使用既有SUPERSEDED，run的表示须同时通过schema和前端契约测试，必要时追加migration。

Task3接受新版本记录派生数据过期；Task6返回数据版本状态，不把旧snippet和新title组合成“最新”。旧已发布版本可保留为过期，但晚到结果不得覆盖现行版本。

- [ ] **Step 4:** GREEN；模型失败后retry遇新revision、崩溃lease恢复、只改标题、合法新版本均能完成。不能无条件丢弃所有结果来通过测试。
- [ ] **Step 5:** commit `fix(pipeline): fence derived writes by accepted snapshot revision`。

## Task 6: 独立本机服务及认证查询

**Files:** apps/aiks-service所有上述文件；service/runtime/query；workspace manifests/lock；http_contract/lifecycle tests。

**Interfaces:** `ServiceRuntime::open(config) -> anyhow::Result<ServiceRuntime>`持独占库、Store、snapshot Worker、ModelService和旧查询服务；`shutdown().await`；`build_router(Arc<ServiceRuntime>,LocalAuth) -> axum::Router`；`LocalAuth::with_authority(SocketAddr) -> LocalAuth`。LocalAuth持Token的SHA-256、bound authority和随机boot nonce，不提供泄露字段的Debug。

启动参数仅config/listen/bootstrap-stdin。客户端生成32随机字节，经stdin单JSON帧传给Service；stdout仅返回api_version/instance_id/boot_nonce/address，无Token。stdin关闭不作为关服命令；独立Service继续处理已接收任务。显式停机由进程所有者触发。

所有业务请求带Bearer和X-AIKS-Instance-ID；Host只接受实际数字loopback authority。S1由Tauri Rust reqwest调用，拒绝所有带Origin的业务请求、不开CORS；浏览器来源在S2认证后另设allowlist。不依赖Forwarded/X-Forwarded-*授权，不接受URL token。

- [ ] **Step 1:** 用真实router和TcpListener临时端口，用reqwest验证缺失/错误Token、恶意Host/Origin、错误instance、原生SQL路径被拒绝。测试辅助RunningService位于tests/support/mod.rs，字段base/token/instance_id，真实创建runtime/router；stop发送oneshot并join，不造成功JSON。

```rust
let denied = client.post(format!("{base}/api/v1/session-snapshots"))
    .json(&input).send().await?;
assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
// 带认证重做同请求，断言202和真实可查询回执。
```

- [ ] **Step 2:** `cargo test -p aiks-service` RED；crate脚手架归本任务，不单独交付只有healthz的“服务完成”。
- [ ] **Step 3:** 实现白名单路由、读取/大小/分页预算、安全错误；数据库spawn_blocking，不持锁await。不能调用旧AiksEngine::initialize意外启动Provider/第二Worker。

```rust
let listener = tokio::net::TcpListener::bind(config.listen).await?;
let bound = listener.local_addr()?;
let app = build_router(runtime.clone(), local_auth.with_authority(bound));
axum::serve(listener, app).with_graceful_shutdown(shutdown_signal).await?;
runtime.shutdown().await?;
```

查询按受信本地主体/空间验证binding，未绑定legacy数据经Task7接管后可见。复用UnifiedSearch与8秒预算，不另写全文扫描。session/job/knowledge/receipts跨实例均不可读。已发布正文只由固定SiYuan适配器读取，拒绝跨host重定向；候选明确draft。AI关闭返回ai_disabled，不请求默认云模型。不暴露思源Token、路径/内部端点及raw错误。

- [ ] **Step 4:** GREEN；二进制在空HOME运行不建工具目录；0.0.0.0监听/mode=team启动非零退出；SiYuan关闭时会话接收/关键词搜索仍可用，规范知识正文返回content_unavailable。
- [ ] **Step 5:** commit `feat(service): expose authenticated local ingestion and query APIs`。

## Task 7: 旧个人库显式接管

**Files:** service/adoption/mod/repo/runtime；service_migration tests。

**Interfaces:** AdoptionManifest包含目标instance_id、旧session ID与确切source/upstream/registration对应、可用规范快照；`adopt_local_state(&StateDb,&LocalContext,&AdoptionManifest) -> Result<AdoptionReport,ServiceError>`。report为bound/snapshot_ready/source_unavailable/conflict，不设网络管理API。

- [ ] **Step 1:** 构造旧schema真实关联数据，含知识、思源映射、基线、pending job；保存对照。迁移后断言所有旧主键、source key、siyuan_doc_id和内容基线一致。

```rust
let before = repo.find_by_source_and_id("continue", "legacy-session")?.unwrap();
let report = adopt_local_state(&db, &ctx, &manifest)?;
let after = repo.find_by_source_and_id("continue", "legacy-session")?.unwrap();
assert_eq!(before.id, after.id);
assert!(report.conflict.is_empty());
```

- [ ] **Step 2:** `cargo test -p aiks-core --test service_migration` RED。
- [ ] **Step 3:** 停旧Worker/CLI、备份一致DB与思源工作区后，在独占lease内采用旧ID。不能按标题猜mapping。已存在知识建立归属关联；缺源记录保留并显示不可重新处理，不制造快照。旧未绑定任务保留并报告/受控转换，不能删除或让新Worker碰运气读取Provider。迁移幂等、失败恢复；回滚用成套备份，切连接不触发adoption。
- [ ] **Step 4:** GREEN；缺源、仅发布知识、再次迁移、中断恢复、错误实例全部覆盖。
- [ ] **Step 5:** commit `feat(core): adopt personal state without changing canonical identities`。

## Task 8: 采集队列与上传目标绑定

**Files:** Tauri service_client/mod/transport/collector/outbox，manifest，模块内tests。

**Interfaces:** ServiceConnection(instance_id,base_url,credential_handle)；PendingSubmission(target_instance_id,target_space_id,submission,payload_hash)；CollectorOutbox.enqueue/next_for/record_receipt；ServiceClient.capabilities/register_source/submit_snapshot/get_receipt/get_job/search对应Task1/6契约。

采集outbox是独立客户端SQLite及独立schema版本，不共享业务StateDb、不加入业务migration。行保存payload、目标实例/空间、submission_id、retry_at、attempt、ack；切换界面不改队列目标。

- [ ] **Step 1:** A响应丢失后重传同回执；切B不能发送A队列；同URL换instance必须暂停。

```rust
outbox.enqueue(&pending_for_a)?;
assert!(outbox.next_for("service-b")?.is_none());
let retried = outbox.next_for("service-a")?.unwrap();
assert_eq!(retried.submission.submission_id, original_submission_id);
assert_eq!(retried.payload_hash, original_payload_hash);
```

- [ ] **Step 2:** `cargo test -p aiks-desktop service_client` RED；已核对Tauri package名为aiks-desktop。
- [ ] **Step 3:** Collector复用Provider/选定来源；排除/脱敏/complete检查在发送前完成，不把脱敏规则宣传为绝对保证。429/503/断线退避保留队列；409先取回执与版本，禁止盲目改expected_revision使旧内容覆盖新内容。每会话最多一个in-flight，重复未变扫描不无限追加。超限明确报错，不截断正文。
- [ ] **Step 4:** GREEN；暂停/关闭来源不删除已接收数据；原始路径/正文URL不作为服务下载目标。
- [ ] **Step 5:** commit `feat(desktop): queue immutable uploads by service identity`。

## Task 9: Desktop首条HTTP链路与生命周期

**Files:** supervisor、TS service/platform/status页面和tests；lifecycle/app_state/lib、api/index/client/types、App.tsx、CLI main。

**Interfaces:** 受控Tauri actions `service_status/service_collect_selected/service_get_receipt/service_get_job/service_search`，不传任意URL/命令；DesktopPlatformApi承载窗口目录，ServiceApi承载本阶段业务；ServiceStatusPage区分连接、接收、排队和处理结果。

实验配置backend.mode仅legacy/service_local，默认legacy，未知值拒绝。supervisor只启动固定开发/资源目录的二进制，不接受页面任意程序路径；S3负责各平台完整安装包与默认迁移。

- [ ] **Step 1:** 前端测试202显示已接收而非成功，任务失败显示错误，搜索零条不同于网络失败；服务模式未完成写动作不回退旧command；无Tauri生产环境不自动mock。

```ts
expect(statusText({ receiptState: "accepted", jobState: "PENDING" })).toBe("已接收，等待处理");
expect(statusText({ receiptState: "accepted", jobState: "FAILED" })).toBe("处理失败");
```

`statusText`是ServiceStatusPage导出的纯展示函数，以上状态必须来源于API适配器而非本地伪造；页面测试同时mock transport验证实际映射。另测supervisor握手、双开和关闭语义。

- [ ] **Step 2:** Desktop `npm test -- --run`与Rust supervisor tests RED。
- [ ] **Step 3:** legacy/service_local互斥；service_local不得初始化旧AiksEngine或旧后台Worker。legacy Desktop与直接业务CLI统一先获取Task2 lease；Service活跃时CLI清晰拒绝，而非并行写库。

stdin传启动凭据，校验stdout版本/实例/nonce，带认证查capabilities；陌生端口不信任。窗口关闭入托盘保留进程；彻底退出停止采集并有界drain自有Service，未完任务保留重启恢复。独立Service不因客户端退出而停；远程连接不shutdown/kill。恢复legacy须完全停止Service后才能打开库。前端状态函数不是业务状态事实源。

- [ ] **Step 4:** GREEN和`npm run build`；重启、陌生进程、双开、托盘/彻底退出、旧模式回退分别验收。不能把任务不依赖客户端误说成本地整个应用退出后进程永远不关闭。
- [ ] **Step 5:** commit `feat(desktop): route the first personal workflow through local service`。

## Task 10: 真实链路与交付门槛

**Files:** apps/aiks-service/tests/offline_flow.rs，docs/implementation/aiks-service-s1.md，现有CI；按实际状态更新README/AGENTS。

- [ ] **Step 1:** 真实二进制临时目录流程：bootstrap→登记→上传Continue合成快照→删除源目录/关闭客户端→真实Worker与回环mock AI→持久KnowledgeItem→会话查询；内部测试调用现有publisher与测试SiYuan走知识发布/索引/搜索，验证Core不分叉。不得将内部测试发布声称为已交付HTTP写API。
- [ ] **Step 2:** 两个独立loopback实例使用相同DTO与处理链，验证实例隔离；这不是远程团队验收。
- [ ] **Step 3:** 服务重启、响应丢失、版本乱序、模型禁用/超时、思源不可用、只读目录/锁冲突/旧源缺失全部覆盖。通过网络拦截阻断外部请求而允许loopback；仅AI关闭不等于本机AI离线验收。
- [ ] **Step 4:** 对不可变HEAD执行：

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

保留现有Linux Tauri依赖/CI-only思源资源占位。Windows实际执行storage/worker/revisions/migration和HTTP/lifecycle，不只check。CI无写分支权限，不增加自动生成补丁提交的脚本，不为绿灯弱化断言或改变模型默认值。

- [ ] **Step 5:** 记录SHA、结果、覆盖范围；S1路由/限制、回滚方式、S2—S4边界清楚；mock和CI不等于真机/大库性能。无独立reviewer环境时如实记录，不虚构代理审查。
- [ ] **Step 6:** commit `test(service): verify durable offline ingestion and desktop handoff`，整分支diff检查，不自动合main或发安装包。

## 3. 自检与执行记录

每项先核对分支HEAD，别人更新先读差异，不force push；每任务一个可审阅交付和真实RED/GREEN，失败必须定位。不能放宽网络/权限、吞错误或伪造成功换取通过。

本计划不改变已有解析器与canonical模型。新类型来源：ValidatedSnapshot/ServiceError/DTO（Task1）；LocalContext/ServiceStore/BusinessDbLease（Task2）；接收事务（Task3）；PipelineInputSource/SnapshotInput/RevisionFence（Task4）；SupersededRevision及guarded写入（Task5）；ServiceRuntime/LocalAuth/RunningService（Task6）；AdoptionManifest/Report（Task7）；CollectorOutbox/ServiceClient（Task8）；UI平台与服务适配器（Task9）。其余ModelService、UnifiedSearchService、各Repo等为基线已有实现。

规格覆盖：§§1—5、9、11的S1部分由Tasks1—9实现；§6团队授权属S2，S1的唯一主体/默认拒绝在Task6；§7内部思源限制全程执行，受控写入与§8内容outbox属S2；§10共享属S2但连接队列保护由Task8实现；§12问答属S4；§§13—14分段与验收由Task10和范围表执行。S2—S4并未被取消，也没有被当成S1已完成。

自检已修正revision guard的错误类型：真实数据库错误通过anyhow传播，只有typed SupersededRevision才可成为正常版本替代。运行所有权必须覆盖legacy入口，不能只保护新服务。已核对桌面crate名。尚未编译或执行文中产品代码/测试，所有checkbox保持未勾选。

依赖依据为官方crate文档；可用系列已核对，但与本仓库的实际兼容性仍以生成Cargo.lock和测试为准：

```text
https://docs.rs/axum/0.8.9/axum/serve/struct.Serve.html
https://docs.rs/fs2/0.4.3/fs2/trait.FileExt.html
```
