# AIKS V3.0 Alpha 稳定性修复与知识流水线闭环实施方案

版本：V3.0 Alpha Stabilization Plan  
日期：2026-09-13  
适用项目：AI Knowledge Sync / AIKS Desktop  
适用基线：`49ae827dfa157fd015501564c22cbf4612ae83d7`

## 1. 文档目标

本方案用于合并两类结论：

1. V3 产品目标：将 AIKS 从“Session 同步器 + SiYuan 知识库”收口为统一的知识处理流水线；
2. 代码审查结论：当前项目虽然可构建、已有 84 个测试通过，但同步正确性、桌面生命周期、自动 Pipeline、安全、数据完整性、恢复能力仍存在发布阻断问题。

本轮目标不是继续新增页面，而是先把项目修到：

```text
可靠
可恢复
可观察
可追溯
可调试
```

当前正式定位：

```text
AIKS V3.0-alpha
```

而不是“V3 已完成”。

---

## 2. 当前真实状态

当前项目已经具备：

- 四个 Provider
- Canonical Model
- SQLite State DB
- Tauri Desktop
- Embedded SiYuan
- Session Cleaner
- LLM Chunker
- AI Client
- Pipeline 基础模型
- KnowledgeItem / KnowledgeChunk
- EmbeddingRecord
- FTS5
- 工作记录、知识库、处理中心、搜索页面
- MockAiksApi / TauriAiksApi

但核心链路仍未可靠闭环：

```text
Source
↓
Scan
↓
Parse
↓
Normalize
↓
Persist
↓
Pipeline
↓
Clean
↓
Chunk
↓
AI Extract
↓
Knowledge
↓
Embedding
↓
Index
↓
Search
↓
Trace / Recovery
```

当前最大风险不是“功能少”，而是“功能状态可能不可信”。

---

## 3. 总体实施原则

### 3.1 不推翻重写

继续复用：

```text
Provider
Canonical Model
NormalizedSession
Watcher 基础模块
Sanitizer
Archive
Qwen Client
Tauri Shell
Installer
SQLite
V3 数据模型
MockAiksApi
TauriAiksApi
```

重点修复：

```text
数据库访问安全
同步状态机
SiYuan API 契约
Runtime 生命周期
Watcher 生命周期
自动 Pipeline 入队
Pipeline 持久化与恢复
事务一致性
AI / Embedding 失败语义
设置真实生效
搜索一致性
Provider 正确性
发布验收
```

### 3.2 可靠性优先

暂停优先开发：

```text
更多 Dashboard
更多统计图
知识图谱
多 Agent
团队知识
复杂 Reranker
更多 Provider
```

### 3.3 “完成”必须分四层

以后所有功能必须分别记录：

```text
Implementation
Wired
Tests
Real E2E
```

只有四项都满足，才允许写“完成”。

---

## 4. 里程碑

### M0：稳定性止损
预计 1~2 天

### M1：可靠采集与同步闭环
预计 3~5 天

### M2：可恢复 V3 Pipeline
预计 4~6 天

### M3：兼容性与发布验收
预计 3~5 天

顺序必须：

```text
M0 → M1 → M2 → M3
```

---

# 5. M0：稳定性止损

## 5.1 B01 / P0：StateDb unsafe Sync

当前 `StateDb` 包装单个 `rusqlite::Connection`，并手工声明 `Send + Sync`，但 Worker、Tauri 命令、同步流程可能并发访问。

必须删除无依据的：

```rust
unsafe impl Sync
```

推荐优先采用 DB Actor：

```text
Tauri / Worker / Sync
        ↓
      mpsc
        ↓
单独 DB Thread
        ↓
rusqlite::Connection
```

禁止在持有 DB 锁时等待：

```text
HTTP
AI
Embedding
SiYuan
```

验收：

```text
[ ] 无 unsafe Sync
[ ] UI 查询 + Sync + 多 Pipeline 并发无 panic
[ ] 无数据库未定义共享访问
```

---

## 5.2 B05 / P0：Unicode 安全截断

当前多个模块直接使用：

```rust
&text[..N]
```

对中文、Emoji 会产生 UTF-8 边界 panic。

新增统一：

```text
util/text.rs
```

至少提供：

```rust
truncate_chars()
truncate_utf8_bytes()
truncate_middle()
```

替换：

```text
Renderer
Cleaner
SessionChunker
AI Client
Embedding Client
ToolResult
日志预览
JSON 预览
```

验收：

```text
纯中文
Emoji
中英混合
中文+Emoji
阈值边界
超长 ToolResult
```

全部不 panic。

---

## 5.3 B06 / P0：Secret Sanitizer

明确三个边界：

```text
Persist Boundary
External Send Boundary
Log Boundary
```

需要覆盖：

```text
Authorization
Bearer
api_key
access_key
secret_key
SecretKey
AWS_ACCESS_KEY_ID
AWS_SECRET_ACCESS_KEY
token
password
```

支持：

```text
JSON
env
引号
空格
Unknown JSON
ToolCall
ToolResult
title
path
错误响应
日志
AI Prompt
```

Unknown JSON 建议递归按敏感 key 脱敏。

验收：匿名 Secret fixture 通过所有输出路径。

---

# 6. M1：可靠采集与同步闭环

## 6.1 B02：SiYuan v3.8.3 API 契约

必须修正：

```text
createDocWithMd 成功 data 是 String ID
属性 API 使用 /api/attr/...
禁止依赖不存在的 /api/search/searchAttr
```

优先通过本地 `target_id` 定位。

建立 `MockSiYuanServer`，覆盖：

```text
Create
Attrs
Find
Update
404
500
Timeout
Malformed JSON
```

真实测试 Notebook 验收：

```text
Create
↓
Set Attrs
↓
Find
↓
Update
↓
第二次运行不重复创建
```

---

## 6.2 B03：同步状态机

必须拆分：

```text
observed_hash
synced_hash
target_hash
```

推荐状态：

```text
DISCOVERED
PENDING
SYNCING
SYNCED
RETRY
CONFLICT
MISSING
FAILED
```

正确流程：

```text
Scan
↓
更新 observed_hash
↓
判断是否需要同步
↓
PENDING
↓
远端写正文
↓
写属性
↓
保存本地 target mapping
↓
全部成功
↓
更新 synced_hash
```

失败时绝不能更新 `synced_hash`。

`UNCHANGED` 只有在以下条件同时满足时成立：

```text
observed_hash == synced_hash
target 存在
无 RETRY / FAILED / MISSING
```

---

## 6.3 B04：Conflict

禁止用 `custom-aiks-content-hash` 直接判断用户正文编辑。

正确方案：

```text
AIKS 写入目标
↓
规范化远端正文
↓
保存 target_hash
```

下一次写入前：

```text
读取远端正文
↓
同样规范化
↓
current_target_hash
```

若不同：

```text
CONFLICT
```

UI：

```text
目标内容已被手动修改
[查看差异]
[保留现有]
[使用 AIKS 覆盖]
```

默认不覆盖。

---

## 6.4 B07：Runtime 单一所有权

只允许注册一个：

```rust
ManagedRuntimeState {
    inner: RwLock<Option<SiyuanRuntime>>
}
```

所有：

```text
status
restart
shutdown
tray exit
sink
```

必须操作同一个 Runtime。

Runtime 重启后如果端口变化，必须同步更新：

```text
SiYuan Client
Engine Sink
AppState
UI Status
```

---

## 6.5 B08：Watcher + Periodic Scan

Desktop 必须长期持有：

```rust
SchedulerService {
    watcher_handle,
    periodic_task,
    cancel_token,
    pipeline_handle,
}
```

不能让 WatcherHandle 离开局部作用域自动 Drop。

同时增加周期兜底：

```text
Watcher
+
5 分钟 Periodic Scan
```

SiYuan 离线时：

```text
仍能 Scan
仍能 Persist
仍能 Pending
```

不能因为知识引擎失败导致 Provider 不可用。

---

## 6.6 B10：设置真实生效

统一一个权威：

```text
AppConfig
```

包含：

```text
general
scan
pipeline
ai
embedding
siyuan
security
```

统一：

```text
load_config
save_config
apply_config
```

禁止 UI 保存一套、Engine 用 `Config::default()`。

必须验收：

```text
关闭 AI → 不再外发 AI 请求
修改模型 → 实际请求模型变化
关闭自动扫描 → Scheduler 停止
重启后配置保持一致
```

---

## 6.7 B15：Missing Source

只有某 Provider 完整扫描成功后，才能将该来源未发现的旧 Session 标为：

```text
MISSING
```

Provider 扫描失败时禁止批量标 Missing。

---

## 6.8 Archive

如果：

```text
archive.enabled=true
```

则 NormalizedSession 完成后必须写：

```text
archive/{source}/{session-id}.json.gz
```

不能仅存在 Archive 模块却不进入生产链路。

---

## 6.9 CLI 语义

### dry-run

必须返回：

```text
NEW
UPDATED
UNCHANGED
RETRY
CONFLICT
```

且没有任何成功基线副作用。

### resync

必须使用：

```text
source + session_id
```

或内部主键。

### rebuild-state

禁止直接删除整个 `aiks.db`。

拆分：

```text
rebuild-sync-index
reset-local-knowledge
```

后者必须高危确认。

---

# 7. M2：可恢复 V3 Pipeline

## 7.1 B09：自动入队必须真实存在

统一生产入口：

```rust
sync_and_enqueue_pipeline()
```

流程：

```text
Scan
↓
Raw Persist / Sync
↓
Candidate
↓
Create Persistent Pipeline Job
↓
Job Insert Success
↓
queued += 1
```

UI `queued` 必须来自真实 DB Job 数，禁止用 `candidates.len()` 冒充。

---

## 7.2 Persistent Job Queue

新增：

```sql
CREATE TABLE pipeline_job (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    external_session_id TEXT NOT NULL,
    source_hash TEXT NOT NULL,
    generation INTEGER NOT NULL,
    status TEXT NOT NULL,
    attempt INTEGER NOT NULL DEFAULT 0,
    available_at TEXT,
    lease_until TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

状态：

```text
PENDING
RUNNING
RETRY_WAIT
SUCCESS
FAILED
CANCELLED
SUPERSEDED
```

---

## 7.3 B13：并发上限

真正使用：

```text
Semaphore
```

默认：

```text
max_concurrent = 1
```

禁止：

```text
unbounded_channel + 每 Job tokio::spawn
```

---

## 7.4 同 Session Single Flight

身份：

```text
source + external_session_id
```

同一 Session 同时仅允许一个活跃 Job。

新 hash 到来时：

```text
旧 generation
↓
SUPERSEDED
```

---

## 7.5 Generation

Session 内容变化：

```text
generation + 1
```

以下对象都挂 generation：

```text
KnowledgeItem
KnowledgeChunk
Embedding
FTS
```

---

## 7.6 B11：事务化知识更新

禁止先删父表再插入。

推荐：

```text
BEGIN
↓
写新 generation KnowledgeItems
↓
写 KnowledgeChunks
↓
写 Embeddings
↓
写 FTS
↓
切换 active_generation
↓
COMMIT
```

失败：

```text
ROLLBACK
```

上一版有效 Knowledge 保持可用。

---

## 7.7 B12：统一失败终态

任何阶段失败：

```text
Parse
Load
Clean
Chunk
AI
Knowledge Save
Embedding
Index
```

必须进入明确终态。

禁止：

```text
? return
↓
数据库永久 PROCESSING
```

---

## 7.8 Restart Recovery

启动时扫描：

```text
RUNNING
PROCESSING
lease expired
```

恢复成：

```text
RETRY_WAIT
```

重新入队。

---

## 7.9 Attempt

每次重试：

```text
attempt + 1
```

Stage Run 保留：

```text
Attempt 1
Attempt 2
...
```

不能覆盖历史。

---

## 7.10 B17：AI Failure 语义

必须区分：

```text
NO_KNOWLEDGE
AI_OUTPUT_INVALID
AI_TIMEOUT
AI_RATE_LIMIT
AI_NETWORK_ERROR
```

不能把 JSON 解析失败当：

```text
RAW_ONLY
```

推荐：

```rust
enum ExtractionOutcome {
    Success(Vec<KnowledgeItem>),
    NoKnowledge,
    RetryableError(...),
    PermanentError(...),
}
```

---

## 7.11 B23：Embedding 参数校验

保存配置时：

```text
batch_size > 0
target > 0
0 <= overlap < target
target <= max
```

必须成立。

LLM Chunker 对单条超长 Message 必须支持二次拆分。

Embedding 返回必须校验：

```text
数量
维度
顺序
```

---

## 7.12 Embedding Failure

如果所有 Batch 失败：

```text
EMBEDDING_FAILED
```

禁止继续：

```text
INDEXED
READY
```

如果 Embedding 被禁用：

```text
READY_TEXT_ONLY
```

---

## 7.13 AI Request Log

每次真实请求记录：

```text
request_id
pipeline_run_id
stage
model
endpoint
input_tokens
output_tokens
latency_ms
http_status
retry_count
status
error_type
created_at
```

未知 token：

```text
NULL
```

禁止用 chunk 数冒充 token 数。

---

# 8. Search 统一

## 8.1 一个 SearchService

统一入口：

```rust
search(query, scope, filters)
```

禁止前端一个页面调 `search_knowledge`，Core 又有另一个 `hybrid_search`。

---

## 8.2 FTS 一致性

Knowledge generation 更新时：

```text
旧 FTS 删除
新 FTS 写入
```

必须与 Knowledge 更新放在同一事务。

禁止 orphan FTS。

---

## 8.3 Filter

真实支持：

```text
project
category
source
date
```

结果与 total 必须使用同一 predicate。

---

## 8.4 特殊字符

必须验证：

```text
C++
C#
"foo"
a:b
*
+
-
```

不能因为 FTS MATCH 语法错误返回空结果。

---

## 8.5 排序稳定

FTS rank 必须保留。

Hybrid Search 后续：

```text
keyword_score
+
vector_score
```

稳定融合。

禁止 HashMap 非确定顺序影响结果。

---

# 9. Provider 正确性

## 9.1 OpenCode

修复 Tool Error：

```text
state.error
```

非图片 File：

```text
FileReference / Unknown
```

禁止静默跳过。

---

## 9.2 Codex

真正应用：

```text
include_archived_sessions
```

保留完整：

```text
rollout_path
```

禁止只保留文件名后重新全目录遍历。

---

## 9.3 Unknown Event

Claude / Codex / Gemini：

未知顶层事件默认保留：

```text
metadata
或
Unknown block
```

不能大量 silent skip。

---

# 10. Hash 体系

拆分：

```text
raw_source_hash
canonical_hash
render_hash
```

`canonical_hash` 必须覆盖：

```text
title
project
messages
tools
files
timestamps
metadata
```

`render_hash` 必须纳入：

```text
renderer_version
sanitizer_version
render_config
```

这样渲染规则变化也能触发重建。

---

# 11. Incremental Scanner

当前增量 Scanner 不能只存在于单测。

生产调度要真正使用：

```text
source_path
mtime
size
last_offset
parser_version
hash
```

Pipeline Job 必须携带：

```text
source
session_id
source_path
```

禁止每处理一条 Session 都重新 discover 整个 Provider。

---

# 12. UI 真实化

## 工作记录

真实显示：

```text
source
session_id
title
message_count
project
updated_at
pipeline_status
knowledge_count
```

禁止 hardcode `knowledge_count=0`。

## 处理中心

必须来自：

```text
pipeline_job
pipeline_run
pipeline_stage_run
```

## 知识库

必须读取：

```text
active generation
```

## 搜索

只调用统一：

```text
SearchService
```

---

# 13. 健康状态

禁止：

```text
enabled=true
↓
显示 Ready
```

必须真实检查：

```text
SiYuan Health
AI Health
Embedding Health
DB Health
```

且带：

```text
last_checked_at
```

---

# 14. M3：兼容性与发布验收

## 14.1 Provider Golden Tests

每个 Provider：

```text
fixture
↓
parse
↓
canonical JSON
↓
golden JSON
```

覆盖：

```text
正常
Unknown
Tool Error
附件
归档
坏数据
parser_version
```

---

## 14.2 OpenCode WAL

真实模拟：

```text
OpenCode SQLite 正在写
+
AIKS 只读
```

要求：

```text
不阻塞 OpenCode
可读取已提交 WAL
```

---

## 14.3 1k / 10k Benchmark

测：

```text
Scan Time
Peak Memory
DB Size
Queue Length
UI Load Time
Search Time
```

---

## 14.4 Windows 生命周期

必须真实测：

```text
首次安装
启动
托盘
退出
重启
覆盖安装
升级
卸载
```

---

# 15. 升级安装生命周期

继续保留之前发现的问题：

```text
SiYuan-Kernel.exe 正在运行
↓
Installer 无法覆盖
```

升级必须：

```text
检测旧 AIKS
↓
优雅退出
↓
停止 Watcher
↓
停止 Scanner
↓
停止 Pipeline Worker
↓
停止 AI/Embedding 请求
↓
等待 DB flush
↓
停止 SiYuan Runtime
↓
等待
↓
必要时安全强杀
↓
安装
```

禁止：

```text
taskkill /IM SiYuan-Kernel.exe
```

误杀用户独立 SiYuan。

必须只终止 AIKS 自己持有 PID 的 Runtime。

---

# 16. 开发调试流程

日常前端：

```powershell
cd apps\aiks-desktop
$env:VITE_AIKS_MOCK="true"
npm run dev
```

Tauri Dev：

```powershell
npm run tauri dev
```

或：

```powershell
.\scripts\dev.ps1
```

Release 只有正式发布时：

```powershell
.\scripts\build-release.ps1
```

禁止再用 Installer 调页面。

---

# 17. Pipeline Debug CLI

补齐：

```text
aiks sessions list
aiks session show <id>
aiks pipeline run <id>
aiks pipeline status <id>
aiks pipeline retry <id> --from ai
aiks chunk session <id>
aiks extract <id>
aiks embed <id>
aiks search "<query>"
```

---

# 18. 真实 E2E 基准对象

固定：

```text
OpenCode ses_f717
```

完整验收链：

```text
OpenCode SQLite
↓
Provider Load
↓
约 140 Messages
↓
NormalizedSession
↓
Persist Raw
↓
Create Pipeline Job
↓
Clean
↓
LLM Chunk
↓
Qwen3.8-27B
↓
0~N KnowledgeItems
↓
KnowledgeChunk
↓
Embedding（启用时）
↓
Index
↓
Search
↓
Source Trace
```

UI 每一步可见。

---

# 19. AI 真实验收

```text
Base URL:
http://127.0.0.1:11434/v1

Model:
Qwen3.8-27B
```

必须测试：

```text
Health
Chat Completion
Structured JSON
Long Session
Timeout
Retry
Malformed JSON
No Knowledge
```

---

# 20. Embedding 验收

如果尚未有 Embedding Model：

允许：

```text
Embedding = DISABLED
```

UI 必须显示：

```text
未配置
```

不能显示成功。

---

# 21. 最低回归用例

至少补齐：

```text
dry-run → real
failed → retry
body success + attrs failure
manual remote edit
target deleted
provider unavailable
session malformed
unknown event
source removed / restored
parser version upgrade
active WAL
CJK truncation
Emoji truncation
secret propagation
same session repeated extraction
knowledge + embedding regeneration
zero knowledge
transaction rollback
duplicate enqueue
slow AI
worker crash
process restart
config persistence
watcher lifecycle
periodic fallback
runtime restart
FTS special chars
category filter
orphan FTS cleanup
rebuild-state preservation
```

---

# 22. 测试层次

必须区分：

```text
Unit Tests
Integration Tests
E2E Tests
Manual Release Acceptance
```

Unit：模块逻辑。

Integration：

```text
DB
MockSiYuan
MockAI
MockEmbedding
```

E2E：

```text
真实 OpenCode
真实 SiYuan
真实 Qwen
真实 Embedding
```

Manual Release：

```text
Installer
Tray
Upgrade
Uninstall
```

---

# 23. CI / Release Gate

至少：

```text
cargo fmt --check
cargo clippy
cargo test --workspace
npm run build
```

后续补：

```text
frontend test
integration test
```

---

# 24. 项目约束治理

当前 V3 实现方向与旧 `AGENTS.md` 存在冲突：

```text
旧约束：
禁止自研 Embedding / Search / Knowledge UI
```

而 V3 已经实现：

```text
Embedding
FTS5
Knowledge UI
```

必须增加：

```text
ADR-V3-Architecture.md
```

明确 AIKS V3 是否正式负责：

```text
Embedding
FTS
Vector Index
Knowledge UI
Search
```

如果正式采用 V3：

同步更新：

```text
AGENTS.md
README.md
acceptance.md
config.example.toml
```

SiYuan 定位：

```text
Legacy / Optional Export
```

---

# 25. 完成状态文档模板

以后所有功能都用：

```markdown
## Feature Name

Implementation: ✅
Wired: ✅
Tests: ✅
Real E2E: ❌
Release Accepted: ❌
```

禁止再用一个“完成”掩盖差异。

---

# 26. PR 拆分建议

| PR | 范围 | 验证 |
|---|---|---|
| PR1 | DB 安全、Unicode、Sanitizer | B01/B05/B06 |
| PR2 | SiYuan 契约、Sync State、Conflict | Create/Update/Retry/Conflict |
| PR3 | Runtime、Config、Watcher、Periodic | Desktop 生命周期 |
| PR4 | Archive、Missing、Assets、CLI Recovery | 数据恢复 |
| PR5 | Persistent Queue、Generation、Transaction | Pipeline 可恢复 |
| PR6 | AI/Embedding Error + Config Validation | 模型失败 |
| PR7 | Search / FTS / Filter | 索引一致 |
| PR8 | Provider Golden / WAL / Hash / Incremental | Provider 稳定 |
| PR9 | Windows Installer / Upgrade / Release Gate | 发布验收 |

规则：

```text
先写失败测试
↓
确认复现
↓
修复
↓
验证
↓
更新文档
```

禁止大爆炸式一次重写全部模块。

---

# 27. 优先级汇总

## P0

```text
B01 StateDb unsafe Sync
B05 Unicode Panic
B06 Secret Sanitizer
```

## P1

```text
B02 SiYuan API
B03 Hash 提前提交
B04 Conflict
B07 Runtime State
B08 Watcher / Periodic
B09 Pipeline 自动入队
B10 Settings 不生效
B11 Knowledge FK / Transaction
B12 Worker Recovery
B13 Concurrency
B14 rebuild-state
B23 Embedding Config
```

## P2

```text
B15 Missing / Archive / Assets
B16 FTS / Search
B17 AI/Embedding 状态
B18 Provider Compatibility
B19 Hash Coverage
B20 Incremental Scanner
B21 Filter / Status
B22 CLI Semantics
```

---

# 28. M0 Exit Criteria

```text
[ ] 无 unsafe Sync
[ ] 中文 / Emoji 截断不 panic
[ ] Sanitizer 回归覆盖主要路径
[ ] DB 备份恢复方案存在
```

---

# 29. M1 Exit Criteria

完整脚本：

```text
首次创建
↓
第二次不变
↓
源追加
↓
远端手改
↓
CONFLICT
↓
显式覆盖
↓
SiYuan Offline
↓
恢复
↓
自动补同步
↓
重启
↓
源删除
↓
MISSING
```

全部通过。

---

# 30. M2 Exit Criteria

```text
[ ] Pipeline Job 真实入库
[ ] UI queued == DB job 数
[ ] 并发上限真实生效
[ ] 同 Session 不重复并发
[ ] AI 失败明确 FAILED
[ ] Embedding 失败明确 FAILED
[ ] 进程中断可恢复
[ ] 重提炼事务化
[ ] 上一代 Knowledge 不因失败丢失
[ ] Search 无 orphan index
```

---

# 31. M3 Exit Criteria

```text
[ ] 四 Provider Golden
[ ] OpenCode WAL
[ ] 1k Benchmark
[ ] 10k Benchmark
[ ] Windows Install
[ ] Tray
[ ] Upgrade
[ ] Uninstall
[ ] Release Gate
[ ] acceptance.md 有真实证据
```

---

# 32. 版本策略

建议：

```text
V2.5
只修严重 Bug

V3.0-alpha
当前阶段

V3.0-beta
完成 M0~M3 后进入内部试用

V3.0
内部试用稳定后发布
```

---

# 33. 本轮明确暂停

在 M1 完成前暂停：

```text
SiYuan 完全移除
新的 Knowledge UI 大改
Embedding 展示增强
更多统计图
知识图谱
团队功能
更多 Provider
Agent
```

---

# 34. 最终目标

AIKS V3 的目标不是：

```text
页面多
模块多
测试数量多
能打包
```

而是：

```text
数据不会丢
状态不会假
失败可以恢复
自动链路真实存在
AI / Embedding 失败可解释
处理过程可以追踪
搜索结果可以回溯
升级不会破坏数据
调试不需要频繁打 EXE
```

下一次里程碑统一定义为：

> 可靠同步与可恢复知识处理流水线通过真实 E2E 验收。

只有当：

```text
OpenCode Session
↓
Reliable Ingestion
↓
Recoverable Pipeline
↓
Knowledge
↓
Embedding / Search
↓
Source Trace
```

可以真实跑通，并且在离线、失败、重启、重复处理、用户修改、升级等场景下仍然正确，AIKS 才进入可用于公司内部试用的阶段。
