# AIKS S2 钉钉登录与单公司知识共享 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在同一套 AIKS Core/Service 上实现单公司钉钉登录、默认私有知识、人员/组织只读分享、创建者受控编辑和可选团队连接，完整保留个人离线版。

**Architecture:** 先建立可信身份/所有权/授权边界，再接登录和组织适配，之后把同一业务 runtime 的所有入口贯通到请求上下文。配置和授权未就绪时不开放团队入口；桌面保留个人 Service，与团队连接并存，不能将团队连接错误回退为个人身份。思源始终为内部内容服务，编辑复用现有 publisher/sink 的安全语义，不另写一套 Pipeline、检索或知识库。

**Tech Stack:** 现有 Rust workspace、Tokio、Axum、reqwest、rusqlite、Serde/TOML、Tauri、React/Vite、Vitest、GitHub Actions；S2 不替换数据库、不增加微服务、不做实时协作文档引擎。新增依赖仅为确有需要的原生凭据存储，需固定 Cargo.lock 并记录审计。

**Spec:** `docs/superpowers/specs/2026-09-23-aiks-team-sharing-dingtalk-design.md`

**Baseline:** `feature/aiks-service-extraction` / `dda5e4cb30f5fe8398388dcd12b1c0c3fab41f24`。用户已审阅设计并要求钉钉参数手动填写；本计划待审阅，所有未勾选事项均不是已实现功能。

## Global Constraints

- “单公司、单个 AIKS Service 写入实例”。
- “第一版只有创建者可以修改正文、管理分享和归档；被分享者只读。”
- “不做多人实时编辑、自动合并、跨企业分享、匿名链接和多租户 SaaS。”
- “个人版继续使用本机 Service / 思源 / 数据目录，不需要钉钉登录”。
- “S2 授权、连接与部署验收通过之前不得放开团队监听。”
- “Client Secret 仅放服务端环境变量或挂载的秘密文件”。
- “每 5 分钟尝试刷新；目录授权信息最长使用 15 分钟”。这两项为本项目默认策略，不是上游保证。
- “S2 首版 permission 只接受 read”。不在 UI 虚设可编辑按钮。
- “分享知识不隐式分享整段原始 AI 会话、处理日志、来源路径或模型提示”。
- “网络等待前后检查可见性及相关授权/内容版本，不跨网络等待持有 SQLite 锁”。
- “首版增加创建者受控编辑/发布的必要接口，不直接开放思源编辑器或 proxy/SQL/file 透传。”
- 正式 migration 只追加 `crates/aiks-core/migrations`，不改变已使用的 001–016；旧数据不猜所有者；所有测试使用临时资料和合成第三方凭据。
- 不修改 `main`，不合并、不发 Release、不迁移/扫描真实用户资料；需追加授权后才做这些操作。

## Review Focus

1. 用户改配置后指向同一个库但换 CorpId/Client ID：拒绝重新绑定，不能把原公司的私有资料暴露给新公司。归 Task 1/2。
2. 同一客户端服务地址不变但登录人改变：排队快照不能换成新用户发送；原身份的回执/排除规则也不得混用。归 Task 9。
3. 在获取正文、模型结果或附件期间撤销分享/停用用户：结果发出前再次鉴权，不能借慢请求泄漏。归 Task 6/7/8。
4. 通讯录授权范围缩小、分页中断或部门含环：不发布残缺快照、不把未扫描用户当离职、不退回全员可见；超过最大陈旧时间拒绝团队访问。归 Task 3/4。
5. 思源已完成写入但客户端未收到结果/服务崩溃：持久意图恢复并核对规范正文，不重复建文档、不覆盖人工编辑。归 Task 8。

## 0. 交付顺序与文件职责

本计划覆盖已批准设计的 A–E，按依赖顺序执行，不为每一小步重新讨论业务范围。
A（Tasks 1–2）交付静态配置检查、身份/ACL 存储；B（3–5）交付组织同步和合成登录；
C（6–8）贯通业务授权、分享和写入；D（9–10）完成桌面连接与分享体验；
E（11）做受控部署与完整验收。A–D 的中间提交不能被宣传为可对外部署版本。

### 已存在并需要复用的代码

- `apps/aiks-service/src/{config,auth,bootstrap,routes,error,lib}.rs`：当前 personal HTTP 入口。
- `crates/aiks-core/src/service/{repo,contracts,ingestion,query,runtime,revision,adoption}.rs`：持久身份、快照及唯一业务入口。
- `crates/aiks-core/src/search/{mod,scope,lexical}.rs`：所有召回路径，范围过滤必须在候选 LIMIT 前。
- `crates/aiks-core/src/pipeline/{knowledge_repo,worker,job_repo,repo}.rs`：知识归属及任务恢复。
- `crates/aiks-core/src/sink/{siyuan,safe_siyuan}.rs`：规范正文与幂等创建适配。
- `crates/aiks-core/src/storage/{db,ownership}.rs`：唯一 migration 入口与跨进程写入所有权。
- `apps/aiks-desktop/src-tauri/src/service_client/`：来源发现、预览、规则及独立上传队列。
- `apps/aiks-desktop/src-tauri/src/{lib,lifecycle}.rs` 和 `lifecycle/`：本机服务生命周期，保留既有行为。
- `apps/aiks-desktop/src/api/service.ts`、`src/pages/service/`：个人知识主界面、引导、同步页。

### 新模块（不是新服务）

- `crates/aiks-core/src/team/{mod,types,repo,policy,directory,login,sessions,shares,content,assets}.rs`：每文件一个职责；不复制本体业务实现。
- `apps/aiks-service/src/team/{mod,config,secrets,dingtalk,middleware,routes}.rs`：运行配置、上游 HTTP 与 Axum 适配。
- `apps/aiks-desktop/src-tauri/src/team_client/{mod,connection,login,credentials}.rs`：HTTPS 连接、浏览器交接、原生凭据生命周期。
- `apps/aiks-desktop/src/api/team.ts`、`src/pages/service/{TeamConnection,ShareDialog}.tsx`：薄 UI；禁止返回服务端 Secret 或原生登录 token。
- `deploy/team/`：Task 11 的真实部署样例。当前只在 `docs/implementation/examples/` 保存待实现的配置契约。

### 核心类型与接口命名

所有 `*_id` 在 wire 上是不可猜测的字符串，业务 ID 内部保持已有类型；不以昵称/邮箱合并用户。
下列都是本计划要求新增的接口，不是当前存在的代码。

```rust
// crates/aiks-core/src/team/types.rs
pub enum Action { Read, Edit, ManageShares, Archive }
pub enum GrantTarget { User(String), Org { id: String, descendants: bool } }
pub struct GrantInput { pub target: GrantTarget } // 本期没有 edit/write 字段值
pub struct UserRecord { pub id: String, pub external_user_id: String,
    pub union_id: String, pub display_name: String, pub active: bool }
pub struct OrgRecord { pub id: String, pub parent_id: Option<String>, pub name: String }
pub struct Membership { pub user_id: String, pub org_id: String }
pub struct DirectorySnapshot { pub scope: Vec<String>, pub users: Vec<UserRecord>,
    pub orgs: Vec<OrgRecord>, pub memberships: Vec<Membership>, pub observed_at: u64 }
// 字段私有，禁止 Deserialize；仅认证 repo 构造，外部只允许 getter。
pub struct TeamContext {
    instance_id: String, company_id: String, user_id: String,
    space_id: String, session_id: String,
}
pub enum RequestContext { Personal(crate::service::LocalContext), Team(TeamContext) }
pub enum TeamError { Unauthorized, NotFound, Forbidden, Conflict,
    DirectoryUnavailable, InvalidInput, ConfigInvalid, Unavailable, Storage }
```

`TeamContext` 提供只读 getter：`instance_id()`、`company_id()`、`user_id()`、`space_id()`、
`session_id()`；每次存储访问仍验证会话未撤销、成员有效、目录新鲜，不能信任旧对象永久有效。
`LocalContext` 对外兼容，原有 personal 方法包装成 `RequestContext::Personal`，不混用认证。

## Task 1：可手工填写的配置与无副作用检查

**Files:** Create `apps/aiks-service/src/team/{mod,config,secrets}.rs`、
`apps/aiks-service/tests/team_config.rs`；Modify `apps/aiks-service/src/{config,lib}.rs`；
Task 11 再修改 `bootstrap.rs` 接真实 team 启动。
样例依据 `docs/implementation/examples/team-service.toml.example` 和 `team-secrets.env.example`。

**Interfaces:**
`TeamSettings: Deserialize + Default` 包含 `enabled`、`public_base_url`、`dingtalk`、`directory`、`sessions`。
`DingTalkSettings` 包含 `enabled/corp_id/client_id/redirect_uri/client_secret_env/client_secret_file`。
`SecretSource` 为 `Environment(String)` 或 `File(PathBuf)`，仅二选一。
`validate_static(&TeamSettings) -> Result<ValidatedTeamSettings, Vec<ConfigIssue>>`；
`resolve_secret(&SecretSource) -> Result<SecretValue, ConfigIssue>`；
`ConfigIssue { field: &'static str, code: &'static str }`，不包含用户值；
`SecretValue` 不实现 Serialize，Debug 固定 `[REDACTED]`。

- [ ] **1. 写失败测试**（不接真实环境，不修改进程全局环境）：

```rust
#[test]
fn empty_settings_do_not_enable_team() {
    let settings = TeamSettings::default();
    assert!(!settings.enabled);
    assert!(!settings.dingtalk.enabled);
    assert_eq!(validate_static(&settings).unwrap_err()[0].code, "team_disabled");
}
#[test]
fn invalid_origin_and_two_secret_sources_are_rejected() {
    let mut settings = TeamSettings::default();
    settings.enabled = true;
    settings.dingtalk.enabled = true;
    settings.public_base_url = "http://example.com".into();
    settings.dingtalk.client_secret_env = "AIKS_DINGTALK_CLIENT_SECRET".into();
    settings.dingtalk.client_secret_file = "/run/secrets/dingtalk".into();
    let issues = validate_static(&settings).unwrap_err();
    assert!(issues.iter().any(|v| v.code == "https_required"));
    assert!(issues.iter().any(|v| v.code == "ambiguous_secret_source"));
}
```

同文件新增表驱动 case：空字段、未知键、userinfo、query/fragment、callback 不同 origin、
路径不是 `/api/v1/auth/dingtalk/callback`、超长秘密、CRLF/控制字符、secret symlink、权限过宽。
Unix mode 与 Windows ACL 分别测试；测试环境依赖注入 `Fn(&str)->Option<String>` 读取变量，禁止测试互相污染。

- [ ] **2. RED:** `cargo test --locked -p aiks-service --test team_config`；检查是缺少目标接口/校验行为，不是缺系统依赖。个人 lifecycle 测试同时保持可运行。
- [ ] **3. 最小实现:** 字段留空、双开关默认 false，区分解析/静态验证/秘密解析。结构里的数字策略采用样例值 300/900/300/900/604800 秒；限制刷新间隔 60–900、陈旧时限不小于刷新间隔且最多 3600 秒；OAuth 事务 60–600 秒，访问令牌 60–3600 秒，刷新 3600–2592000 秒。不允许公司信息为空时测试账号兜底。

```rust
// static check 的关键顺序；issues 收集字段名，不回显值
if !settings.enabled { return Err(vec![ConfigIssue { field: "team.enabled", code: "team_disabled" }]); }
// enabled 后检查 HTTPS、确切 callback、非空 CorpId/Client ID 和唯一 SecretSource。
// resolve_secret 独立：personal 启动路径永远不调用它。
```

- [ ] **4. GREEN/回归:** `cargo test --locked -p aiks-service`、`cargo check --locked -p aiks-cli`、`cargo fmt --all --check`；加入“个人配置完全无钉钉字段/环境变量仍启动”的真实进程测试。配置检查不产生数据库/监听/HTTP 请求。
- [ ] **5. 提交:** `feat(service): define opt-in team settings and private secret references`。此提交不修改允许的 listener，也不把新 mode 加入可运行分支。

## Task 2：持久公司身份、用户/组织模型与默认拒绝 ACL

**Files:** Create `crates/aiks-core/src/team/{mod,types,repo,policy}.rs`、
`crates/aiks-core/migrations/017_team_identity_acl.sql`、`crates/aiks-core/tests/team_policy.rs`；
Modify `crates/aiks-core/src/{lib.rs,storage/db.rs}`、`src/service/repo.rs`。

**Interfaces:** `TeamStore::bind(db: Arc<StateDb>, corp_id: &str, client_id: &str) -> Result<TeamStore,TeamError>`；
内部 `record_owner(tx: &Transaction, knowledge_id: &str, user_id: &str) -> Result<(),TeamError>`；
`policy::allows(action: Action, same_company: bool, member_active: bool, directory_fresh: bool,
owner: bool, directly_shared: bool, org_shared: bool) -> bool` 为纯函数。
Task 4 负责提供合法组织/成员，Task 5 负责提供 `TeamContext`；不得提供客户端指定身份的生产构造函数。

- [ ] **1. 写 ACL 失败测试**：

```rust
#[test]
fn only_owner_writes_and_reader_paths_union() {
    use aiks_core::team::{Action, policy::allows};
    assert!(allows(Action::Read, true, true, true, false, true, false));
    assert!(allows(Action::Read, true, true, true, false, false, true));
    assert!(!allows(Action::Edit, true, true, true, false, true, true));
    assert!(!allows(Action::ManageShares, true, true, true, false, true, true));
    assert!(!allows(Action::Archive, true, true, true, false, true, true));
    assert!(allows(Action::Edit, true, true, true, true, false, false));
    assert!(!allows(Action::Read, false, true, true, true, true, true));
    assert!(!allows(Action::Read, true, false, true, true, true, true));
    assert!(!allows(Action::Read, true, true, false, true, true, true));
}
```

- [ ] **2. RED:** `cargo test --locked -p aiks-core --test team_policy`。
- [ ] **3. 加 migration/实现**：新增 singleton `team_company`（公司 UUID、CorpId、Client ID、generation）、
`team_user`、`identity_provider_binding`、`org_snapshot/org_unit/org_membership/org_closure`、
`team_knowledge_owner`、`document_share_grant`、`team_auth_session/team_login_attempt`、`team_audit_event`。
所有外键/唯一键带 company 或引用同公司复合主键。share.permission SQL CHECK 只接受 read；
`include_descendants` 默认 0。公司库只新建于独立路径：已有 service_instance 个人资料时拒绝自动 team 接管；
重新绑定 CorpId/Client ID 不一致直接失败。原 016 `service_knowledge_binding` 保持个人兼容，不作为团队 ACL 的隐式兜底。

```rust
pub fn allows(action: Action, same_company: bool, member_active: bool,
    directory_fresh: bool, owner: bool, directly_shared: bool, org_shared: bool) -> bool {
    same_company && member_active && directory_fresh &&
        (owner || matches!(action, Action::Read) && (directly_shared || org_shared))
}
```

- [ ] **4. 补存储 GREEN**：同公司同 union_id 登录保留内部 UUID；同名/同手机号不同人不合并；
外部 ID 在另一公司不冲突；17 migration 在有16历史数据时保持所有旧行、ID、内容和未知归属。
SQLite 错误不是正常拒绝结果。运行 Core 全量及 CLI 检查，修改前后逐条比对旧记录。
- [ ] **5. 提交:** `feat(core): add single-company identity and owner-only authorization policy`。

## Task 3：钉钉只读 HTTP 适配与有限请求预算

**Files:** Create `apps/aiks-service/src/team/dingtalk.rs`、`apps/aiks-service/tests/dingtalk_adapter.rs`；
Modify `apps/aiks-service/Cargo.toml`（只复用 workspace 现有依赖，必要时添加 async-trait）；
Create `docs/implementation/dingtalk-api-contracts.md`。

**Interfaces:** `DingTalkClient::new(settings: ValidatedTeamSettings, secret: SecretValue)`；
`exchange_code(code: &str) -> Result<ExternalLogin,TeamError>`；
`directory(scope: &[String]) -> Result<DirectorySnapshot,TeamError>`；
`ExternalLogin { corp_id:String, union_id:String, external_user_id:String, display_name:String }`。
`DingTalkClient` 实现测试可注入的 `IdentityProvider`：上述两个异步方法同签名；测试替身不进入生产配置。

- [ ] **1. 写本地 HTTP fixture 的失败测试**：构造真实 Axum/TcpListener 回环服务，捕获真实 reqwest 请求，
验证授权码只发 POST JSON；302 不跟随；超时/超大响应/非法 JSON/上游字段缺失统一安全错误。
用 fixture 明确返回授权公司不同、无对应员工、active=false、缺应用权限，各请求都不产生 AIKS 会话。

```json
{"clientId":"synthetic-app","clientSecret":"synthetic-secret","code":"synthetic-code","grantType":"authorization_code"}
```

断言测试日志和所有对外 DTO 不包含上述 synthetic-secret、上游响应正文或上游 URL query。
- [ ] **2. RED:** `cargo test --locked -p aiks-service --test dingtalk_adapter`。
- [ ] **3. 按官方契约实现**：授权 origin 固定 `https://login.dingtalk.com`；API origin 固定
`https://api.dingtalk.com` 和有明确只读契约的 `https://oapi.dingtalk.com`，均不跟随 redirect。
确认用户授权码接口 `POST /v1.0/oauth2/userAccessToken`、当前用户信息
`GET /v1.0/contact/users/me`；企业应用 token 按官方
`POST /v1.0/oauth2/{corpId}/token`，JSON 包含 `client_id/client_secret/grant_type=client_credentials`，
不与 userAccessToken 的 camelCase 字段混用。再做 union_id→企业内 user_id 及有效成员检查。
部门/成员接口在实现前从官方 API 页面/SDK 把 method、path、参数、权限码、分页语义逐项保存进 contracts 文档；
不得把 UserAccessToken、企业 app token、UnionId、组织内 UserId 混为一谈。官方不支持或权限不足时明确报错。

每请求 10s，总体扫描受限，单响应至多 2MiB、单批人员至多 100、整次至多10000部门/100000成员；
同一 token 刷新 singleflight，有效期提前60s续取。429 做有界指数退避并服从合理 Retry-After，
三个尝试上限，调用不持有数据库锁。用户 refresh token 本期不持久保存，登录结束即释放；AIKS 的续期由 Task5 管理。
- [ ] **4. GREEN/回归:** 验证 fixture 请求路径/头/JSON，而不是只 mock 方法返回；补零权限和超预算停止场景，
完整 Service 测试通过。保存官方页面核对日期及字段，不宣传真实企业已经接通。
- [ ] **5. 提交:** `feat(auth): implement bounded read-only DingTalk identity adapter`。

## Task 4：组织快照、成员调动与停用失效

**Files:** Create `crates/aiks-core/src/team/directory.rs`、`crates/aiks-core/tests/team_directory.rs`；
Modify `team/repo.rs`；Create `apps/aiks-service/src/team/directory_worker.rs`。

**Interfaces:** `TeamStore::publish_directory(snapshot: DirectorySnapshot, now: u64) -> Result<u64,TeamError>`；
`TeamStore::directory_is_fresh(now:u64,max_age:u64)->Result<bool,TeamError>`；
`TeamStore::mark_member_inactive(user_id:&str,now:u64)->Result<(),TeamError>`；
内部审计事件与会话撤销同一事务，不能用公开 endpoint 接受“我是管理员”的布尔值。

- [ ] **1. RED 测试数据**：公司C，部门研发R含后端B；用户A在R、用户B在B、用户C同时在R/B。
初次发布 generation1：直接R授权只包含A/C；include_descendants=true 才包含B。
缺少最后一页、部门环 `R→B→R`、自引用、无父节点、重复且不一致 user_id 返回错误且 generation 不变。

```rust
#[test]
fn fresh_window_is_bounded() {
    use aiks_core::{storage::StateDb, team::{TeamStore, DirectorySnapshot, OrgRecord}};
    let root = tempfile::tempdir().unwrap();
    let db = std::sync::Arc::new(StateDb::open_exclusive(&root.path().join("team.db")).unwrap());
    let store = TeamStore::bind(db, "synthetic-corp", "synthetic-client").unwrap();
    store.publish_directory(DirectorySnapshot {
        scope: vec!["root".into()], users: vec![], memberships: vec![],
        orgs: vec![OrgRecord { id: "root".into(), parent_id: None, name: "Root".into() }],
        observed_at: 1000,
    }, 1000).unwrap();
    assert!(store.directory_is_fresh(1900, 900).unwrap());
    assert!(!store.directory_is_fresh(1901, 900).unwrap());
    assert!(!store.directory_is_fresh(999, 900).unwrap());
}
```

同一测试目标继续覆盖上述坏快照与成员变更；不能只验证时钟边界就宣称组织同步完成。
- [ ] **2. 运行:** `cargo test --locked -p aiks-core --test team_directory`，确认所有坏快照和成员变更断言为行为 RED。
- [ ] **3. 实现**：先在内存检查 scope、完整性、层级，事务插入新 generation 的组织、人员关系和 closure，
最后切换当前 generation。扫描中断只记录失败，不撤销旧成员。对权威停用事实立即撤销对应会话。
每300s刷新，900s无成功快照 fail-closed；时钟倒退不延長授权。应用范围缩小仅在已确认完整新 scope 下发布。
离职创建者的文档和授权保留，其他人不能编辑/自动接管；管理员不自动获得私有阅读权。
- [ ] **4. GREEN/回归:** 真 SQLite 发布/回滚，读者并发只能看到前后完整版本；断网不放开权限；
成员恢复不复活已撤销 token。自动 worker 只一个，用 cancel token+graceful shutdown。
- [ ] **5. 提交:** `feat(team): publish atomic directory snapshots and revoke inactive memberships`。

## Task 5：一次性 OAuth、AIKS 会话、刷新与退出

**Files:** Create `crates/aiks-core/src/team/{login,sessions}.rs`、`apps/aiks-service/src/team/auth_routes.rs`、
`apps/aiks-service/tests/team_login.rs`；Modify `team/types.rs`、`apps/aiks-service/src/error.rs`。

**Interfaces / HTTP:**
`POST /api/v1/auth/dingtalk/start {verifier_hash}` → `{attempt_id, authorize_url, expires_at}`；
`GET /api/v1/auth/dingtalk/callback?authCode=...&state=...` → 固定完成/取消页，不返回 token；
`POST /api/v1/auth/dingtalk/exchange {attempt_id, verifier}` → `{access_token,refresh_token,expires_in,identity}`；
`POST /api/v1/auth/refresh {refresh_token}` → 同上（旋转）；`POST /api/v1/auth/logout` →204；
`GET /api/v1/me` →安全用户/公司 DTO。
`SessionStore::authenticate(access_token:&str,now:u64)->Result<TeamContext,TeamError>`。
服务业务层从不反序列化 TeamContext。

- [ ] **1. RED，真实 router＋Task3 合成上游**：正确登录→用户身份；错误/缺失/重复state；
授权过期；回调两次；拿另一尝试的verifier；错CorpId；active=false；目录过期；授权取消；
刷新token重放、退出后请求、多个同时交换请求只有一个成功；伪X-User-ID/X-Corp-ID不起作用。

```text
start(verifier_hash=A) → attempt1/state1
callback(state1, valid_code) → complete
exchange(attempt1, verifier=B) → 401
exchange(attempt1, verifier=A) → 200, tokens
exchange(attempt1, verifier=A) → 401
logout(access_token) → 204
GET /api/v1/me with same access_token → 401
```

- [ ] **2. RED:** `cargo test --locked -p aiks-service --test team_login`。
- [ ] **3. 实现**：state、verifier、access/refresh secrets 均32随机字节以上；数据库仅存 token/state/verifier 的 SHA256。
OAuth 事务TTL300s、交换一次性、浏览器会话nonce+state绑定，校验先于token兑换；不从return_url跳转。
授权链接仅固定配置的回调，`scope=openid corpid`，出现corpId必须匹配，同时应用验证当前有效企业员工。
原生客户端拥有verifier，不将其放浏览器URL；浏览器只收到OAuth必要的一次性授权码/state。
AIKS access默认900s、refresh默认7天；refresh串行旋转且记录family，检测已消费令牌重放撤销family。
刷新响应丢失可要求重新登录，不能用旧refresh无限换新；secret 只存哈希，不为方便重试明文保存。
公共登录请求体≤8KiB；每来源地址有界限频与全局最多1024待处理尝试，过期清理有界；
不信任任意 X-Forwarded-For 作为绕过限频依据，标准错误不带上游内容。
- [ ] **4. GREEN/回归:** 并发交易使用真实 SQLite，保存上下文后再停用/撤销必须影响下次 authenticate。
浏览器跨站业务请求不接收 cookie 作业务授权，bearer不落localStorage。所有新错误写 error.rs 的固定码。
- [ ] **5. 提交:** `feat(auth): add single-use DingTalk login and revocable native sessions`。

## Task 6：同一 runtime 的请求上下文与全路径隔离

**Files:** Modify `crates/aiks-core/src/service/{repo,ingestion,query,runtime,contracts}.rs`、
`crates/aiks-core/src/search/{mod,scope,lexical}.rs`、`pipeline/knowledge_repo.rs`；
Create `apps/aiks-service/src/team/middleware.rs`、`apps/aiks-service/tests/team_isolation.rs`。

**Interfaces:** 给现有 runtime 操作增加 `*_for(ctx: &RequestContext, ...)` 内核方法，
原 personal 公共方法只委派给 Personal ctx；团队 routes 只使用 `authenticate` 输出的 Team ctx。
`TeamStore::knowledge_access(ctx:&TeamContext,id:&str,action:Action,now:u64)->Result<(),TeamError>`
和 session/job/receipt owner-access 同一policy。metadata DTO 不包含内部思源 token、路径、全局任务日志。

- [ ] **1. RED 集成**：A/B分别上传相同source/上游session_id，不得合并；A搜不到B会话/标题/摘要，
持B资源ID访问返回404；跨公司即使owner字符串相同也不允许。把未授权匹配记录放在前1000条，
授权结果仍要召回，FTS删表降级和合成向量路径也必须成立。

```text
A uploads source=continue, external_id=same → session-A, private knowledge-A
B uploads source=continue, external_id=same → session-B != session-A
B GET sessions/session-A, jobs/job-A, receipts/receipt-A → 404
B search(session-A body) lexical/fallback/vector → no hit
A disabled while delayed content request running → no response body delivered
```

- [ ] **2. RED:** `cargo test --locked -p aiks-service --test team_isolation`。
- [ ] **3. 实现**：principal采用服务器内部user_id，私有space由服务器注册分配并随会话永久绑定，
客户端不得通过payload覆盖owner/company。源注册/快照幂等身份包含实例、公司、用户、space、source注册。
所有列表/统计/metadata/substring/semantic候选查询使用同一授权SQL片段+绑定参数，候选LIMIT之前生效；
共享知识通过知识ACL召回，但其raw session仍仅owner可读。生成知识同事务继承真实owner，不能归给Worker。
读取跨await检查目录generation、session撤销和grant/content revision，变动返回安全冲突/拒绝；
扩展read epoch触发器覆盖相关team变更，不通过最后一层过滤假装前置授权。
- [ ] **4. GREEN/全回归:** `cargo test --locked -p aiks-core`、`cargo test --locked -p aiks-service`，
保留 S1 `query_isolation/content_boundary/service_revisions` 全部测试，个人不调用团队授权。
- [ ] **5. 提交:** `feat(service): enforce trusted request identity across ingestion and recall`。

## Task 7：只读分享、撤销、组织并集及原始来源保护

**Files:** Create `team/shares.rs`、`apps/aiks-service/src/team/share_routes.rs`、
`apps/aiks-service/tests/team_sharing.rs`；Modify `team/repo.rs`、`service/query.rs`。

**Interfaces:**
`GET /api/v1/knowledge/{id}/shares` 返回 `{grant_version,grants}`，只owner看完整名单；
`PUT /api/v1/knowledge/{id}/shares {expected_grant_version, grants:[{target_type,target_id,include_descendants,permission:"read"}]}`
替换当前名单，空表即私有；需所有者且目标为当前公司有效人员/有效组织。版本不符409。
`TeamStore::replace_grants(ctx,id,expected:u64,grants:&[GrantInput],now:u64)->Result<u64,TeamError>`。

- [ ] **1. RED 序列**：

```text
A GET shares(kA) → version0/empty
A PUT [user B, org R direct-only] expected0 → version1
B reads kA →200; member of R child-only →404
B PUT shares(kA) or PUT content(kA) or DELETE kA →403
A removes B direct grant, B still in R →200
A removes R grant as well →404
A attempts grant.permission=edit →400 (not silently read)
B follows private session/foreign attachment reference from kA →404
```

- [ ] **2. RED:** `cargo test --locked -p aiks-service --test team_sharing`。
- [ ] **3. 实现**：事务检查owner、grant_version、fresh membership、目标scope，去重并规范排序;
一个用户多个部门取并集，include_descendants默认false。无deny例外，无“上级自动看私有”。
同事务写审计/更新generation，撤销后搜索、统计、正文缓存不得继续使用旧授权。共享列表只显示这篇知识，
不把源会话标题/路径/提示词放DTO；无权source仅返回“来源不可访问”。普通转发链接无匿名能力。
- [ ] **4. GREEN/回归:** 数据库失败回滚名单与epoch；grant两窗口冲突；乱序响应不能覆盖新权限；
目录更新后直接grant仍有效、部门grant只按新快照计算。测试读取已下载内容不可回收的语义，不实现伪远程擦除。
- [ ] **5. 提交:** `feat(knowledge): add owner-managed person and department read grants`。

## Task 8：创建者编辑/发布与受控附件

**Files:** Create `team/{content,assets}.rs`、`crates/aiks-core/migrations/018_team_content_intents.sql`、
`apps/aiks-service/src/team/content_routes.rs`、`apps/aiks-service/tests/team_content.rs`；
Modify `storage/db.rs`、既有knowledge发布服务及 `sink/{siyuan,safe_siyuan}.rs`。

**Interfaces:**
`PUT /api/v1/knowledge/{id}/content {base_revision,operation_id,title,markdown}` →202 `{operation_id,state:"pending"}`；
`POST /api/v1/knowledge/{id}/publish {base_revision,operation_id}`；
`GET /api/v1/content-operations/{id}` 仅owner可查；
`GET /api/v1/knowledge/{id}/assets/{asset_id}` 仅资源自身授权后读取；不接受上游路径/URL/SQL。
`ContentOperation { id:String, document_id:String, base_revision:u64, expected_remote_hash:String,
state:Pending|Applying|Verifying|Done|Conflict|Failed }`；schema保存目标hash、结果revision与lease。

- [ ] **1. RED 本机思源fixture**：

```text
owner saves at v1, returns operation1
viewer save at v1 →403
second owner window save at v1 while op1 pending →409
SiYuan commits op1 then times out →service restart→read canonical body→op1 Done v2
retry same operation1 same payload →same outcome; same id different payload →409
SiYuan body is unexpectedly human-edited →Conflict, do not overwrite/recreate
shared knowledge embeds another private doc's asset →404 for reader
```

- [ ] **2. RED:** `cargo test --locked -p aiks-service --test team_content`。
- [ ] **3. 实现**：复用已有KnowledgeRepo/publisher/safe sink，队列保存写入意图；每文档单未完成op约束，
持久lease及事务CAS，出站前重验owner。慢思源不持SQLite锁；验证远端baseline后写入；
写入成功/超时都读取规范正文核对目标hash，再事务提交新revision并安排既有index重建。
写入意图处于 Applying/Verifying 时，该文档规范正文读取返回 `content_pending`，不能让旧metadata拼接新正文；
所有最终提交核对operation ID/lease/current revision，读请求出站前后同样检查未完成操作。
重启恢复不根据失败文本盲目create，不回写已发表的人工作品；正文UTF8预算1MiB，运算ID可重放但不重定义。
source_revision、content_revision、grant_version相互独立，改分享不是重新提炼。

附件通过本资源owner主动上传受限字节建立 `managed_asset`，stable ID映射只由服务写入；
读取前验证doc/asset同属关系和完整可见性。未知文件引用不提升授权；不抓取任意URL，
不开放原生思源页面、raw `.sy`、SQL或任意file端点。附件默认 `Content-Disposition: attachment`、
`nosniff`，不执行HTML/SVG；跨await再次验权。业务错误不得泄漏内部存储路径。
- [ ] **4. GREEN/回归:** 真实进程崩溃恢复fixture、迟到模型不会覆写、跨用户猜op/asset ID、
UTF8/二进制上限、HTML/URL正文安全；真实思源内核上的修改/恢复再单独运行并记录，不以fixture代替。
- [ ] **5. 提交:** `feat(content): support owner-only versioned writes through private SiYuan`。

## Task 9：桌面团队连接、原生凭据与队列身份隔离

**Files:** Create `apps/aiks-desktop/src-tauri/src/team_client/{mod,connection,login,credentials}.rs`、
`apps/aiks-desktop/src-tauri/tests/team_client.rs`；Modify 原 `service_client/{transport,outbox,collector}.rs`、
`src-tauri/src/lib.rs` 和 `lifecycle/` 中模式分派；不替换本机owned supervisor。

**Interfaces:** 原生IPC只暴露 `team_add_connection(origin)`、`team_begin_login(connection_id)`、
`team_finish_login(connection_id)`、`team_logout(connection_id)`、`team_connection_status`。
Webview得到公司名/登录状态，不得到verifier/access/refresh。`TargetIdentity { instance_id,company_id,user_id,space_id }`
写入outbox目标；`CredentialStore::{store,load,delete}(connection_id,user_id)`使用系统安全存储。
Windows为凭据管理器/DPAPI，macOS Keychain、Linux Secret Service；不可用时报错或仅内存会话，禁止明文回退。

- [ ] **1. RED 原生fixture**：同origin先A后B，A已有pending/sending/blocked，B登录后不得claim A；
相同Corp不同serverinstance也不可改投。未登录scan仍本地只读，publish/collect到team要求确认目标身份。
登录URL异origin/非https/带token或userinfo被拒；重定向到未知server不得转发凭据。

```text
enqueue(target = instance1/company1/userA/spaceA)
logout(userA); login(userB)
claim(target = instance1/company1/userB/spaceB) →none
login(userA); same pending envelope →original submission/hash/revision retained
logout(team) →local Service still alive, team process never receives shutdown
```

- [ ] **2. RED:** `cargo test --locked -p aiks-desktop --test team_client`，Linux/Windows均执行。
- [ ] **3. 实现**：个人connection类型仍只数字回环，团队新类型要求HTTPS/TLS校验及固定origin;
原生打开系统浏览器前验证start结果；随机verifier只在原生内存，服务器交換后桌面显示所登录账号确认。
不通过URL/bearer query传令牌；本机refresh singleflight；错误响应禁止旧模式/mock回退。
个人和团队连接作为界面可选目标，不需要为了登录团队关闭或迁移个人库。

outbox追加自己的version2 migration，不在业务migration中建队列表；legacy queued row保留personal目标。
新主键/查询含user/company，原有排除规则与cursor一并绑身份；来源session同ID不同用户不共享receipt。
Logout取消未来dispatch，已在途请求提示可能已接收，不谎称撤回；credentials丢失需重新登录，不能隐式换user。
- [ ] **4. GREEN/回归:** 安全存储失败、TLS失败、浏览器取消、服务停机、refresh重放、断网恢复;
原生actual library测试与service harness均跑，保留S1 Windows路径回归。
- [ ] **5. 提交:** `feat(desktop): add optional team sign-in without retargeting personal data`。

## Task 10：团队分享 UI 与显式个人发布

**Files:** Create `apps/aiks-desktop/src/api/team.ts`、`src/pages/service/{TeamConnection,ShareDialog}.tsx`、
`src/pages/service/team-sharing.test.tsx`；Modify `src/pages/ServiceStatusPage.tsx` 及 service 知识阅读组件。
新增服务端 `POST /api/v1/knowledge/import`（Task8写入管道）与幂等导入测试。

**Interfaces:** 安全DTO `TeamIdentity {connection_id,display_name,company_name,user_name}`；
`ShareState {grant_version,grants,can_manage}`；列表view `mine/shared_to_me/department`，不是三套正文。
`ImportRequest {operation_id,title,markdown,source_fingerprint}` 没有owner/company任意覆盖字段；
服务将owner设为当前认证用户，初始grants为空。UI不复制旧本地owner_id作为团队身份。

- [ ] **1. RED行为测试**：

```text
cold personal start, no company settings → skip guide → Knowledge
choose team → unauthenticated → login action, no implicit history upload
reader opens shared knowledge → read UI, no edit/share/archive controls
owner opens share → search names/departments → default descendants false → save → reopen preserved
choose local knowledge → Publish to company confirmation shows company+account+summary
cancel → zero request; confirm →private imported document; re-click →same operation receipt
```

用真实组件事件与测试IPC适配；不得仅对源码contains断言代替点击/读写顺序。
`npm test`覆盖late response、网络错误不变空列表、switch connection后旧请求不能写入新视图。
- [ ] **2. RED:** `cd apps/aiks-desktop && npm test -- src/pages/service/team-sharing.test.tsx`。
- [ ] **3. 实现**：模型/来源未配置不阻止个人阅读；team缺配置显示管理员未配置而不是按钮死转。
分享只读，人员和部门并存，列出授权来源；撤销一个通路还有效时说明原因。
本地发布独立确认、保留本地原件，不上传源会话/未授权附件；复制到team后再由owner分享，
不承诺双向同步。渲染转义文本，外部图片/内嵌HTML不默认加载；导入body执行相同预算和内容检查。
- [ ] **4. GREEN/回归:** 前端全量test/build + Service导入幂等/归属测试；
原生窗口实机流程单列验收，测试IPC替身不能作为GUI已真机验收的依据。
- [ ] **5. 提交:** `feat(ui): add company connection and owner-managed knowledge sharing`。

## Task 11：启用受控部署、配置检查入口与整体验收

**Files:** Create `deploy/team/{service.toml.example,.env.example,nginx.conf.example,aiks-service.service.example,README.md}`、
`scripts/check-team-deployment.py`、`.github/workflows/team-contracts.yml`；
Modify `apps/aiks-service/src/{config,bootstrap,routes}.rs`、`.gitignore`、
`docs/implementation/dingtalk-configuration.md`、`AGENTS.md`（只把已经实现的S2能力更新为现状）。

**Interfaces:**
`aiks-service --config /absolute/service.toml --check-config` →无副作用检查；
`aiks-service --config /absolute/service.toml` team显式启动；
personal原来的 `--bootstrap-stdin`/managed副进程启动保持兼容。

- [ ] **1. RED配置/网络测试**：空team配置启动不建库不监听；只有 `team.enabled` 和 `team.dingtalk.enabled` 均为 true 才可能启用；
缺secret给字段码；个人无需钉钉。Callback Host伪造、任意Origin/Forwarded header、将AIKS地址改思源地址均拒绝。
解析实际部署配置只允许代理对外开放AIKS已注册route，思源不能有外网publish端口。

```text
--check-config empty example →nonzero + missing field names; no state.db created
--check-config synthetic complete secrets →0; no socket and no upstream request
team start with wrong mode-bound database →nonzero; original bytes/rows preserved
owner login/read; other user private read →404
read-only shared user edit →403
GET /api/query/sql, /proxy/*, /api/file/getFile →404
```

- [ ] **2. RED:** `cargo test --locked -p aiks-service --test team_deployment`（新增同名测试文件），
`python scripts/check-team-deployment.py deploy/team`。
- [ ] **3. 只在Tasks1–10授权端到端通过后接team启动**：默认同机Nginx终止TLS，
Service和思源各自绑定回环、独立目录/服务账户；origin从静态配置决定，不从请求header推导callback。
HTTPS public origin不意味着思源公开；本期先交付同机systemd/Nginx参考配置，Docker安装包属S3。
Nginx匹配允许path，转发固定Host；不用全能`/proxy/*`，禁止外部伪代理头影响身份。健康响应不含公司人员/路径。

把本次docs样例键原样搬到deploy/team模板，加载器测试同时读取模板避免说明漂移；
新增.gitignore精确排除 `/deploy/team/.env`、`/deploy/team/service.toml`、`/deploy/team/secrets/`、
`/deploy/team/data/`，保留example文件；不把用户配置覆盖入模板。
系统服务使用 EnvironmentFile 只为服务端环境注入，不在交互shell执行任意文件。
日志清除callback query、auth header和provider错误正文；公开新登录接口前逐项处理/记录依赖审计告警，
可利用的高危问题必须阻止网络发布，不用“功能测试通过”代替审计。

备份：暂停新写入→drain本服务任务→停止内部思源→SQLite checkpoint/一致性拷贝及workspace备份→
记录manifest及文件hash→恢复到新隔离目录验证→再启动原服务。失败不删除原目录；回滚用停止后的成对备份，
不让旧二进制原地打开未知新schema。用户名/密钥不进manifest。
- [ ] **4. 最终GREEN**：

```bash
cargo fmt --all --check
cargo test --locked -p aiks-core
cargo test --locked -p aiks-service
cargo check --locked -p aiks-cli
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cd apps/aiks-desktop
npm ci
npm test
npm run build
```

Windows执行actual native library/HTTP/lifecycle与PowerShell5.1脚本测试；Linux执行全工作区。
个人完全离线测试继续隔离外部网络+回环模型/思源，不在个人模式请求钉钉。
团队CI只用合成身份服务、临时文件、合成secret，不需要用户填真实配置。
真实钉钉测试企业验证单独填写日期/组织授权范围/版本/结果；凭据本地注入，不能放CI日志。
真实思源版本与恢复验证、GUI浏览器回调交接、基本多人读取/同owner双窗口保存测试各自列证据。
读取最新SHA所有Checks，不能借之前绿色提交替代本次完成声明。
- [ ] **5. 提交与交接**：`feat(deploy): enable verified single-company mode with explicit DingTalk configuration`。
用户未填钉钉参数时报告“合成联调通过，真实企业联调待配置”，不能宣布真实登录已通。
最终仍留功能分支，main/Release另行授权。

## 计划自审与交接

- 规格1–2 → Tasks1/2/6/9：同一核心、无强制迁移、个人兼容。
- 规格3–4 → Tasks1/3/5/9/11：参数手填、一次性登录、公司约束、凭据不外泄。
- 规格5–7 → Tasks2/4/7/8/10：组织快照/并集、owner、私有源和附件不越权。
- 规格8 → Tasks6/7/8：范围前置、受控编辑、非分布式事务恢复。
- 规格9 → Task11：HTTPS/内部思源/备份/成对恢复/真实联调单列。
- 规格10–12 → 任务依赖顺序、逐步测试与最后启用；不把中间状态作为完整公司版交付。
- 五项Review Focus分别有明确test数据与预期；支持Unix/Windows而非只检查Linux。

本次提交仅包含计划、填写说明和docs样例，未更改产品源码、运行配置或数据库。
请审阅计划，执行方式沿用当前会话逐项编码、测试并提交；收到计划确认后从Task1开始，
不需要提供真实钉钉参数，不再次要求提交Secret。

## 官方契约核对记录（2026-09-23）

- 浏览器 OAuth、authCode/state、redirect URI 与 openid/corpid：
  https://opensource.dingtalk.com/developerpedia/docs/develop/permission/token/browser/get_user_app_token_browser/
- 企业应用 token 的 snake_case 协议：
  https://open-dingtalk.github.io/developerpedia/docs/develop/permission/single_to_multi/new_get_app_token/
- Client Secret 保密要求：
  https://opensource.dingtalk.com/developerpedia/docs/learn/permission/intro/permission-glossary/

以上是上游协议依据。本计划的自有接口、路径、生命周期和预算是本项目约定；
目录各API的实际权限仍由Task3保存官方契约和fixture，不将搜索摘要当作真实企业联调。
