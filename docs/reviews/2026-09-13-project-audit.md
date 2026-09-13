# AIKS 项目审查：问题、Bug 与后续实施规划

- 审查日期：2026-09-13
- 审查目录：E:/Project/Htrs/AISK
- 代码基线：`49ae827dfa157fd015501564c22cbf4612ae83d7`
- 实际版本：Cargo / npm / Tauri 均为 `0.2.0`；设计文档称为 V3 Phase A–E。
- 审查方式：代码与调用链检查、现有测试、匿名数据复现、依赖源码及 SiYuan v3.8.3 官方接口核对。
- 本次只做审查与文档交付，没有修改产品实现，没有读写真实 Session 内容或用户思源知识库。

## 1. 总体结论

**当前项目已具备较完整的模块和桌面界面，但尚不能按“可靠自动知识同步工具”验收，也不宜根据“84 个测试通过、安装包可构建”判断 V3 全链路完成。**

主要障碍集中在四处：

1. **同步正确性没有闭环。** 思源接口契约错误、失败后不重试、冲突检测无效、Missing Source 未接入。
2. **桌面生命周期与真实功能脱节。** Watcher 提前释放、没有周期扫描、运行时状态对象注册失败、设置只保存不执行。
3. **V3 自动链路没有真正接通。** 自动入队方法没有生产调用者；按钮返回的“已入队数量”只是候选数量。手动运行后的重复提炼、错误状态与恢复也有缺陷。
4. **安全和数据完整性存在发布阻断项。** SQLite 连接被不安全地跨线程共享，中文截断可以 panic，脱敏存在旁路，重建状态会删除 V3 知识数据。

建议暂缓“SiYuan 弱化、版本号升级、更多界面功能”，先完成稳定性修复和端到端验收。已有模块可以保留并逐步收敛，不建议推倒重写。

### 已有的有效基础

- 四个 Provider 已有实现，输出统一的 Canonical Model。
- Provider 没有直接调用 SiYuan；OpenCode/Codex 的源 SQLite 以只读标志打开。
- SQLite 状态表、Markdown Renderer、Secret Sanitizer、Archive、CLI Daemon 已有代码。
- CLI Daemon 同时持有 Watcher 和周期扫描。
- V3 清洗、切片、AI 提炼、知识条目、向量记录、详情页已存在。
- 构建链可工作，适合在现有代码上补齐可靠性，而非继续堆叠第二套实现。

## 2. 验证结果与证据边界

| 检查 | 本次结果 | 能证明什么 |
|---|---|---|
| `cargo test --workspace` | 退出码 0；core 84 个测试通过；CLI/桌面端均为 0 个测试 | 当前测试和测试目标可编译运行 |
| `npm run build`，目录 `apps/aiks-desktop` | 退出码 0；TypeScript + Vite 构建成功 | 前端类型检查和产物构建通过 |
| 13 个临时匿名审计探针 | 13/13 均确认了预期的错误行为 | 下文标注“复现”的问题确实存在 |
| SiYuan v3.8.3 官方源码 | 核实创建响应和属性路由 | 当前 Sink 与声明绑定版本的契约不一致 |
| Tauri / rusqlite 本机锁定依赖源码 | 核实 manage 不替换已有状态、Connection 不是 Sync | 生命周期和并发结论有依赖实现依据 |
| 真实思源、公司 AI、Embedding 端到端 | 未执行 | 不能声称真实全链路通过 |
| Windows 安装/升级/卸载、长时间运行 | 未执行 | 历史安装包记录不代替本次发布验收 |

测试过程中使用临时目录、匿名内存模型、独立 SQLite 和本地回环 HTTP 响应。没有调用公司 AI，也没有向真实思源写文档。

审计探针断言的是“当前错误行为存在”，因此探针通过不代表产品正确。完整探针及复现说明保存在 [审计复现附件](E:/Project/Htrs/AISK/docs/reviews/2026-09-13-audit-reproduction.md)。产品测试目录内的临时探针已移除。

## 3. 发布阻断及高优先级问题

优先级：P0 = 必须立即处理的安全/数据完整性基础问题；P1 = 发布前必须修复的关键功能问题；P2 = 随后的正确性、覆盖和维护问题。未执行的压力或故障注入场景不会写成“已发生事故”。

### B01 · P0 · StateDb 的 unsafe Sync 没有成立的同步保护

**证据：** [storage/db.rs:22](E:/Project/Htrs/AISK/crates/aiks-core/src/storage/db.rs:22)、[engine/mod.rs:167](E:/Project/Htrs/AISK/crates/aiks-core/src/engine/mod.rs:167)、[pipeline/worker.rs:77](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/worker.rs:77)。

`StateDb` 包装单个 `rusqlite::Connection`，手工声明 `Send + Sync`。注释称 AiksEngine 用 Mutex 串行访问，但实际是 `Arc<StateDb>`，`conn()` 直接返回共享引用。Worker 为每个任务 spawn，Tauri 命令也可并发访问同一连接。

本机 rusqlite 0.31.0 的 Connection 内部使用 RefCell，本身只实现 Send，没有实现 Sync；默认打开标志还包含 NO_MUTEX。WAL 不能解决 Rust 内部共享访问安全性。

**影响：** 并发任务会进入不受保证的共享访问，存在数据竞争、panic、未定义行为风险。本次未通过故意制造 UB 来验证崩溃，结论来自明确的安全契约违反。

**建议：** 删除无依据的 unsafe Sync；采用单独 DB actor/连接线程，或明确受锁保护的短事务访问。不要在持有连接锁时 await HTTP。之后再增加 Pipeline 并发。

**验收：** UI 查询、同步与多个 Pipeline 同时运行，不共享未加锁的 Connection；不靠 unsafe 绕过编译器。

### B02 · P1 · SiYuan Sink 与 v3.8.3 API 契约不匹配

**证据：** [sink/siyuan.rs:150](E:/Project/Htrs/AISK/crates/aiks-core/src/sink/siyuan.rs:150)、[193](E:/Project/Htrs/AISK/crates/aiks-core/src/sink/siyuan.rs:193)、[246](E:/Project/Htrs/AISK/crates/aiks-core/src/sink/siyuan.rs:246)、[272](E:/Project/Htrs/AISK/crates/aiks-core/src/sink/siyuan.rs:272)。

- `createDocWithMd` 的成功 `data` 是字符串 ID，当前按 `{ id: ... }` 解析。
- 属性读写使用了 `/api/block/getBlockAttrs`、`/api/block/setBlockAttrs`，官方路由是 `/api/attr/...`。
- 当前调用的 `/api/search/searchAttr` 不在 v3.8.3 官方 router 中。

已用返回 `{"code":0,"msg":"","data":"20260913000000-auditxx"}` 的回环服务器复现：HTTP 成功，客户端仍报反序列化失败。即使服务端已创建文档，客户端也无法完成记录和属性设置。

官方依据：[v3.8.3 filetree.go，createDocWithMd](https://github.com/siyuan-note/siyuan/blob/v3.8.3/kernel/api/filetree.go#L876)、[v3.8.3 router.go](https://github.com/siyuan-note/siyuan/blob/v3.8.3/kernel/api/router.go#L315)。

**建议：** 按固定版本重建 API 契约测试，修正返回类型和路由；优先用已保存 target_id 定位文档，再实现属性恢复查询。区分“未找到”和“网络/鉴权/接口失败”。

**验收：** 在独立测试 Notebook 完成 Create → Attrs → Find → Update；第二次运行不重复创建。

### B03 · P1 · 源 hash 提前提交，使失败任务永久被当作 UNCHANGED

**证据：** [sync/engine.rs:262](E:/Project/Htrs/AISK/crates/aiks-core/src/sync/engine.rs:262)、[297](E:/Project/Htrs/AISK/crates/aiks-core/src/sync/engine.rs:297)。

当前先 upsert 源 hash，再执行远端请求；下次只看源 hash/parser version 就直接返回 UNCHANGED，没有检查目标是否存在、是否成功、是否待重试。

**已复现：**

- 第一次连接不可用端点：`failed=1`；第二次相同输入：`unchanged=1`，不再尝试同步。
- 直接调用 Core 的 dry-run：写入源状态但无 target；随后正式同步：`unchanged=1`、target 数量为 0。

**需要区分：** 当前 CLI 的 `sync --dry-run` 提前返回，只扫描数量，没有进入上述 Core dry-run 分支；它自身的问题是没有做 NEW/UPDATED 差异预览。不能把 Core 的状态污染直接说成当前 CLI 已触发的行为。

**建议：** 分离 observed hash、成功 synced hash 和实际 target hash。源扫描可以持久化，但 UNCHANGED 必须同时满足目标已同步且无需重试；dry-run 不改变成功基线。先建立 Pending，再执行可恢复写入。

**验收：** 离线 → 恢复自动补同步；dry-run 后首次正式运行仍为 NEW；属性写失败、目标丢失、parser 升级均可恢复。

### B04 · P1 · 冲突检测比较自定义源 hash，无法识别用户正文编辑

**证据：** [sync/engine.rs:306](E:/Project/Htrs/AISK/crates/aiks-core/src/sync/engine.rs:306)、[313](E:/Project/Htrs/AISK/crates/aiks-core/src/sync/engine.rs:313)、[368](E:/Project/Htrs/AISK/crates/aiks-core/src/sync/engine.rs:368)。

程序比较思源自定义属性 `custom-aiks-content-hash` 与本地 synced_hash。该属性由 AIKS 设置，用户修改正文不会让它自动变成正文 hash。保存的 target_hash 也直接复制源 hash，没有读取远端内容。

同时，文档查找错误被吞成 None，属性读取失败跳过检查，属性设置失败仅告警，mark_synced 的错误被忽略。即使修好 B02，这一逻辑仍不能保护手工修改。

**影响：** 远端编辑可能被静默覆盖；查询失败可能走创建分支；文档缺少恢复属性仍被报告成功。

**建议：** 对远端正文做稳定规范化并计算目标基线；写前校验，失败时停止覆盖；只有正文、属性和本地映射都成功才标记 SYNCED。

**验收：** 手工改正文、源新增消息后进入 CONFLICT；只有显式 overwrite 才覆盖；属性接口失败不能返回成功。

### B05 · P1 · 中文/Emoji 长文本截断可 panic

**证据：** [renderer/markdown.rs:150](E:/Project/Htrs/AISK/crates/aiks-core/src/renderer/markdown.rs:150)、[pipeline/session_chunker.rs:58](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/session_chunker.rs:58)、[cleaner.rs:120](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/cleaner.rs:120)、[ai/client.rs:142](E:/Project/Htrs/AISK/crates/aiks-core/src/ai/client.rs:142)。

多处把“字符数”当成 UTF-8 字节偏移，直接使用 `&text[..N]`。类似代码也在旧 ai/chunker、ai/extractor 和 EmbeddingClient 中。

**已复现：** 默认 Renderer 处理 4,000 个“中”组成的 ToolResult，在 10,000 字节边界 panic；V3 Chunker 处理 1,000 个“中”，在 2,000 字节边界 panic。

**影响：** 普通中文日志就能让一个同步调用整体中断，或者让后台任务卡在 PROCESSING。Provider 的 Result 隔离不能自动捕获 panic。

**建议：** 统一 Unicode 安全截断工具，明确按 chars 还是 bytes 限制；所有中文工具结果、错误响应和 JSON 预览共用它。

**验收：** CJK、Emoji、混合文本在阈值前后均不 panic；单任务异常不拖垮整个同步批次。

### B06 · P1 · Secret Sanitizer 有字段与输出路径旁路

**证据：** [util/sanitizer.rs:60](E:/Project/Htrs/AISK/crates/aiks-core/src/util/sanitizer.rs:60)、[renderer/markdown.rs:172](E:/Project/Htrs/AISK/crates/aiks-core/src/renderer/markdown.rs:172)、[pipeline/session_chunker.rs:122](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/session_chunker.rs:122)、[pipeline/ai_stage.rs:129](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/ai_stage.rs:129)。

**已复现：**

- 开启脱敏时，Unknown JSON 中的 password 仍原样进入 Markdown。
- `AWS_SECRET_ACCESS_KEY=...`、`SecretKey=...`、带引号的 password 值、`token = ...` 均能绕过当前正则。

另外，标题/路径/文件引用没有统一脱敏；V3 chunks 在 AI 前脱敏之前就写入 SQLite；Map-Reduce 最终 prompt 又加入未脱敏标题/项目字段；API 错误体、搜索查询直接进入错误或日志。

**影响：** “默认脱敏”不能覆盖全部持久化和外发路径。这里只用匿名标记验证，没有声称真实 Secret 已泄漏。

**建议：** 在持久化、日志和外发三个边界统一处理；支持结构化敏感字段及常见 env 键、引号和空白；截断之前先脱敏。产品需明确本地原文缓存的隐私策略。

**验收：** 同一组匿名凭据从 title、text、Unknown、ToolCall、ToolResult、错误响应输入，所有规定脱敏的输出均不含原值。

### B07 · P1 · Tauri 真实 Runtime 没有替换占位对象

**证据：** [src-tauri/src/lib.rs:26](E:/Project/Htrs/AISK/apps/aiks-desktop/src-tauri/src/lib.rs:26)、[lifecycle.rs:137](E:/Project/Htrs/AISK/apps/aiks-desktop/src-tauri/src/lifecycle.rs:137)、[commands.rs:238](E:/Project/Htrs/AISK/apps/aiks-desktop/src-tauri/src/commands.rs:238)。

Builder 先 manage 一个 runtime_root 为“.”的占位 Runtime；startup 再次以相同类型 manage 真实 Runtime。Tauri 2.11.5 的 manage 在该类型已存在时返回 false，不替换；返回值被忽略。

**影响：** 状态查询、重启、退出操作获取的是占位对象；真实已启动 Kernel 没有按预期被这些命令管理。重启还没有同步更新引擎固定的 base_url 和 AppState URL。

**建议：** 只注册一次共享容器，在该容器内设置真实 Runtime；运行时重启后统一更新 Sink 连接。启动/退出持有明确的资源所有权。

**验收：** 启动、查询、重启、托盘退出操作同一 PID；退出没有遗留受管 Kernel；端口改变后同步仍可用。

### B08 · P1 · 桌面自动同步缺少有效 Watcher 和周期兜底

**证据：** [lifecycle.rs:218](E:/Project/Htrs/AISK/apps/aiks-desktop/src-tauri/src/lifecycle.rs:218)、[watcher/mod.rs:137](E:/Project/Htrs/AISK/crates/aiks-core/src/watcher/mod.rs:137)、[lifecycle.rs:251](E:/Project/Htrs/AISK/apps/aiks-desktop/src-tauri/src/lifecycle.rs:251)。

WatcherHandle 存在 `if let Ok(_handle)` 局部作用域中，没有移入 task 或长期状态；退出该作用域就 Drop，底层 Debouncer 随之停止。桌面 startup 没有周期 scanner。CLI daemon 的长期 handle 和 interval 是存在的，不应与桌面问题混为一谈。

SiYuan 缺失/启动失败时桌面 AppState 的 engine 为 None，连 scan 也无法使用，不符合离线仍可采集的要求。

**建议：** 桌面和 CLI 共用长期运行的调度服务；持有 watcher、interval、取消信号与任务句柄。Provider/DB 初始化独立于思源可用性。

**验收：** 启动后持续新增/修改/删除 Session 均可处理；故意丢失 watcher 事件仍由周期扫描发现；思源离线仍能扫描并排队。

### B09 · P1 · “同步并提炼”没有实际提交 V3 Job

**证据：** [commands.rs:418](E:/Project/Htrs/AISK/apps/aiks-desktop/src-tauri/src/commands.rs:418)、[engine/mod.rs:475](E:/Project/Htrs/AISK/crates/aiks-core/src/engine/mod.rs:475)、[lifecycle.rs:179](E:/Project/Htrs/AISK/apps/aiks-desktop/src-tauri/src/lifecycle.rs:179)。

`sync_and_extract` 调用的是 `engine.sync()`，之后只打印 Queuing 日志并返回 candidates.len()。实际 `sync_and_enqueue_extraction()` 在仓库生产代码中只有定义，没有调用。启动、托盘、watcher、CLI 同样只走 raw sync。

手动 `run_pipeline_for_session` 有真实入队，因此不是 Worker 完全不存在，而是自动入口断开。

此外，待接入方法只按 external_session_id 查询，未带 source，不符合跨 Provider 复合身份边界。

**建议：** 统一 sync orchestration 入口；使用 source + session_id 或内部主键传递候选；只有持久化任务成功才增加 queued。补充既有 Session 的 backfill，否则修好入口后历史 UNCHANGED 仍没有 Pipeline。

**验收：** 首次扫描/按钮/文件变化都创建真实任务；UI 数量等于持久化 Job 数；相同 ID 的不同来源不串线。

### B10 · P1 · 设置保存成功，但运行配置不生效

**证据：** [commands.rs:204](E:/Project/Htrs/AISK/apps/aiks-desktop/src-tauri/src/commands.rs:204)、[lifecycle.rs:116](E:/Project/Htrs/AISK/apps/aiks-desktop/src-tauri/src/lifecycle.rs:116)、[SettingsPage.tsx:24](E:/Project/Htrs/AISK/apps/aiks-desktop/src/pages/SettingsPage.tsx:24)。

设置写到 config/app.json，启动 AiksEngine 却一直 config_path=None，读取 Config::default()，没有把 AppSettings 应用到引擎。重启也不会应用这些配置。

AI 地址/模型是另外两个 React state，保存只发送 settings，因此这两个输入甚至不持久化。自启动选项也没有调用插件 enable/disable。“关闭后驻留”是固定 true 的空回调。

**影响：** 用户关闭自动同步或 AI 后，实际行为不受该开关控制；界面显示“已保存”具有误导性。

**建议：** 一个权威配置模型、一个保存入口；明确即时生效与需重启字段；引擎、worker、watcher、插件都消费该配置。

**验收：** 保存前后和重启后的配置一致；关闭 AI 后没有新外发请求；修改模型/地址可观察到真实请求变化。

### B11 · P1 · 已有知识切片时，重复提炼必然触发外键失败

**证据：** [knowledge_repo.rs:28](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/knowledge_repo.rs:28)、[159](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/knowledge_repo.rs:159)、[002_v3_pipeline.sql](E:/Project/Htrs/AISK/crates/aiks-core/migrations/002_v3_pipeline.sql)。

save_items 先删除 knowledge_item，但 knowledge_chunk 引用它且无 ON DELETE CASCADE。已有 embedding_record 时，删除 chunk 也有相同问题。外键在 StateDb 中已启用。

**已复现：** 保存 item → 保存 chunk → 再 save_items，直接返回 FOREIGN KEY constraint failed。默认即使 Embedding 禁用，Worker 也会生成知识切片，因此该问题不只影响启用向量的用户。

多步删除/插入也没有事务；没有子表时中途失败会留下不完整结果。

**建议：** 事务内按依赖顺序更新，或设计级联与稳定 ID；用 generation 原子切换防止读到半成品。不要用关闭外键绕过。

**验收：** 同一 Session 追加后可再次提炼；知识/切片/向量/全文索引一致；失败保留上一版可用结果。

### B12 · P1 · Worker 错误与重启恢复不闭环

**证据：** [worker.rs:112](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/worker.rs:112)、[repo.rs:27](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/repo.rs:27)、[worker.rs:265](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/worker.rs:265)。

PARSE/discover/load/save_chunks 等用 ? 直接退出，外层只记录日志，没有统一 mark_failed；panic 也没有监督收敛。任务在内存 channel 中，启动不重载未完成记录；重试重用 run_id，不清理旧阶段、finished_at，阶段 INSERT OR REPLACE 实际用新 UUID 追加。

**已复现：** 提交找不到 Provider 的任务，后台报错后数据库仍是 PROCESSING。

**建议：** 持久化 Job、attempt、租约与统一终态处理；启动恢复挂起任务；区分可重试/永久失败；每次 attempt 独立阶段记录；退出等待或安全中断。

**验收：** 每个阶段注入错误都进入明确终态；强制退出后可恢复；重试不会混入旧成功阶段或旧完成时间。

### B13 · P1 · Worker 无并发上限，也没有同 Session 去重

**证据：** [worker.rs:46](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/worker.rs:46)、[77](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/worker.rs:77)、[ai/config.rs:21](E:/Project/Htrs/AISK/crates/aiks-core/src/ai/config.rs:21)。

unbounded_channel 加每个 Job tokio::spawn，100ms 延迟只是启动速率，不是并发限制；max_concurrent=1、debounce_minutes=10 没有被 V3 消费。重复点击可并发操作同一 Session 的 chunks 和知识条目。

**影响：** 慢 AI 下会积累大量任务和连接；同一会话的不同版本互相覆盖，且放大 B01。尚未做容量压测，不给出实际内存峰值。

**建议：** 有界队列、Semaphore、同 Session 单飞、按 hash 合并任务；只允许对应 generation 的结果提交。真正实现配置的 debounce。

**验收：** 模拟慢服务时在途请求不超过配置；重复入队无重复处理；新版本不会被旧任务覆盖。

### B14 · P1 · rebuild-state 会清空 V3 知识，并没有恢复映射

**证据：** [apps/aiks-cli/src/cli/resync.rs:29](E:/Project/Htrs/AISK/apps/aiks-cli/src/cli/resync.rs:29)。

命令确认后删除整个 aiks.db，再创建空库。当前数据库不仅保存同步游标，还保存 knowledge_item、chunks、embedding 等 V3 数据。函数没有读取思源托管属性重建映射，也没有备份/协调其他进程。engine_config 参数未使用。

**影响：** “重建状态”实际上变成清空本地知识库；后续重新生成依赖原始 Session 和 AI 仍可用。本次没有执行该破坏性命令。

**建议：** 明确区分“恢复同步索引”与“删除知识数据”；重建前备份并协调所有 DB 连接，按思源属性恢复。V3 知识生命周期与可重建缓存应分开。

**验收：** rebuild-state 前后知识条目不丢失，target 映射可恢复；重跑不重复创建文档。

## 4. 其他正确性问题与验收缺口

### B15 · P2 · Missing Source、Archive、附件上传没有接入完整流程

**证据：** [sync/engine.rs:383](E:/Project/Htrs/AISK/crates/aiks-core/src/sync/engine.rs:383)、[util/archive.rs:19](E:/Project/Htrs/AISK/crates/aiks-core/src/util/archive.rs:19)、[renderer/markdown.rs:162](E:/Project/Htrs/AISK/crates/aiks-core/src/renderer/markdown.rs:162)。

- mark_missing_sessions 只有定义，没有生产调用；已复现普通 run_sync 后消失源仍 is_missing=false。
- 该函数直接把所有未发现记录标 missing，未来接入时还需区分 Provider 扫描失败、禁用与真实删除。
- Archive::write/read 目前只在其单测中使用，archive.enabled=true 并不让同步保存归档。
- SiYuanSink 没有附件上传方法；Renderer 对图片和文件只输出占位说明。

**建议与验收：** 完整成功扫描后按来源标记 missing；归档接入规范化阶段并原子写入；通过 HTTP 实现附件上传与链接替换。缺失附件不能中断同步，源消失后已有知识仍可读。

### B16 · P2 · FTS 索引残留，搜索入口与“混合搜索完成”不一致

**证据：** [knowledge_repo.rs:49](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/knowledge_repo.rs:49)、[pipeline/search.rs:57](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/search.rs:57)、[api/tauri.ts:77](E:/Project/Htrs/AISK/apps/aiks-desktop/src/api/tauri.ts:77)。

重提炼生成新 ID，却不删除旧 FTS 行。已复现无子表情况下连续保存两次：hybrid_search 返回两条，其中旧 ID 的详情不存在。当前桌面 search_knowledge 有 JOIN，能过滤部分孤儿行；这个现象主要直接影响 Core hybrid_search，并造成索引膨胀。

搜索页实际调用 search_knowledge，不调用 hybrid_search；“向量搜索已接 UI”的完成描述不成立。FTS 的 LIKE fallback 也没有处理执行期 MATCH 语法错误；查询中的特殊字符可能被解释为 FTS 语法，行错误又被 filter_map 丢弃。

Hybrid merge 对同一个知识的多个向量切片重复加权；纯 FTS 的 rank 被丢弃，固定 0.5 后 HashMap 汇总，使得等分结果排序不稳定。

**建议与验收：** 索引与知识更新放同一事务；统一一个搜索服务与返回 DTO；明确普通搜索词的转义/分词策略。删除重建后无孤儿结果，语法特殊字符有确定结果，同一输入排序稳定。

### B17 · P2 · AI/Embedding 失败可能被显示为正常完成

**证据：** [ai_stage.rs:135](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/ai_stage.rs:135)、[74](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/ai_stage.rs:74)、[embedding_stage.rs:127](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/embedding_stage.rs:127)。

AI JSON 解析失败返回 skip，最终 RAW_ONLY；无法区分“无知识价值”和“模型输出损坏”。Map 阶段失败后把错误字符串当摘要继续；ai_request_log 只在最终成功路径写一条，并把 chunks 数/items 数放到 input_tokens/output_tokens。

Embedding 所有 batch 失败也记录 SUCCESS，Worker 继续 INDEXED/READY。min_knowledge_score 未在 V3 判断，decisions 字段没有进入 build_content；合法的零条结果提前返回，也不会处理上一版遗留知识。

**建议与验收：** 解析错误返回类型化失败；定义允许降级的状态并向 UI 展示；记录实际每次请求和 usage，未知 token 值用 NULL；明确零条结果的替换政策。模型不合法输出、限流、全量 embedding 失败不能显示完整成功。

### B18 · P2 · Provider 兼容性和信息保留不完整

**证据：** [codex.rs:46](E:/Project/Htrs/AISK/crates/aiks-core/src/providers/codex.rs:46)、[499](E:/Project/Htrs/AISK/crates/aiks-core/src/providers/codex.rs:499)、[gemini.rs:125](E:/Project/Htrs/AISK/crates/aiks-core/src/providers/gemini.rs:125)、[opencode.rs:227](E:/Project/Htrs/AISK/crates/aiks-core/src/providers/opencode.rs:227)。

- Codex 配置 include_archived_sessions=true 没有传入 Provider；DB 固定 archived=0，文件只扫描 sessions。
- Codex DB 路径把 rollout_path 转成文件名后丢弃原路径；load_session 再次遍历默认目录，外部 rollout 路径不能直接使用。
- Claude/Codex/Gemini 对未知顶层事件多处直接跳过，metadata 通常为空；Unknown block 的局部支持不等于未知事件全量保留。
- OpenCode 的 error ToolResult 从 state.output 取内容；固定上游 commit 的 ToolStateError 实际定义 state.error，因此会丢掉错误原因。上游证据：[本机 schema/session.ts:292](E:/Project/Htrs/AISK/references/opencode/packages/schema/src/v1/session.ts:292)，commit `193de13a88d62a6409c6d385831180f1def527dc`。
- OpenCode 非图片 file 直接跳过，message/part 的坏行可被静默忽略。

**建议与验收：** 按 AGENTS.md 的上游排查顺序补匿名 fixture、更新 parser_version，并跑完整 Golden；归档、未知事件、工具错误、非图片附件都有明确保留策略。不要把未经真实版本验证的 Schema 假设写成兼容承诺。

### B19 · P2 · Hash 没有覆盖全部可见内容

**证据：** [model/hash.rs:11](E:/Project/Htrs/AISK/crates/aiks-core/src/model/hash.rs:11)。

title、project、部分文件字段没有进入 hash；图片只 hash 前 128 字节。已复现标题改变、图片尾部改变，hash 不变。渲染配置/脱敏规则变化也不会自动使已同步目标更新。

**建议与验收：** 分清原始内容指纹、Canonical 指纹和渲染指纹；稳定结构化编码，全量流式 hash 大对象，并纳入影响输出的版本。标题、附件及渲染规则修改后可可靠重同步。

### B20 · P2 · 增量扫描代码闲置，实际同步仍大量全量读取

**证据：** [sync/scanner.rs:27](E:/Project/Htrs/AISK/crates/aiks-core/src/sync/scanner.rs:27)、[sync/engine.rs:232](E:/Project/Htrs/AISK/crates/aiks-core/src/sync/engine.rs:232)、[gemini.rs:383](E:/Project/Htrs/AISK/crates/aiks-core/src/providers/gemini.rs:383)、[worker.rs:126](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/worker.rs:126)。

IncrementalScanner 只被其单元测试调用。run_sync 每次加载完整 Session 后才能判 UNCHANGED；Gemini load 每条都遍历文件并解析以匹配 ID；每个 Pipeline 又 discover 一次整个 Provider。数量增长时可能出现近似二次扫描开销。

**建议与验收：** 优先使用 summary.source_path 和 DB 原路径；将 file state、hash、parser version 接入真实调度；阻塞 I/O/SQLite 使用专用执行上下文。基于 1k/10k 匿名 Session 测量扫描次数、耗时、峰值内存，不凭感觉宣称高性能。

### B21 · P2 · 分类筛选和状态数字不真实

**证据：** [commands.rs:582](E:/Project/Htrs/AISK/apps/aiks-desktop/src-tauri/src/commands.rs:582)、[engine/mod.rs:530](E:/Project/Htrs/AISK/crates/aiks-core/src/engine/mod.rs:530)、[pipeline/repo.rs:163](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/repo.rs:163)。

list_knowledge 接收 project/category，但 SQL 和 total 都没有过滤。前端选分类仍返回全部数据。full_status 的 siyuan_ready 实际只检查 Client 能否构造；ai_ready 等于 enabled；last_sync_updated 固定为 0；部分列表 knowledge_count 固定为 0。V3 之外的提炼统计仍查旧 knowledge_extraction。

**建议与验收：** 查询条件和 total 使用同一套 predicate；健康状态来自真实带时间戳检查；统一 V3 统计来源。离线不能显示就绪，筛选计数与结果一致。

### B22 · P2 · CLI dry-run/resync 的行为不满足命令含义

**证据：** [cli/sync.rs:3](E:/Project/Htrs/AISK/apps/aiks-cli/src/cli/sync.rs:3)、[storage/db.rs:83](E:/Project/Htrs/AISK/crates/aiks-core/src/storage/db.rs:83)、[cli/resync.rs:18](E:/Project/Htrs/AISK/apps/aiks-cli/src/cli/resync.rs:18)。

CLI dry-run 只列发现数量，不提供 NEW/UPDATED/UNCHANGED。resync 指定 session-id 但不指定 source 时，更新条件使用 source=""；已复现目标 hash 完全没重置。resync 还固定 overwrite=true，应在命令帮助及参数中明确其覆盖语义。

**建议与验收：** 复用无副作用 diff；source 可选时正确按 ID 或复合身份定位；明确 resync 与 overwrite 的关系。先验证预览目标集合，再执行同一个集合。

### B23 · P1 · Embedding 配置缺乏边界校验，可造成循环不前进

**证据：** [embedding_stage.rs:140](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/embedding_stage.rs:140)、[102](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/embedding_stage.rs:102)、[embedding_client.rs:58](E:/Project/Htrs/AISK/crates/aiks-core/src/pipeline/embedding_client.rs:58)。

滑窗用 end.saturating_sub(overlap_chars) 计算下一 start。target=0 或 overlap>=target，且输入需拆分时，start 可始终不前进，chunks 不断增长；batch_size=0 会使 slice.chunks(0) panic。Config::from_file 没有相应验证。

另外，LLM Chunker 的首条消息不计入扩张预算，单条超长消息不拆分；已复现一个 chunk 超过 20k 估算 token。Embedding 返回的维度、数量也没有严格校验，dimensions 配置没有加入请求。

**建议与验收：** 启动/保存配置时验证 batch>0、target>0、0<=overlap<target<=max；大消息有二级拆分；接收向量时校验维度和完整性。不运行无限循环也能由循环不变式证明这一缺陷；本次未执行可能耗尽内存的配置。

## 5. 架构、文档与发布治理

### 5.1 项目约束和 V3 方向需要一个明确的权威版本

AGENTS.md 禁止自研 Embedding Pipeline、全文搜索引擎、知识库 Web UI，并把 GUI/Extractor 放到后续版本；实际已有 Embedding、FTS5 和知识库 UI，后续文档又规划弱化 SiYuan。

这说明代码与约束存在直接冲突。本次无法仅凭“后续设计文档存在”推定负责人已正式修改最高项目约束，也不建议未经确认删除既有实现。

**建议：** 发布一份负责人认可的 ADR，明确当前维护的是 V1 同步工具还是 V3 独立知识产品，并同步更新 AGENTS.md、README 和验收标准。在决策前，优先处理两条方向都必需的正确性、安全和数据恢复问题。

### 5.2 完成状态文档高估了实际闭环

- README 仍称“没有正式实现代码”。
- Phase A–E 文档称自动入队、混合搜索界面完成，但实际调用链未接通。
- acceptance.md 全部未勾选；其中多个标准经本次审查确认尚不满足，不能靠补勾选解决。
- config.example.toml 保留旧 extractor 配置，未说明实际使用的 ai/embedding 配置。默认 ai.enabled=true，旧 extractor.enabled=false 并不能关闭 V3 AI。

**建议：** 每项功能用“已有模块 / 已接入 / 已测试 / 已真实环境验收”四个证据维度记录，避免一个“完成”掩盖差异。

### 5.3 上游分析与分发说明不完整

当前 references 中有六个上游目录，缺少 mnemos、claude-code、siyuan。对应分析仍出现 N/A/TBD，没有满足 AGENTS.md 要求的九仓库固定 SHA 记录。即使不能取得 Claude 的私有实现，公共仓库资料仍需按实际可读范围记录 commit 和局限。

THIRD_PARTY_NOTICES 同时包含“Bundled Runtime”和旧“External only”的思源说明，Derived Files 仍写旧 src/providers 路径，与实际 crates 路径不一致；部分依赖条目重复。

**建议：** 补齐固定版本、实际复用文件及分发资产清单，统一说明口径；对安装包中的许可材料做交付核查。此处是文档/可追溯性发现，不是对许可证法律义务作最终判断。

### 5.4 测试覆盖与发布门禁偏弱

现有 84 个测试主要是模块内联单测。未发现执行仓库 fixtures 后比较完整 Canonical 输出的 Golden 套件；OpenCode 测试创建普通临时 SQLite，没有 WAL 活跃写入场景，fixtures/opencode README 列出的 db 文件也不在当前文件清单中。

SyncEngine 没有覆盖远端 Create/Update/Attrs 失败、Conflict、dry-run→real、重启恢复等的集成测试。CLI/桌面端无测试；前端 package.json 无 test/lint 脚本；没有 .github CI 工作流。外部 CI 是否存在，本次无法判断。

**建议：** 用失败场景驱动补测试，不要继续只增加枚举/格式化 happy path 单测。编译警告可随后清理；它们的优先级低于同步与数据完整性问题。

## 6. 对 V1 验收标准的映射

| 验收主题 | 当前判断 | 主要阻碍 |
|---|---|---|
| 四 Provider 与 Canonical | 部分实现，真实版本全覆盖未验证 | B18、缺 Golden |
| 首次导入/一个 Session 一个文档 | 不可判定通过，已有明确阻断 | B02、B03、B04 |
| Session 追加与重启幂等 | 不满足完整恢复语义 | B03、B11、B19 |
| OpenCode WAL 只读 | 代码使用只读；活跃 WAL 未验证 | 缺 WAL 并发测试 |
| SiYuan Offline 自动补同步 | 不满足 | B03、B08 |
| Conflict 默认不覆盖 | 不满足 | B04 |
| Missing Source | 未接入 | B15 |
| Security | 部分实现且有可复现旁路 | B06 |
| Renderer | 基础测试通过，中文边界失败 | B05 |
| SiYuan Metadata | 不满足可靠写入/恢复 | B02、B04 |
| CLI | 命令存在但语义不完整 | B14、B22 |
| Archive/Assets | 模块或占位存在，链路未完成 | B15 |
| Tests | 单元测试通过，验收套件不足 | 第 5.4 节 |
| Release | Windows 测试目标/前端可构建；发布未验收 | 生命周期、文档、安装升级测试 |

## 7. 后续建议与实施规划

以下是基于本次代码证据的排期建议，不是已获批准的范围变更。以一名熟悉 Rust/Tauri 的开发者为参考，工期需在首轮修复后校准。

### M0：稳定性止损与基线冻结，约 1–2 个工作日

**工作：** 修复 StateDb 共享访问、统一 Unicode 截断、补齐脱敏出口；为现有 aiks.db 制定备份和恢复步骤；停止以 rebuild-state 清库方式排障。

**交付：** 不依赖 unsafe Sync 的数据访问层、安全文本处理组件、匿名安全回归测试。

**退出条件：** B01/B05/B06 有对应测试和明确修复证据。先处理数据库安全，再做并发压测。

### M1：把“可靠采集与同步”做成闭环，约 3–5 个工作日

**工作：**

1. 修正 SiYuan API 契约并新增回环 mock + 独立 Notebook 集成测试。
2. 重构 observed/synced/target 三种基线和 Pending/Retry/Conflict 状态机。
3. 修复 Tauri runtime 单一所有权，配置真实生效。
4. 抽出桌面/CLI 共享的 watcher + periodic 调度，支持思源离线。
5. 接入 Archive、Missing Source 和附件流程，修复 CLI dry-run/resync/rebuild 语义。

**退出条件：** 创建 → 不变 → 追加 → 手改冲突 → 显式覆盖 → 离线恢复 → 重启 → 源删除这一条完整脚本通过，且无重复文档和秘密泄漏。

### M2：如果继续维护 V3，完成可恢复 Pipeline，约 4–6 个工作日

该阶段以第 5.1 节范围决策为前提；不额外引入被 AGENTS.md 禁止的新核心依赖。

**工作：**

1. 接通真实入队入口并回填历史 Session。
2. 持久队列、并发上限、同 Session 单飞、generation 和 attempt。
3. 事务化更新 items/chunks/embedding/FTS，重试保留上一版有效知识。
4. 每个阶段统一失败终态与重启恢复，规范化零条结果和降级语义。
5. 修复 AI 输出验证、Embedding 配置校验和调用日志。
6. 统一搜索入口及 UI 状态，修复分类过滤。

**退出条件：** 同一 Session 可反复更新和重试；随机阶段停止进程后可恢复；AI/Embedding 失败不被标完整成功；知识详情和来源对应同一 generation。

### M3：兼容性与发布验收，约 3–5 个工作日

**工作：** 四 Provider 固定上游版本及 Golden、活跃 WAL、Unknown 和坏 Session；Windows 安装/重启/托盘/升级/卸载；匿名 1k/10k 数据集容量基准；更新权威文档和通知清单，加入持续测试门禁。

**退出条件：** acceptance.md 每条均链接到测试/人工证据；更新实际版本号和变更说明；已知局限写入发行说明。未通过验收不将“能打包”称为稳定版。

### 建议拆分的 PR 顺序

| PR | 范围 | 核心验证 |
|---|---|---|
| 1 | DB 安全、Unicode、脱敏 | B01/B05/B06 回归 |
| 2 | SiYuan 契约、同步状态、Conflict | 故障注入、重复运行、离线恢复 |
| 3 | Runtime、配置、Watcher/Scanner | 桌面生命周期、开关真实生效 |
| 4 | Archive/Missing/Assets、CLI 恢复 | 源删除、附件缺失、无副作用预览 |
| 5 | V3 队列、版本、事务、错误恢复 | 重复提炼、崩溃恢复、并发上限 |
| 6 | 搜索/状态/Provider Golden/文档门禁 | 筛选、索引一致性、固定版本验收 |

每个 PR 都先加入能失败的回归用例，再修复并验证；避免一次大改同时替换存储、同步、AI 与 UI，导致问题无法归因。

## 8. 最小回归用例清单

除继续保留现有 84 个单测外，至少补齐：

- dry-run → real、失败 → 重试、写正文成功但写属性失败。
- 远端被手工编辑、target 被删除、查询/鉴权失败、目标 hash 读取失败。
- 单 Session 损坏、单 Provider 不可用、Unknown event、源移除/恢复。
- 每个 Provider 的追加、重写/截断、归档、parser_version 升级。
- 活跃 OpenCode WAL 下读数据且不妨碍写入。
- 中英文/Emoji 工具结果及中文错误响应安全截断。
- Secret 在标题、Unknown、工具参数、错误响应和日志路径的传播。
- 相同 Session 连续提炼、带 embedding 重建、零知识结果、事务失败回滚。
- 并发重复入队、所有阶段失败、进程中断、重启恢复、队列背压。
- 设置持久化与实际行为、Watcher 保活、周期兜底、Runtime 重启和退出。
- 特殊字符搜索、分类筛选、孤儿 FTS 清理、稳定排序。
- rebuild-state 保留知识数据且恢复文档映射。

## 9. 项目判断

项目值得继续做，模块基础已经形成；最需要投入的是正确性、恢复能力与真实入口接通。现阶段应定位为“可构建、关键闭环仍待修复的开发版本”。

下一次里程碑应以“可靠同步与可恢复处理通过验收”为目标，而不是新增页面数量、模块数量或版本名称。

