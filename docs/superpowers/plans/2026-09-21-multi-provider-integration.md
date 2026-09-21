# AIKS Multi-Provider Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Antigravity、Cursor、Cursor Agent、Cline、Roo Code、Kilo Code、GitHub Copilot、Kimi Code、Qwen Code、Continue、Aider 共 11 个来源接入 AIKS 的本地会话导入、增量同步、知识处理和搜索。

**Architecture:** 保留 `SessionProvider` 与现有 Canonical Model；为不同存储格式编写独立适配器，只共享有边界的文件读取、只读数据库访问和已确认的消息块转换。来源描述由 Core 统一提供给 Desktop/CLI，扫描结果显式携带完整性和读取范围，防止扫描失败或配置收窄造成误判删除。不建立第二套 Pipeline、Search 或 SiYuan 客户端。

**Tech Stack:** 现有 Rust workspace、Tokio、serde_json、rusqlite、chrono、walkdir、sha2；React/TypeScript、Tauri、Vitest。优先使用现有依赖，不升级锁文件或默认模型。

**Spec:** `docs/superpowers/specs/2026-09-21-multi-provider-integration-design.md`

**State:** 用户已确认设计范围；本文件是待审阅的实施计划，不是实现或测试完成报告。所有任务尚未勾选。

**Branch:** `feature/multi-provider-integration`

**Inspected baseline:** `d50445ef31c985e96b03c112fac7be5cdd6ceb26`，产品代码来自 `main` 的 `d676480bce84c9326078cd19341b3e7fae1d2b0d`。

**Execution recommendation:** 当前会话逐任务实施；每个任务记录先失败、后通过的测试证据，最后进行整分支审查和完整 CI。没有可用的独立审查执行器时，明确记录为自审，不能把自审称为独立审查。功能分支完成前不合并 `main`，不发布安装包。

## Global Constraints

以下约束来自已确认设计，适用于每个任务：

- 所有 11 个来源都在范围内，不以分批为由取消较复杂来源。
- 旧五种来源的 key、配置节、档案路径和记录身份不变，保留 WorkBuddy 正确显示名与当前搜索修复。
- Canonical 唯一身份仍是 `(source, external_session_id)`。
- 显式路径高于工具环境变量，再高于平台默认路径；不会忽略错误显式路径而偷偷导入另一处数据。
- Aider 的递归发现只限用户配置的项目根，不默认扫描整个磁盘或凭 transcript 内 `cwd` 任意扩展读取范围。
- SQLite 采用只读、有限 busy timeout、WAL-aware 读取；禁止修改上游 schema、数据、WAL 或配置。
- 部分解析不应静默覆盖之前完整导入的会话。
- 用户关闭来源/移除配置根不等于授权删除已导入知识或思源文档。
- 未完成约定消息形态时不得宣称该来源已完整接入。
- 不为本次任务清空数据库、不原地修改既有 migrations、不换默认模型、不重写搜索引擎。

## Review Focus

1. 多根、同名 ID 与配置收窄：移除一个根后保留其旧记录；两个不同 store 的同名会话不得互相覆盖。Task 2、6、9、10、13 固定这些行为。
2. 缺少消息时间：不得把文件每次修改时间写进所有旧消息，导致追加一行就改变全部历史消息的 hash。Task 4、5、7、8 覆盖。
3. 扫描后文件被替换、JSONL 半写、WAL 更新：加载时再次检查；不完整读取不覆盖之前完整内容。Task 1、2、9、13 覆盖。
4. Markdown 围栏、消息包装与未知块：代码中的伪会话标题不切分；不因字段名相似就丢掉原文、伪造角色或读取附件。Task 3、5、7、8 覆盖。
5. 参考实现“可显示”不等于真实会话：Antigravity token 统计合成的对话不能入库；未关联到调用的工具结果不能伪造成功结果。Task 8、10、11 覆盖。

---

## 0. 已核对的代码事实、证据与文件分工

### 基线事实

`SourceKind` 位于 `crates/aiks-core/src/model/mod.rs`，已有 `as_str`、`display_name`、`from_str`。新增来源应扩展这一个定义。尤其不要顺手改变现有枚举的 serde 序列化：现有 `#[serde(rename_all = "snake_case")]` 与手写 `as_str` 并非对所有旧变体都一致；旧 archive 的字节契约须用回归测试保护。

`compute_session_hash` 已包含标题和消息正文，排除了会话的扫描时间。新适配器不能为缺少时间的消息使用变化的文件 mtime 或 `Utc::now()`。

`ProviderRegistry::discover_all_detailed` 目前只区分 `Result<Vec<SessionSummary>>`；`SyncEngine::mark_missing_sessions` 将成功返回的来源视作完整扫描。本次必须让部分扫描和配置范围在判缺失时可区分。

`SyncEngine::run_sync_with_candidate_handler_and_trigger` 目前先扫描所有 Provider，再按来源过滤。选择一个来源时应在调用 Provider 前完成过滤。

### 固定参考版本

主要参考仓库：`jhlee0409/claude-code-history-viewer`，commit `fdfc766ce7f0d76dceb03087aedac47add33d61b`。执行时读取这个 commit，而不是浮动 `main`。读取对应文件内的完整解析函数和相关 synthetic tests 后再移植，保留 MIT 版权及修改说明到 `THIRD_PARTY_NOTICES.md`。参考项目的界面、进程启动、删除、遥测、任意附件读取、无声吞错等逻辑不在移植范围。

| 模块 | 固定参考文件（相对于参考仓库） |
| --- | --- |
| Qwen | `src-tauri/src/providers/qwen.rs` |
| Continue | `src-tauri/src/providers/continue_dev.rs` |
| Cursor Agent | `src-tauri/src/providers/cursor_agent.rs` |
| Cline/Roo/Kilo | `src-tauri/src/providers/cline.rs` |
| Aider | `src-tauri/src/providers/aider.rs` |
| Kimi 新旧 | `src-tauri/src/providers/kimi.rs`、`kimi_code.rs` |
| Cursor IDE | `src-tauri/src/providers/cursor.rs` |
| Copilot | `src-tauri/src/providers/copilot_cli.rs`、`copilot.rs`、`vscode.rs` |
| Antigravity | `src-tauri/src/providers/antigravity_cli.rs`、`antigravity.rs`、`src-tauri/src/commands/antigravity.rs` |

**新增证据约束：** 固定版本 `providers/antigravity.rs` 的 `load_messages` 明确创建 fake user turn，并把 token counts 写成 assistant text；同模块还通过 protobuf/log 字符串推断工具名称。不得移植这些路径当作真实消息解析。Antigravity CLI 的日志可独立实现；IDE 没有可验证的正文时必须返回 Unsupported，不把“目录存在”“usage 存在”算会话导入成功。该限制与已确认设计中的不伪造正文要求一致，不代表 IDE 全形态已完成。

### 文件分工

| 文件 | 职责 |
| --- | --- |
| `crates/aiks-core/src/providers/local_io.rs`（新） | 受控读取、JSONL 完整性、目录预算、只读 SQLite、阻塞任务边界 |
| `crates/aiks-core/src/providers/discovery.rs`（新） | 带完整性和诊断的扫描结果；安全判缺失辅助 |
| `crates/aiks-core/src/providers/catalog.rs`（新） | 描述数据和新增适配器工厂，不复制 Canonical Model |
| `crates/aiks-core/src/config/mod.rs` | 新来源配置，保留旧五种配置类型 |
| `crates/aiks-core/src/model/mod.rs` | 新来源稳定 key、显示名与枚举转换 |
| `crates/aiks-core/src/providers/{qwen,continue_dev,cursor_agent,aider}.rs`（新） | 各自的文件型来源 |
| `crates/aiks-core/src/providers/cline_family.rs`（新） | 三个来源的共享存储家族适配；不同扩展身份和索引策略 |
| `crates/aiks-core/src/providers/kimi/{mod,legacy,wire}.rs`（新） | 根/索引、旧 context、新 wire 重放分离 |
| `crates/aiks-core/src/providers/cursor/{mod,global,workspace}.rs`（新） | 根与去重、全局 Composer、旧工作区 SQLite |
| `crates/aiks-core/src/providers/copilot/{mod,cli,vscode}.rs`（新） | 三入口聚合与去重、CLI/Desktop 事件、VS Code 日志 |
| `crates/aiks-core/src/providers/antigravity/{mod,cli,desktop}.rs`（新） | 两种根；CLI 日志与 IDE 正文/Unsupported 分类 |
| `crates/aiks-core/tests/support/provider_fixture.rs`（新） | 只使用临时目录的来源测试工具 |
| `crates/aiks-core/tests/fixtures/providers/`（新子目录） | 每个来源、每个存储形态的合成样例和来源说明 |
| `crates/aiks-core/src/providers/mod.rs`、`engine/mod.rs`、`sync/engine.rs` | 新工厂注册、筛选前置、诊断与现有同步接线 |
| `apps/aiks-desktop/src-tauri/src/provider_commands.rs`（新）、`src-tauri/src/lib.rs` | 来源描述 command 和注册 |
| `apps/aiks-desktop/src/api/{types,tauri,mock}.ts`、`src/source-display.ts` | 描述传输与显示；保留现有调用兼容 |
| `apps/aiks-desktop/src/pages/{SourcesPage,SessionsPage,SessionDetailPage,ProcessingPage,ProcessingDetailPage,KnowledgePage}.tsx` | 数据源、筛选和品牌名显示 |
| `crates/aiks-core/src/renderer/markdown.rs`、`sink/siyuan.rs`、`knowledge/publisher.rs` | 已确认来源的显示名；不重命名历史思源路径 |
| `docs/reference-analysis/multi-provider-support.md`（新） | 实际格式证据、支持矩阵、真机状态 |

测试中每次写文件和 SQLite 都在临时根进行；不访问开发机默认工具目录。所有后文新增类型/函数是计划中的接口，不是声称基线已经存在。

## Task 1: 受控本地读取与可执行安全回归

**Files:** 新建 `providers/local_io.rs`、`tests/provider_local_io.rs`、`tests/support/provider_fixture.rs`；在 `providers/mod.rs` 导出新模块。

**Interfaces:** 产生 `ScopedReader::new(root: PathBuf, limits: ReadLimits) -> Result<Self>`、`checked_path(&self, relative: &Path) -> Result<PathBuf>`、`for_each_jsonl(&self, relative: &Path, visit: impl FnMut(usize, serde_json::Value) -> Result<()>) -> Result<ReadReport>`。`ReadReport` 含 `complete: bool`、`malformed_lines: usize`、`partial_tail: bool`。读取不完整必须由调用方拒绝覆盖已有完整会话。

- [ ] 写测试：`../connectors/token`、绝对路径、子目录 symlink、Unix FIFO、目录替换均拒绝；合法 JSONL 可读；半写尾行与中间坏行分类不同。使用以下最小数据固定行为：

```rust
let dir = tempfile::tempdir().unwrap();
std::fs::write(dir.path().join("events.jsonl"), b"{\"type\":\"user\"}\n{\"type\":").unwrap();
let reader = ScopedReader::new(dir.path().to_path_buf(), ReadLimits::default()).unwrap();
let mut count = 0;
let report = reader.for_each_jsonl(std::path::Path::new("events.jsonl"), |_, _| {
    count += 1;
    Ok(())
}).unwrap();
assert_eq!(count, 1);
assert!(!report.complete);
assert!(report.partial_tail);
assert!(reader.checked_path(std::path::Path::new("../token")).is_err());
```

- [ ] 运行 `cargo test -p aiks-core --test provider_local_io`，先记录失败；API 新增造成的编译失败之后，还要单独运行具体行为断言以验证 RED。
- [ ] 实现：逐层拒绝链接/Windows reparse 逃逸，只允许普通文件；先做 canonical containment 再读，加载时重新校验。使用 `BufRead::fill_buf/consume` 限制单行分配，不能先无界 `read_line` 再检查长度。初始预算为单行 8 MiB、单文件 256 MiB、每 store 100000 个候选、目录深度 16；到限返回显式不完整诊断。来源专用 allowlist 决定允许文件；reader 不自行全盘递归。读取前后比较文件 metadata；源变化时最多重试一次，仍变化则报告不完整。

```rust
let conn = rusqlite::Connection::open_with_flags(
    &validated_db_path,
    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
)?;
conn.busy_timeout(std::time::Duration::from_secs(5))?;
```

SQLite 不使用 `immutable=1`，不主动 checkpoint；保持短读事务以读取一致的 WAL 快照。数据库连接在 `spawn_blocking` 内创建/使用/销毁，不跨 await。reader 的上限作用于已读数据，不是虚假的“总耗时上限”。
- [ ] 增加 WAL writer 未关闭时可见新记录的测试；记录主数据库内容/schema 不变。SQLite 自身的共享内存行为不能冒充“整个目录绝对没有任何系统级文件变化”。Windows junction 在 Windows 测试中单独执行，Unix 测试不能代替。
- [ ] 测试全部通过后提交 `feat: add scoped read-only provider IO`。

## Task 2: 来源身份、兼容配置、筛选和完整扫描契约

**Files:** 修改 `model/mod.rs`、`config/mod.rs`、`providers/mod.rs`；新建 `providers/{catalog,discovery}.rs`、`tests/provider_catalog.rs`、`tests/provider_discovery_scope.rs`。

**Interfaces:** 新增 `ExternalProviderConfig { enabled: bool, path: String, paths: Vec<String> }`，默认 enabled=true。新增配置节按下表定义；仅新增来源使用这个类型，旧 ProviderConfig 和 CodexProviderConfig 保持不变。`SourceDescriptor { key: String, display_name: String, config_key: String, enabled: bool }` 由 Core 返回。新增 `build_external_provider(source: SourceKind, config: &ExternalProviderConfig) -> anyhow::Result<Box<dyn SessionProvider>>`，各实现逐任务接入，不注册假的空解析器。

| key/config section | SourceKind 新变体 | display_name |
| --- | --- | --- |
| antigravity | Antigravity | Antigravity |
| cursor | Cursor | Cursor |
| cursor_agent | CursorAgent | Cursor Agent |
| cline | Cline | Cline |
| roo_code | RooCode | Roo Code |
| kilo_code | KiloCode | Kilo Code |
| github_copilot | GithubCopilot | GitHub Copilot |
| kimi_code | KimiCode | Kimi Code |
| qwen_code | QwenCode | Qwen Code |
| continue | Continue | Continue |
| aider | Aider | Aider |

Rust 的 Continue 配置字段用 `continue_dev` 并加 `#[serde(rename = "continue")]`。新枚举显式 serde rename 到表中 key。旧枚举的序列化、alias 和档案读取不变。

- [ ] 写 key/serde/旧配置回归，尤其保护 `WorkBuddy` 旧 archive 格式，不把显示名写入数据库 key。

```rust
let kind = SourceKind::from_str("qwen_code").unwrap();
assert_eq!(kind.as_str(), "qwen_code");
assert_eq!(kind.display_name(), "Qwen Code");
assert_eq!(serde_json::to_value(kind).unwrap(), serde_json::json!("qwen_code"));
let config: aiks_core::config::Config = toml::from_str("[providers.workbuddy]\nenabled=false\n").unwrap();
assert!(!config.providers.workbuddy.enabled);
assert!(!config.ai.enabled);
assert!(!config.embedding.enabled);
```

- [ ] 在 `provider_discovery_scope` 用两个带原子调用计数器的 SessionProvider 替身：指定 Qwen 来源时另一替身的发现次数必须为 0。新增 `ProviderRegistry::discover_selected(source: Option<SourceKind>) -> Vec<(SourceKind, anyhow::Result<DiscoveryReport>)>`；原有发现 API 继续保留兼容包装。
- [ ] 定义 `DiscoveryReport { sessions: Vec<SessionSummary>, complete: bool, diagnostics: Vec<DiscoveryDiagnostic>, covered_paths: Vec<PathBuf> }`。`DiscoveryDiagnostic` 仅记录 code、store 编号/相对定位和计数，不包含原始行、正文或凭据。每个新 Provider 覆盖 `discover_report`，允许返回健康 store 的会话但把完整性标为 false。默认旧 Provider 的报告由旧 `discover_sessions` 产生。旧 `discover_all_detailed` 遇到不完整报告返回 Err，而不是误称全量成功。
- [ ] 增加 `ProviderHealth::Unsupported { message }`，保留 `is_ok` 仅对 Ok 为真；错误、未配置、未找到分别保留。配置存在但路径无效不回退别处。`path` 与 `paths` 合并为显式根列表，规范化去重；只要存在显式根，不再自动加入默认根。Aider 无显式根返回 NotConfigured。
- [ ] 在 Task 1 的测试 helper 中增加以下工具；后文解析测试均复用它们，不读取用户 home：

```rust
pub fn fixture(files: &[(&str, &str)]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for (relative, content) in files {
        let path = root.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    root
}
pub async fn load_one(key: &str, root: &std::path::Path) -> aiks_core::model::NormalizedSession {
    let cfg = aiks_core::config::ExternalProviderConfig {
        enabled: true,
        path: root.to_string_lossy().into_owned(),
        paths: Vec::new(),
    };
    let p = aiks_core::providers::catalog::build_external_provider(
        aiks_core::model::SourceKind::from_str(key).unwrap(), &cfg,
    ).unwrap();
    let found = p.discover_sessions().await.unwrap();
    assert_eq!(found.len(), 1);
    p.load_session(&found[0]).await.unwrap()
}
pub fn texts(s: &aiks_core::model::NormalizedSession) -> Vec<String> {
    s.messages.iter().flat_map(|m| &m.blocks)
        .filter_map(|b| b.text_content().map(str::to_owned)).collect()
}
```

- [ ] 运行 `cargo test -p aiks-core --test provider_catalog --test provider_discovery_scope`；验证失败再实现，最终提交 `feat: add provider catalog and scoped discovery contracts`。

## Task 3: Qwen Code，建立第一条完整文件型接入

**Files:** 新建 `providers/qwen.rs`、`tests/qwen_provider.rs`、`tests/fixtures/providers/qwen_code/`；修改 `providers/catalog.rs`、`providers/mod.rs` 注册 Qwen。

**Interfaces:** `QwenProvider::new(config: &ExternalProviderConfig) -> anyhow::Result<Self>` 实现 SessionProvider；版本 `qwen-jsonl-v1`。默认根优先 `QWEN_RUNTIME_DIR`、`QWEN_HOME`、`~/.qwen`。显式 path 指 runtime root，不是某个 transcript。

- [ ] 固定 user/assistant JSONL 测试：

```rust
#[path = "support/provider_fixture.rs"] mod support;
#[tokio::test]
async fn qwen_imports_real_messages_and_internal_session_id() {
    let root = support::fixture(&[("projects/p/chats/file-name.jsonl", concat!(
        "{\"uuid\":\"u1\",\"sessionId\":\"s1\",\"type\":\"user\",\"cwd\":\"/example/p\",\"message\":{\"role\":\"user\",\"parts\":[{\"text\":\"QWEN_UNIQUE 问题\"}]}}\n",
        "{\"uuid\":\"a1\",\"parentUuid\":\"u1\",\"sessionId\":\"s1\",\"type\":\"assistant\",\"message\":{\"role\":\"model\",\"parts\":[{\"text\":\"回答\"}]}}\n"
    ))]);
    let s = support::load_one("qwen_code", root.path()).await;
    assert_eq!(s.external_session_id, "s1");
    assert_eq!(support::texts(&s), vec!["QWEN_UNIQUE 问题", "回答"]);
    assert_eq!(s.messages[1].parent_id.as_deref(), Some("u1"));
}
```

- [ ] 运行 `cargo test -p aiks-core --test qwen_provider` 验证失败，然后实现只枚举 `projects/*/chats/*.jsonl`。内部 `sessionId` 为会话身份；同文件混合身份必须分离或明确报错，不能按文件名覆盖。
- [ ] parts 转换规则：`text`→Text，`thought:true` 的 text→Thinking，`functionCall {id,name,args}`→ToolCall，`functionResponse {id,response}`→ToolResult；未知 part→Unknown。外层 uuid/parentUuid/timestamp/model/usageMetadata 分别映射已存在字段；缺失值保持 None。
- [ ] 增加工具 call/result、未知块、半写保护、元数据标题/模型变化、真实 cwd 不可逆编码、重复导入与追加不改原消息 ID 的 fixtures。
- [ ] 测试通过后提交 `feat: import Qwen Code sessions`，支持矩阵只把已测 JSONL 形态置为 automated-tested。

## Task 4: Continue

**Files:** 新建 `providers/continue_dev.rs`、`tests/continue_provider.rs`、`tests/fixtures/providers/continue/`；注册 Continue。

**Interfaces:** `ContinueProvider::new(&ExternalProviderConfig) -> Result<Self>`；版本 `continue-json-v1`；根是 `CONTINUE_GLOBAL_DIR` 或 `~/.continue`。

- [ ] 添加测试数据，名称故意与 sessionId 不同，并放置非会话索引：

```rust
let root = support::fixture(&[
    ("sessions/index-name.json", r#"{"sessionId":"s1","title":"Continue test","workspaceDirectory":"C:\\example\\中文","history":[{"message":{"role":"user","content":"CONTINUE_UNIQUE"}},{"message":{"role":"assistant","content":[{"type":"text","text":"answer"}]}}]}"#),
    ("sessions/sessions.json", r#"[{"sessionId":"not-a-transcript"}]"#),
]);
let s = support::load_one("continue", root.path()).await;
assert_eq!(s.external_session_id, "s1");
assert_eq!(support::texts(&s), vec!["CONTINUE_UNIQUE", "answer"]);
assert!(s.messages.iter().all(|m| m.created_at.is_none()));
```

- [ ] 运行 `cargo test -p aiks-core --test continue_provider` 观察失败，再实现 `sessions/*.json` 白名单，跳过 `sessions.json`。逐条处理 `history[].message`；工具记录按固定参考中的真实字段与 synthetic tests 转换，不把 contextItems 自动当用户发言。
- [ ] 读取 title/workspaceDirectory；图片和文件只转引用。缺时间使用 None，不能用 mtime 填每条消息。未知 history/message block 保留诊断或 Unknown，不无声变成空文本。
- [ ] 写重复载入 hash 稳定、修改文件 mtime 后消息时间仍 None、无效 JSON 不覆盖原会话、context 引用不触发外部文件读取的测试。
- [ ] 测试通过后提交 `feat: import Continue sessions`。

## Task 5: Cursor Agent

**Files:** 新建 `providers/cursor_agent.rs`、`tests/cursor_agent_provider.rs`、`tests/fixtures/providers/cursor_agent/`；注册 CursorAgent。

**Interfaces:** `CursorAgentProvider::new(&ExternalProviderConfig) -> Result<Self>`；版本 `cursor-agent-jsonl-v1`；根为 `~/.cursor`，只检查 `projects/*/agent-transcripts/**/*.jsonl`。

- [ ] 添加 JSONL fixture：

```rust
let root = support::fixture(&[("projects/p/agent-transcripts/s1/s1.jsonl", concat!(
    "{\"role\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"<user_query>CURSOR_AGENT_UNIQUE</user_query><context>noise</context>\"}]}}\n",
    "{\"role\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"answer\"}]}}\n"
))]);
let s = support::load_one("cursor_agent", root.path()).await;
assert_eq!(s.external_session_id, "s1");
assert_eq!(support::texts(&s), vec!["CURSOR_AGENT_UNIQUE", "answer"]);
assert!(s.messages.iter().all(|m| m.created_at.is_none()));
```

- [ ] `cargo test -p aiks-core --test cursor_agent_provider` 先失败，再实现来源命名空间、文件 uuid stem 和稳定行序号消息 ID。没有可证 cwd 时不把不可逆目录编码伪装成绝对项目路径。
- [ ] 仅对已确认的用户包装提取 `<user_query>`；助手讨论这些标签时不裁剪。嵌套 tool_result 保留为 ToolResult，command_output 保留输出；redacted 不恢复正文，不作为空用户问题。没有 id 的工具块不要与不相干结果配对。
- [ ] 覆盖未知块、嵌套内容、同名文件在不同项目的命名空间冲突、追加旧消息不变化、错误扩展文件不读取、符号链接逃逸。
- [ ] 测试通过后提交 `feat: import Cursor Agent transcripts`。

## Task 6: Cline、Roo Code、Kilo Code，三来源分别验收

**Files:** 新建 `providers/cline_family.rs`、`tests/{cline,roo_code,kilo_code}_provider.rs`、`tests/fixtures/providers/{cline,roo_code,kilo_code}/`；注册三个来源。

**Interfaces:** `ClineFamilyProvider::new(source: SourceKind, config: &ExternalProviderConfig) -> Result<Self>` 仅接受 Cline/RooCode/KiloCode；parser version 分别为 `cline-json-v1`、`roo-code-json-v1`、`kilo-code-json-v1`。

默认在当前用户的平台编辑器 User 根下查对应 extension store；候选 editor 名称明确列举 Code、Code - Insiders、Cursor、VSCodium，不递归寻找任意 `state.vscdb`。显式根接受 extension storage 或 editor User root，必须由目录标记区分；错误根不猜测 home。

- [ ] 三个测试 target 都放入以下真实会话文件样例，分别使用各自来源 key；三者必须能独立运行：

```rust
let root = support::fixture(&[("tasks/t1/api_conversation_history.json", r#"[
 {"role":"user","content":[{"type":"text","text":"CLINE_FAMILY_UNIQUE"}]},
 {"role":"assistant","content":[{"type":"tool_use","id":"c1","name":"read_file","input":{"path":"example.txt"}}]},
 {"role":"user","content":[{"type":"tool_result","tool_use_id":"c1","content":"synthetic output"}]}
]"#)]);
let s = support::load_one("cline", root.path()).await;
assert!(support::texts(&s).contains(&"CLINE_FAMILY_UNIQUE".to_string()));
assert!(s.messages.iter().flat_map(|m| &m.blocks).any(|b|
    matches!(b, aiks_core::model::ContentBlock::ToolCall { id: Some(id), .. } if id == "c1")
));
```

Roo/Kilo 对应测试将 key 改为自己的稳定 key，并加入各自索引 fixture，不能只调用 Cline 测试作为证明。
- [ ] 逐个运行 `cargo test -p aiks-core --test cline_provider`、`--test roo_code_provider`、`--test kilo_code_provider`，观察 RED。
- [ ] 实现 extension ID 映射：`saoudrizwan.claude-dev`、`rooveterinaryinc.roo-cline`、`kilocode.kilo-code`。Cline 读取 `state/taskHistory.json`；Roo 读取 `tasks/_index.json`；Kilo 对当前 editor `globalStorage/state.vscdb` 的 ItemTable 只按 extension ID 查对应 globalState，按已确认 shape 提取 taskHistory。workspace/cwdOnTaskInitialization 按格式适配，不互相覆盖有效字段。
- [ ] API history 是对话正文优先来源；缺失时才按 `ui_messages.json` 已确认 say/ask 事件还原。API/UI 双份文件不双重导入；UI 的 partial、API 请求日志、命令状态不得冒充完整对话。`task_metadata.json` 只作元数据。
- [ ] 固定三来源不同 store 同 t1 不碰撞、Kilo 仅 SQLite 索引仍能定位任务、无索引但合法 task 文件可导入、索引列出已丢失正文会报不完整、坏 task 不损坏其他 task、混合 text/tool_result 不丢 text 的测试。只读取当前 extension 的条目，禁止导入邻接凭据。
- [ ] 三个 target 通过后提交各自实现或一个明确含三份测试的家族提交 `feat: import Cline Roo and Kilo task histories`。

## Task 7: Aider，Markdown 状态机与稳定会话身份

**Files:** 新建 `providers/aider.rs`、`tests/aider_provider.rs`、`tests/fixtures/providers/aider/`；注册 Aider。

**Interfaces:** `AiderProvider::new(&ExternalProviderConfig) -> Result<Self>`；版本 `aider-markdown-v1`。仅显式配置的项目根；无根为 NotConfigured。

- [ ] 用下面的原始文件覆盖围栏内伪头和两个真正会话头：

```rust
let text = concat!(
    "# aider chat started at 2026-09-21 10:00:00\n\n#### AIDER_UNIQUE\n\n",
    "answer\n```md\n# aider chat started at 1900-01-01 00:00:00\n#### not-a-user\n```\n",
    "# aider chat started at 2026-09-21 11:00:00\n\n#### second\n\nanswer two\n"
);
let root = support::fixture(&[(".aider.chat.history.md", text)]);
let cfg = ExternalProviderConfig { path: root.path().to_string_lossy().into_owned(), ..Default::default() };
let p = AiderProvider::new(&cfg).unwrap();
assert_eq!(p.discover_sessions().await.unwrap().len(), 2);
```

- [ ] `cargo test -p aiks-core --test aider_provider` RED 后实现 line-state machine：跟踪 backtick/tilde 围栏及围栏长度；只有围栏外精确匹配的会话头才切分。用户 `#### ` 分隔、助手正文和 Aider 系统/工具输出按固定参考格式分类，未明确角色的片段保持 Unknown。
- [ ] external_session_id 由 canonical 历史文件身份、原始会话头和同头出现序号进行长度前缀编码后 SHA-256；不含正文 hash、文件 mtime 或扫描序号。追加正文不改旧会话 ID。没有时区的原始头保存在 metadata，不能声称是 UTC。
- [ ] 增加 CRLF、BOM、中文、多行问题、同秒重复会话头、未闭合围栏、末尾追加、两个项目相同会话头、重叠根去重和无配置不扫描 home 的测试。
- [ ] 测试通过后提交 `feat: import scoped Aider history`。

## Task 8: Kimi Code，新旧布局独立解析，事件重放单独审查

**Files:** 新建 `providers/kimi/{mod,legacy,wire}.rs`、`tests/{kimi_legacy,kimi_wire}_provider.rs`、`tests/fixtures/providers/kimi_code/{legacy,wire}/`；注册 KimiCode。

**Interfaces:** `KimiProvider::new(&ExternalProviderConfig) -> Result<Self>`；版本 `kimi-session-v1`；内部 layout 标记分别为 `kimi-legacy`、`kimi-code-wire-v2`。根支持旧 `.kimi` 和 `KIMI_CODE_HOME`/`.kimi-code`；显式多根去重，不自动扩展到兄弟目录。

- [ ] 先从固定参考 `kimi.rs` 的 synthetic tests 提取旧 context/wire 样例到 legacy fixture，保留角色、工具关联和 checkpoint shape，替换所有路径及正文为合成值，并记录参考 commit 与 fixture 的来源函数。legacy 与 wire 都要多轮正文测试，不只检查 count。
- [ ] 新版使用实际 wire vocabulary 写 RED 测试；以下只把已确认的可见消息送入重放：

```rust
let wire = concat!(
    "{\"type\":\"context.append_message\",\"time\":1000,\"message\":{\"id\":\"u1\",\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"KIMI_UNIQUE\"}]}}\n",
    "{\"type\":\"context.append_loop_event\",\"time\":1001,\"event\":{\"type\":\"step.begin\"}}\n",
    "{\"type\":\"context.append_loop_event\",\"time\":1002,\"event\":{\"type\":\"content.part\",\"part\":{\"type\":\"text\",\"text\":\"answer\"}}}\n",
    "{\"type\":\"context.append_loop_event\",\"time\":1003,\"event\":{\"type\":\"step.end\"}}\n"
);
let root = support::fixture(&[
    ("sessions/wd_test/session_s1/state.json", r#"{"id":"s1","version":2,"cwd":"/example/p","title":"Kimi test"}"#),
    ("sessions/wd_test/session_s1/agents/main/wire.jsonl", wire),
]);
let s = support::load_one("kimi_code", root.path()).await;
assert_eq!(support::texts(&s), vec!["KIMI_UNIQUE", "answer"]);
```

- [ ] 运行 `cargo test -p aiks-core --test kimi_legacy_provider --test kimi_wire_provider`，记录失败，再移植纯重放逻辑到 wire.rs；公开接口仍为 SessionProvider，不能暴露参考项目 ClaudeMessage。
- [ ] 重放状态至少包括 history、open assistant、pending tool IDs、deferred user/system messages、undo anchors、current model。append_message 入 history 或 deferred；step.begin 开新 assistant；content.part 累加到该 step；tool.call 加关联；tool.result 仅与明确 ID 关联；step.end 结算。clear 清空对应状态；undo 按真实 user anchor 截断并仅在实际 cut 时清 pending/deferred；apply_compaction 按参考中的明确边界替换历史。
- [ ] 不复制参考的“给未返回调用合成成功输出”；未完成调用在 metadata 中标记 interrupted。完整 JSONL 但 step 未结束是运行中状态，不等于坏 JSON；半写 JSON 行则是读取不完整。未知 context mutation 不能无声忽略后称状态可信，应返回不完整/不支持诊断。
- [ ] 独立增加 undo_no_anchor、undo_open_tools、clear_then_new_turn、compaction_keeps_suffix、deferred_message_order、orphan_tool_result、usage_record_optional、same_id_legacy_and_new_roots、no_blobref_dereference 回归。断言被撤销字符串不在 texts，未知用量保持 None。
- [ ] legacy、wire 两个 target 和来源级扫描去重都通过才提交 `feat: import Kimi legacy and replay Kimi Code journals`；wire 重放需单独列为审查重点。

## Task 9: Cursor IDE，global/workspace 两种数据库

**Files:** 新建 `providers/cursor/{mod,global,workspace}.rs`、`tests/{cursor_global,cursor_workspace}_provider.rs`、`tests/fixtures/providers/cursor/`；注册 Cursor。

**Interfaces:** `CursorProvider::new(&ExternalProviderConfig) -> Result<Self>`；版本 `cursor-sqlite-v1`；根是 Cursor `User` 目录，支持显式 `CURSOR_USER_DIR` 和平台默认路径。

- [ ] 将固定参考 cursor.rs 的 global Composer、bubble 引用与 workspace synthetic tests 分别改成临时 SQLite fixture。数据库 schema 仅有实际需要的 `cursorDiskKV(key TEXT PRIMARY KEY, value TEXT)` 和 `ItemTable(key TEXT UNIQUE, value BLOB)`；测试数据只含合成 Composer、消息和 workspace 元数据。

```sql
CREATE TABLE cursorDiskKV (key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE ItemTable (key TEXT UNIQUE, value BLOB);
```

消息和 Composer value 必须从固定参考测试的真实字段形态构造，不能把 AIKS Canonical JSON 塞进库来模拟上游。
- [ ] 运行 `cargo test -p aiks-core --test cursor_global_provider --test cursor_workspace_provider` 记录 RED。
- [ ] global.rs 只查询 `composerData:%` 和对应会话引用的 bubble key；workspace.rs 读取指定工作区的 composer 列表和对应正文，不扫描所有键值或导入设置。JSON 字符串包裹的 JSON 按参考的 parse_cursor_json 处理，但设置最大嵌套解包次数，不能无限递归。
- [ ] 同一 store 中 global 优先、workspace 补充尚未覆盖的 ID，不能因任意一个 global 会话存在而把全部 legacy workspace 忽略。明确 archived/deleted 字段的意义；archived 不等于删除授权。可读取历史保留 archival metadata，真正删除状态单独处理。
- [ ] 工具、思考、文本通过原始 bubble 类型转换；缺失引用必须诊断不完整。workspace 路径从已验证元数据读取，不猜 project hash。跨 profile 的 ID 带稳定 store 命名空间，根顺序变化不改变 ID。
- [ ] 增加 live WAL、数据库锁超时、非会话 secret row 从未读取、global/legacy 混合各一会话、duplicate ID 去重、相同 ID 跨 profile 不碰撞、metadata-only rename、缺表 Unsupported、损坏单 Composer 隔离、读取失败不标 missing 测试。
- [ ] 两套 target 通过后提交 `feat: import Cursor Composer stores read-only`。

## Task 10: GitHub Copilot，CLI/Desktop/VS Code 三入口

**Files:** 新建 `providers/copilot/{mod,cli,vscode}.rs`、`tests/{copilot_cli,copilot_vscode}_provider.rs`、`tests/fixtures/providers/github_copilot/{cli,desktop,vscode}/`；注册 GithubCopilot。

**Interfaces:** `CopilotProvider::new(&ExternalProviderConfig) -> Result<Self>`；版本 `copilot-history-v1`；`metadata.entrypoint` 使用 `copilot-cli`、`copilot-desktop`、`copilot-vscode`。CLI/Desktop 同物理 events 文件只能导入一次。

- [ ] 先固定 CLI 正文测试：

```rust
let root = support::fixture(&[("session-state/s1/events.jsonl", concat!(
    "{\"id\":\"start\",\"type\":\"session.start\",\"data\":{\"sessionId\":\"s1\",\"context\":{\"cwd\":\"/example/p\"}}}\n",
    "{\"id\":\"u1\",\"type\":\"user.message\",\"data\":{\"content\":\"COPILOT_UNIQUE\",\"transformedContent\":\"wrapped duplicate\"}}\n",
    "{\"id\":\"a1\",\"type\":\"assistant.message\",\"data\":{\"content\":\"answer\"}}\n"
))]);
let s = support::load_one("github_copilot", root.path()).await;
assert_eq!(support::texts(&s), vec!["COPILOT_UNIQUE", "answer"]);
```

- [ ] 为同一个 fixture 增加 `workspace.yaml` 中 `client_name: github/autopilot`，断言只改变 entrypoint，不增加第二份会话、也不改变同物理会话的 ID。只解析需要的标量字段：遇到超出已支持 YAML 子集的结构诊断未知 client，不自己实现不完整的通用 YAML 解析器。
- [ ] 从固定参考 vscode.rs 的 synthetic tests 引入它实际支持的 `chatSessions` JSON/JSONL 形态，分别写测试并记录版本证据。JSONL 若是 snapshot/patch 而非完整消息数组，必须按路径、顺序还原状态；未支持的 patch 明确诊断，不逐行作为独立会话。
- [ ] 运行 `cargo test -p aiks-core --test copilot_cli_provider --test copilot_vscode_provider` RED 后实现。CLI 使用 user.message 原始 content，不重复导入 transformedContent；assistant toolRequests 与 execution_complete 按 toolCallId 关联，execution_start 不生成重复调用。VS Code 请求/回复/工具片段转既有 ContentBlock。
- [ ] 多入口根通过形态标记分类，显式 path 不追加默认其他入口；默认候选包括当前用户 Copilot root 与 VS Code User root。只读取 events、chatSessions、必要 workspace.json/workspace.yaml，不读取 tracked files、plans、checkpoints 或凭据。
- [ ] 覆盖 UUID 跨入口碰撞、CLI/Desktop 同文件去重、多 profile、工具失败/孤立结果、部分回复、VS Code patch/rewrite、root移除、权限错误、未安装零访问；两套 target 通过后提交 `feat: import Copilot CLI Desktop and VS Code histories`。

## Task 11: Antigravity，真实日志和严格的“不支持”边界

**Files:** 新建 `providers/antigravity/{mod,cli,desktop}.rs`、`tests/{antigravity_cli,antigravity_desktop}_provider.rs`、`tests/fixtures/providers/antigravity/{cli,desktop}/`；注册 Antigravity。

**Interfaces:** `AntigravityProvider::new(&ExternalProviderConfig) -> Result<Self>`；版本 `antigravity-local-v1`；metadata 标明 layout。默认分别探测 `.gemini/antigravity-cli` 与 `.gemini/antigravity`，两根独立报告，不把一个失败伪装成另一根的健康状态。

- [ ] 从固定参考 antigravity_cli.rs 的合成 step records 固定 `USER_INPUT`、`PLANNER_RESPONSE`、工具步和 payloadless 步样例；同时核对辅助参考 `seastart/aicoder-session-viewer` 的 `transcript.jsonl` 解析。`transcript_full.jsonl` 与 `transcript.jsonl` 必须按已验证 schema 分类，不仅按名称互相替代；共存时确定优先级并防重复导入。
- [ ] 增加下列负例：只有 usage 缓存和 protobuf 名称不能变成真实对话。

```rust
let root = support::fixture(&[
    (".token-monitor/rpc-cache/v1/s1/usage.jsonl", "{\"recordType\":\"usage\",\"sequence\":1,\"model\":\"test\",\"inputTokens\":12,\"outputTokens\":8}\n"),
]);
let cfg = ExternalProviderConfig { path: root.path().to_string_lossy().into_owned(), ..Default::default() };
let p = AntigravityProvider::new(&cfg).unwrap();
assert!(matches!(p.health_check().await, ProviderHealth::Unsupported { .. }));
assert!(p.discover_sessions().await.is_err());
```

- [ ] `cargo test -p aiks-core --test antigravity_cli_provider --test antigravity_desktop_provider` RED 后实现 CLI 的 allowlist、history 元数据 join、明确角色/正文/工具映射。时间使用源字段，payloadless 不伪造 empty user。
- [ ] IDE adapter 只准接纳经过正文 fixture 证明的本地消息字段；对固定参考实际仅提供的 usage/manifest/brain/protobuf 状态实现可解释的 Unsupported 诊断。不能复制 fake user/assistant token 摘要，不能从 `.pb` ASCII 碎片推断工具调用后入库，不能调用在线/私有 RPC、抓取 token 或启动监控器。
- [ ] 在 `multi-provider-support.md` 分别列 CLI 日志格式、IDE 已验证正文格式、IDE usage-only，不以 Antigravity 一行绿色掩盖形态差异。如果没有可验证 IDE 正文样例，该形态保持“未实现真实正文导入”，整体交付报告必须明确；不能用负例测试通过声称 IDE 接入完成。新增正文格式需要注明来源代码/匿名样例和通过的正例测试，再改变支持状态。
- [ ] 覆盖双方目录共存、伪 transcript 文件、跨会话混合 step、重复记录、unknown enum、目录有数据但缺正文、不同根同 ID、读取边界。提交 `feat: import verified Antigravity transcripts with explicit format diagnostics`。

## Task 12: Desktop、CLI、渲染统一接线

**Files:** 新建 `src-tauri/src/provider_commands.rs`、`apps/aiks-desktop/src/api/provider-catalog.test.ts`；修改文件分工表中的 Desktop API、页面、Core engine 和 renderer/sink/publisher；检查 `apps/aiks-cli/src/cli/mod.rs` 的 source 参数说明。注册位置为 `src-tauri/src/lib.rs`，不是只有启动函数的 main.rs。

**Interfaces:** `AiksEngine::source_descriptors(&self) -> Vec<SourceDescriptor>`、Tauri `get_source_descriptors`、前端 `getSourceDescriptors(): Promise<SourceDescriptor[]>`。旧 API 不删除；FullStatus 新增可选 `provider_diagnostics`，保留原有 count/health 字段。

- [ ] 前端先写 catalog 驱动测试；不能只用源码包含字符串代替全部行为测试。至少断言每个可用来源有稳定 key 和名称、Sync 按 key 调用、筛选 value 不使用品牌名、禁用/未安装/有效空库/不支持格式状态可区分。

```ts
const source = { key: "roo_code", display_name: "Roo Code", config_key: "roo_code", enabled: true };
expect(source.display_name).toBe("Roo Code");
expect(source.key).toBe("roo_code");
// 在页面交互测试中选中该来源后，断言 syncAndExtract 收到 roo_code，
// 并断言 getSessions 的 source 同样为 roo_code，而不是 Roo Code。
```

交互测试使用现有测试能力；不足时用无 DOM 的页面模型函数配合显式 API spy，并单独记录尚未做浏览器点击验收，不能冒充 E2E。
- [ ] 运行 `cd apps/aiks-desktop && npm test -- src/api/provider-catalog.test.ts` 记录 RED。
- [ ] descriptor 从 Core SourceKind/配置生成，不在 TS 再手写一份 16 项逻辑表。Mock 使用合成 descriptor fixture，并用契约测试与 Core schema 校对。页面加载失败显示错误，不拿空数组假装“没有数据源”。未安装来源可显示但不作为已导入成功。
- [ ] 数据源卡片、Sessions 来源选项、列表与详情展示读取同一 catalog；现有 `formatSourceName` 保留兼容入口和 WorkBuddy 正确大小写。已有历史数据不改 source key。新增 Provider 配置提供 path/paths 编辑和显式禁用；保留旧设置文件的其它字段。
- [ ] Renderer 与 SiYuan 新文档来源名称使用统一 display_name；既有 target_path、doc_id、baseline、conflict 记录不批量迁移或重写。用户编辑过的文档仍受原冲突保护。
- [ ] 运行前端全部测试与 build、`cargo check -p aiks-cli`、Windows desktop compile。提交 `feat: expose all provider sources in desktop and CLI`。

## Task 13: 增量同步、故障隔离和现有流水线的真实入库验收

**Files:** 修改 `providers/mod.rs`、`sync/engine.rs`、`engine/mod.rs` 的实际 discovery 调用点；新建 `tests/{multi_provider_sync,multi_provider_pipeline,provider_missing_scope}.rs`。复用现有 `SourceSessionRepo`、`SyncEngine`、Pipeline 和 Search，不另造业务入库函数。

**Interfaces:** 所有新 Provider 必须验证 `summary.source == self.source()`，拒绝跨来源 caller summary；重新校验 source_path 和内部 session ID。新增 `SessionProvider::owns_source_path(&self, path: &Path) -> bool` 或同等私有判定，精确对应该来源已完成扫描的根和文件语法。判缺失条件是：来源参与本次扫描、报告 complete、当前范围确实覆盖旧记录、对应 ID 不可见。缺少可确认路径的旧记录保守保留。

- [ ] 对 11 个来源逐个将 fixture 送入**现有同步入口**，查询 source_session 和流水线候选；第二次同步 row count 不变、canonical id 不变。SQL 断言直接针对现有表：

```sql
SELECT COUNT(*) FROM source_session WHERE source = ?1 AND external_session_id = ?2;
SELECT id, title, content_hash, parser_version, is_missing
FROM source_session WHERE source = ?1 AND external_session_id = ?2;
```

- [ ] 使用现有回环 HTTP 测试方式替代 SiYuan/模型；真实账户、私网服务不可作为通过条件。先运行 `cargo test -p aiks-core --test multi_provider_sync --test multi_provider_pipeline --test provider_missing_scope` 记录失败，再修改调用链。
- [ ] 把 source filter 移到 registry discovery 前；解析无效 source 返回错误，不能变成全来源扫描。扫描报告复用于当前同步周期与缺失判定，不为判缺失再无条件全盘发现一次。只在当前活跃周期复用，下一空闲周期重新发现。
- [ ] 案例：追加消息、删改正文、仅修改标题、SQLite WAL-only 更新、文件截断后稳定的真实短会话、半写/中间坏行、parser version 变化。完整截断且符合格式可刷新；不完整快照不覆盖旧正文。没有源时间时维持原消息 None，不添加当前扫描时间影响 hash。
- [ ] 多根 A/B 首次导入后只配置 A，B 旧记录保持 is_missing=false；B 锁住或报错也保持；A 完整扫描确认移除的文件才在 A 的覆盖范围内标缺失。禁用一个来源、不支持 schema、达到扫描上限、单条损坏引起 incomplete 时，都不能触发误判删除。
- [ ] Pipeline 测试为每个新增来源验证解析内容进入已有持久队列/提炼路径、知识和会话可用唯一短语搜索到；模型调用使用测试替身。未知 provider 不静默映射为已有来源。现有五来源与搜索 PR #45 回归必须持续通过。
- [ ] 日志捕获用合成敏感标记断言正文不会出现：

```rust
let forbidden = "SYNTHETIC_TRANSCRIPT_MARKER_NOT_FOR_LOGS";
assert!(!captured_logs.contains(forbidden));
assert!(!captured_logs.contains("SYNTHETIC_CREDENTIAL_MARKER"));
```

日志包含来源 key、诊断代码、计数、耗时即可。`captured_logs` 由测试的 tracing writer 收集，不是产品日志文件；测试不得读取用户日志。
- [ ] 全部 target 通过后提交 `fix: preserve scoped provider sync and pipeline guarantees`。

## Task 14: 支持矩阵、许可、完整验证与交付

**Files:** `docs/reference-analysis/multi-provider-support.md`、`THIRD_PARTY_NOTICES.md`、`config.example.toml`、`README.md`、`AGENTS.md`、本实施计划的任务勾选状态。不为了通过 CI 修改已发布 migration 或删除失败测试。

- [ ] 对每个来源/存储形态建立矩阵：key、默认根、显式根含义、环境变量、匿名 fixture、参考 commit、parser version、已通过自动测试、真机是否验证、已知限制。Antigravity IDE 无正文证据时必须保留非绿色状态；Copilot 三入口、Cursor 两类存储、Kimi 新旧分别列出。
- [ ] 检查 MIT 复用清单：仅观察格式的模块记录参考，不虚称复制；实际复制/改写的函数保留 copyright、license 和具体文件/commit；剥离未授权扩散读取和伪造消息逻辑。
- [ ] 运行并记录实际退出码：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p aiks-core
cargo check -p aiks-cli
cargo test --workspace
cd apps/aiks-desktop
npm ci
npm test
npm run build
```

Windows compile 和 repository hygiene 使用仓库 `.github/workflows/ci.yml` 现行命令。缺少 Tauri runtime resource 时只在测试环境补占位目录，不改产品资源声明。需 Windows 的 reparse/junction 行为单独运行，不能只 cargo check。
- [ ] 审查 `git diff`：没有用户数据、secret、默认私网 endpoint、无关 dependency 升级、第二套 parser/pipeline 根目录、历史路径重命名或清库操作。全部来源从真正工厂到 UI 的映射有覆盖，而不是枚举有 16 项就算完成。
- [ ] 创建/更新 draft PR，描述精确完成的来源和形态；贴当前 HEAD 的实际 CI，不借用旧提交的绿色状态。完成后提交 `docs: record provider support evidence and acceptance`。
- [ ] 用户确认合并前保持 feature branch，真机验收与自动化验收分别报告，不自动发布安装包。

## 执行记录与计划自审

本计划截至提交时尚未执行产品实现、parser 测试或 CI；空复选框表示未完成，不能据本文档宣称 11 个来源已接入。

覆盖检查：设计中的来源身份/配置→Task 2；只读、预算与路径→Task 1；11 来源与复杂形态→Task 3–11；统一 Desktop/CLI/显示→Task 12；增量、判缺失、Pipeline/Search→Task 13；许可、旧数据/旧来源回归、真机边界→Task 14。五项 Review Focus 都已放入对应任务。

本次没有给新的数据库 migration 预分配编号，因为实现首先复用 source_path 与显式扫描范围；如果实际代码审查证明现有字段不能安全表达覆盖范围，先说明需要持久化的事实和旧库升级测试，再追加正式 migration，不用纯内存猜测替代持久身份。

推荐按 Task 1→2→3 完成第一条 Qwen 导入后验证公共接口，再继续 Task 4–11；最后 Task 12–14 收口。每项完成即在 PR 记录证据，不重复跑与该提交无关的大范围格式化；完整 CI 留给可合并的明确提交节点。
