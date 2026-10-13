# AIKS 第二轮代码审查报告

- 审查日期：2026-09-13
- 审查基线：`7c76486af0f0dfa1bf4c8474d7e6b1fc4482d4eb`
- 对比范围：第一轮审查后 M0/M1/M2 修复提交及当前实际调用链。
- 第一轮报告：[项目审查](2026-09-13-project-audit.md)。
- 本轮复现源码及说明：[复现附件](2026-09-13-audit-round2-reproduction.md)。
- 本轮只审查并产出文档，未修改业务代码、真实 Session、SiYuan 数据或用户配置。

## 1. 审查结论

当前版本仍不满足“可靠采集和同步”的发布要求。已有修复确实改善了数据库同步保护、重复提炼事务、Watcher 生命周期等问题，但新增了 CLI 编译失败和默认知识列表查询失败。文档更新、冲突保护、运行时管理及 AI 错误处理仍存在实际入口未闭环的问题。

不能依据“113 个 Core 测试通过”判定整个产品可用：工作区构建失败，且本轮 10 个匿名缺陷探针全部复现了当前错误行为。探针通过表示缺陷存在，不表示功能正确。

优先级约定：P1 为阻碍发布、破坏数据正确性或核心功能的问题；P2 为重要完整性、恢复能力或维护性问题。以下按修复紧迫程度排列。静态结论与动态复现分别标明，未进行真实桌面、SiYuan 和外部模型端到端验收。

## 2. 主要发现

### R01 · P1 · CLI 调用已删除函数，整个工作区无法通过构建

**位置：** `apps/aiks-cli/src/main.rs:56`；`apps/aiks-cli/src/cli/resync.rs:34`。

重构后模块导出的是 `rebuild_sync_index` 和 `reset_all_data`，主入口仍调用 `rebuild_state`。执行 `cargo test --workspace` 实际出现 E0425，而不是仅有告警。

**影响：** 当前提交不能作为完整工作区构建基线；Core 和前端分别通过不能替代此检查。

**建议：** 同时修正命令枚举、分发和帮助文案，明确普通索引重建与破坏性重置的边界。验收包括工作区构建、CLI help 和临时数据库上的命令测试。

### R02 · P1 · 默认知识列表 SQL 绑定数不匹配

**位置：** `apps/aiks-desktop/src-tauri/src/commands.rs:640–688`。

无筛选条件时查询只有 LIMIT ?1、OFFSET ?2，却固定绑定四个值；仅 project 筛选也不能匹配四个参数。COUNT 查询复用该绑定方式，并用 `unwrap_or(0)` 掩盖错误。

**复现：** 在项目实际 StateDb 上执行同结构查询，得到 `InvalidParameterCount(3, 2)`。这里的 3 是绑定过程中首先失败的数量，不能理解为调用端只传了三个值。

**影响：** 默认知识列表返回错误；依赖列表且将错误转为空数组的界面会表现为空数据。并非所有筛选组合都失败。

**建议：** WHERE 与参数向量同步构建，或使用固定的可选条件。覆盖无筛选、仅项目、仅分类、双筛选和 total 一致性；数据库错误应向调用方返回。

### R03 · P1 · 查询已有文档变成空实现，更新路径实际上一直创建

**位置：** `crates/aiks-core/src/sink/siyuan.rs:148`；`crates/aiks-core/src/sync/engine.rs:317–366`。

`find_document_by_session` 无条件返回 None。注释要求调用方使用已保存 target_id，但 SyncEngine 仍只调用这个空实现决定创建还是更新。

**复现：** 预置已成功同步的 existing-doc，再输入变化后的 Session。HTTP Mock 记录到 createDocWithMd，未记录 updateBlock，统计却为 updated = 1。

**影响：** 本地存在映射也不能正常走更新；标题或路径变化时尤其可能产生新文档和旧文档遗留。真实 SiYuan 同路径创建的具体行为本轮未验证，因此不将“每次必定重复”作为已证事实。当前冲突分支也因 existing_doc 为 None 而不可达。

**建议：** 优先通过本地 target_id 获取远端文档；映射丢失后再按托管属性恢复。不能用查询空实现绕过 API 契约问题。验证 HTTP 方法、文档 ID 连续性及重建后的恢复能力。

### R04 · P1 · 已同步 Session 经 Core dry-run 后，真实更新被吞掉

**位置：** `crates/aiks-core/src/sync/engine.rs:262–308`。

当前 UNCHANGED 判断只检查已观察 source hash 和目标状态，未确认目标的 synced_hash 与本次内容一致。dry-run 更新 source hash，保留旧目标的 SYNCED 状态；随后真实运行便提前返回 UNCHANGED。

**复现：** 旧 source/target 已同步 → 源变化 → Core dry-run → 正常同步。结果分别为 updated = 1、unchanged = 1，目标 synced_hash 仍是旧值。

**边界：** 当前 CLI dry-run 使用扫描入口，未直接走这里；此问题位于公开 Core 同步路径。首次无目标场景已改善，不应笼统认定 B03 完全未修复。

**建议：** 把观察状态与成功提交状态严格分开；UNCHANGED 必须比较成功目标对应的 hash/parser version。dry-run 不应推进会影响后续提交的状态。

### R05 · P1 · 冲突基线仍不是正文，属性失败仍提交成功

**位置：** `crates/aiks-core/src/sync/engine.rs:324–383`。

即使修好 R03，冲突判断仍比较自定义源 hash 与 synced_hash。用户只修改 SiYuan 正文时，托管属性不一定改变；`target_hash = content_hash.clone()` 也没有记录实际远端正文基线。

此外，设置可恢复属性失败只记警告，仍 mark_synced；mark_synced 自身的错误也被忽略。

**复现：** Mock 返回属性设置失败，首次同步仍报告 new = 1，数据库目标状态仍为 SYNCED。

**建议：** 从实际写入/读取的目标正文建立稳定比较基线；正文、属性、本地提交组成可恢复的状态机。任一必要步骤失败都不能记最终成功。已有 b04 测试只是直接传入两个不同 hash 调用仓储，不证明 SyncEngine 会计算正文 hash。

### R06 · P1 · Runtime 仍未写入 Tauri 已注册状态

**位置：** `apps/aiks-desktop/src-tauri/src/lib.rs:24–33`；`apps/aiks-desktop/src-tauri/src/lifecycle.rs:89,146`。

初始化注册的是 `Arc<Mutex<Option<SiyuanRuntime>>>`，值为 None。启动后再次 `app.manage(Arc::new(Mutex::new(Some(runtime))))`。本地 Tauri 2.11.5 源码中 manage 对同类型重复注册返回 false，并不替换原状态。

**影响：** 消费者仍取到 None，重启命令会报告 runtime 未初始化，关闭处理也拿不到真正启动的 runtime。更换容器类型没有修好注册逻辑。

**证据等级：** 调用链和本地依赖源码确认；未启动真实桌面进程验证残留进程行为。

**建议：** 取得原注册 Arc 并修改锁内 Option；覆盖成功启动、失败清理、重启和退出。检查 manage 返回值，避免静默丢弃对象。

### R07 · P1 · 保存设置覆盖整个 aiks.toml，丢失未展示配置

**位置：** `apps/aiks-desktop/src-tauri/src/commands.rs:216–252`；`crates/aiks-core/src/ai/config.rs`。

save_settings 用固定模板重新生成配置，只保留少量布尔值和扫描参数。已有 AI 地址、模型、密钥、Embedding 配置、Provider 路径等字段将被覆盖丢失，重启加载时回落到默认值。界面字段 `auto_extract` 被写入 [ai]，但对应配置类型没有该字段。

**影响：** 用户只修改一个界面选项，也可能改变下次启动的模型连接和采集范围；“保存成功”不等于完整设置生效。运行中的 engine 也没有重新配置。

**建议：** 读入完整强类型配置后只修改受控字段，原子写回；区分即时生效和重启生效。用带自定义路径/模型的临时配置验证未知和非 UI 字段保留。此结论来自代码，未触碰实际用户配置。

### R08 · P1 · 桌面自动同步仍缺周期扫描及完整离线启动路径

**位置：** `apps/aiks-desktop/src-tauri/src/lifecycle.rs:126–255`。

WatcherHandle 保存到 AppState，生命周期问题已修好。但桌面生命周期没有周期扫描任务，启动 Watcher 也未遵循 watch_enabled；external 启动分支仅建立 engine，没有启动相同的 Watcher/初始同步流程。内置运行时失败分支仍缺可用 engine，限制离线采集。

**影响：** 丢失文件事件、启动路径差异、运行时不可用时，自动同步没有要求的兜底。CLI daemon 的周期机制不等于桌面拥有该机制。

**建议：** 将扫描调度与 SiYuan 可用性解耦；所有启动模式共用调度器，同时支持 Watcher 和周期全量校准，并落实开关。

### R09 · P1 · 启动自动提炼已接入，手动入口与历史补提炼仍未接通

**位置：** `apps/aiks-desktop/src-tauri/src/commands.rs:449`；`crates/aiks-core/src/sync/engine.rs:424`；`crates/aiks-core/src/engine/mod.rs:475`。

启动及 Watcher 已使用 sync_and_enqueue_extraction，这是有效修复。但 UI 的 sync_and_extract 仍调用普通 sync 并记录候选信息，没有提交 PipelineJob。托盘和 CLI 的普通同步也不能据此宣称完成提炼。

enqueue_all_pending_for_pipeline 没有生产调用者；即使调用，也只建立 pipeline_run，不向 worker 提交任务。

**建议：** 统一用户入口与自动入口的编排逻辑，明确是否自动提炼的配置；历史回填需要可执行任务而非仅数据库占位行。验收应观察实际 worker/模型调用及最终产物。

### R10 · P1 · AI 解析错误仍被正常运行路径转换成跳过

**位置：** `crates/aiks-core/src/pipeline/ai_stage.rs:65,131,138–155`。

新增 parse_v3_result_typed 能返回错误，但实际 run/map_reduce 使用的包装函数把错误转换为 skip。

**复现：** 匿名 chunk 加 Mock 模型返回非 JSON，直接运行 AiStage，结果为 Ok(0)，AI_EXTRACTED 阶段被标记 SUCCESS。仅测试 typed helper 返回 Err 无法发现此问题。

**影响：** 模型故障/协议不兼容与“无可提炼内容”混淆，降低重试与排错能力。

**建议：** 主路径传播结构化错误，区分业务 skip 与执行失败；覆盖真实 stage 入口和多段合并入口，明确 Embedding 失败时的降级状态。

### R11 · P1 · 中文截断仍可 panic，新增错误预览也有同类问题

**位置：** `crates/aiks-core/src/pipeline/cleaner.rs:120`；`crates/aiks-core/src/pipeline/ai_stage.rs:141`。

cleaner 仍按字节切 `[..8000]`；新 typed 解析错误路径按字节切 100 字节预览。

**复现：** 3,000 个“中”组成工具结果触发 cleaner panic；40 个“中”组成非法响应触发解析错误预览 panic。两者均用 catch_unwind 捕获，未中断测试进程。

**影响：** 正常中文 Session 或非标准模型响应可使后台任务异常退出。worker 只处理返回 Err，未监督 panic，可能遗留处理中状态。

**建议：** 统一 UTF-8 安全裁剪，并审计错误预览及旧提炼/Embedding 客户端的边界；使用实际 cleaner/stage 入口验证。renderer 和部分 chunker 的安全裁剪修复有效，但不能代表全项目覆盖。

### R12 · P1 · 超长切片通过截断丢失正文，中文仍超出预算

**位置：** `crates/aiks-core/src/pipeline/session_chunker.rs:88–128`。

预算估计按字节长度，截断却按字符数；首条消息也未计入扩展循环的累计预算。超长 chunk 直接截断后进入下一组消息，尾部没有继续生成后续 chunk。

**复现：** 80,000 个“中”并追加尾部标记。首 chunk 的项目估算 token_count 仍超过 20,000，所有输出 chunk 均不含尾部标记。

**影响：** 超预算请求依旧存在，且后部知识永久不进入提炼。这里的 token_count 是项目估算值，不是对具体模型 tokenizer 的实测。

**建议：** 按一致预算拆分单条长消息并保留全部内容，明确重叠策略。测试中文、英文、Emoji 的预算及内容覆盖率，不能只检查 ASCII 输出长度。

### R13 · P2 · 并发上限有效，但持久队列和恢复仍是空壳

**位置：** `crates/aiks-core/src/pipeline/worker.rs:35–107`；`crates/aiks-core/migrations/003_pipeline_job.sql`。

Semaphore 限制实际运行任务数量，属于有效修复。但 channel 无界，先 spawn 再等 permit，仍能堆积大量等待任务；同一 Session 没有去重或版本隔离。pipeline_job 目前只建表，没有生产入队、领取、租约和重启恢复调用。

外层失败处理还会把内部记录的具体失败阶段覆盖成 UNKNOWN。

**影响：** 退出后内存任务不可恢复；快速重复触发可能重复计算、竞争写入或旧版本结果覆盖新版本。数据库互斥只能保护连接，不能提供业务幂等。

**建议：** 持久任务唯一键包含 source/session/hash/version，明确有界领取与重试、同 Session 串行和过期结果拒绝；保留准确失败阶段。

### R14 · P2 · Provider 扫描失败被当成源删除

**位置：** `crates/aiks-core/src/sync/engine.rs:394–417`；`crates/aiks-core/src/engine/mod.rs:484`。

Missing 检查已接入全量编排，但再次调用 discover_all；Provider 失败被隔离为空结果，随后将不在结果中的数据库 Session 标为 missing，未确认对应源的扫描是否成功、是否仍启用。

**复现：** 数据库预置存在 Session，Provider 返回发现失败，mark_missing_sessions 仍返回 1 并标为 missing。

**影响：** 权限、路径、暂时不可用会被误报为删除。本轮未观察到因此删除远端文档。

**建议：** 保存每个 Provider 的扫描成功性和完整快照，仅对成功覆盖的源判断缺失；失败显示不可用，不能等同删除。避免同轮二次扫描产生不同快照。

### R15 · P1 · 脱敏增强未覆盖最终文档输出

**位置：** `crates/aiks-core/src/renderer/markdown.rs`。

正文、Unknown 等字段有改善，但标题/部分元数据拼接后没有统一最终脱敏。

**复现：** 标题为虚构字符串 password=AUDITSECRET123，开启脱敏后输出 Markdown 仍包含 AUDITSECRET123。

**影响：** 凭据位于标题或其他旁路字段时仍可能写入知识库。现有正文模式测试不能证明整个输出安全。

**建议：** 明确上传、日志、模型请求和本地原文缓存的脱敏边界；对完整最终文档做端到端脱敏断言。测试仅使用假凭据。

## 3. 第一轮 B01–B23 的复核状态

“未见闭环”表示在本轮调用链复核中仍没有足够证据关闭，不代表本轮重新执行了第一轮全部实验。

| 编号 | 本轮判定 | 说明 |
|---|---|---|
| B01 数据库同步保护 | 主要缺陷已修复 | Connection 已置于 Mutex；遗留 unsafe impl 可移除，但不再沿用第一轮无互斥的判断。 |
| B02 SiYuan API | 部分修复，新增阻断 | API 局部调整；已有文档查询变空实现，见 R03。真实 API 契约仍需集成验收。 |
| B03 增量状态 | 部分修复 | 无成功目标的重试改善；已同步目标 dry-run 仍吞更新，R04。 |
| B04 冲突 | 未闭环 | 未使用远端正文基线，R03、R05。 |
| B05 UTF-8 | 部分修复 | renderer 等改善；cleaner 和错误预览仍 panic，R11。 |
| B06 脱敏 | 部分修复 | 模式与 Unknown 改善；最终标题仍泄漏，R15。 |
| B07 Runtime | 未修复 | 仍重复 manage，R06。 |
| B08 Watcher/Scanner | 部分修复 | handle 已保存；周期、模式差异仍有缺口，R08。 |
| B09 提炼调度 | 部分修复 | 启动/Watcher 已接入；手动/回填未接入，R09。 |
| B10 设置 | 部分修复且有回归 | 开始写 engine 配置，但覆盖非 UI 字段，R07。 |
| B11 重复提炼外键 | 主要缺陷已修复 | save_items 事务先删除依赖，再写新结果；已有回归测试通过。 |
| B12 Worker 恢复 | 部分修复 | 返回错误有兜底；持久恢复、panic、阶段保留未闭环，R11、R13。 |
| B13 并发 | 部分修复 | 实际执行限流已加；队列、去重、版本隔离未完成，R13。 |
| B14 rebuild | 部分修复且有回归 | 底层保留知识测试通过；CLI 编译失败，远端映射恢复缺失，R01、R03。 |
| B15 Missing/Archive/附件 | 部分修复 | Missing 接入但误判；Archive/附件完整同步验收仍缺，R14。 |
| B16 FTS/搜索 | 部分修复 | 当前知识替换会清理关联 FTS；界面仍非完整混合搜索链路。旧索引迁移需另验。 |
| B17 AI/Embedding 状态 | 未闭环 | typed helper 不能代表实际 stage；R10。Embedding 降级验收仍缺。 |
| B18 Provider 兼容性 | 未见闭环 | 仍需逐 Provider fixture/golden、WAL、未知事件和错误隔离验收。 |
| B19 hash 内容覆盖 | 未见闭环 | 应保证所有会改变渲染/提炼结果的字段参与变化检测。 |
| B20 增量扫描效率 | 未见闭环 | 存在增量模块不等于实际入口利用；需要读文件数量/字节的验证。 |
| B21 筛选/状态 | 部分修复且有回归 | 分类条件已加，默认 SQL 却失败；健康状态真实性仍待验，R02。 |
| B22 CLI 语义 | 未闭环 | dry-run 仍偏扫描；resync/rebuild 的命令行为需与用户语义一致，且先修 R01。 |
| B23 切片配置边界 | 部分修复 | 原 Embedding 循环边界已有修复；新 LLM 截断仍丢内容、中文超预算，R12。 |

## 4. 实际验证与限制

| 检查 | 结果 | 可以证明什么 |
|---|---|---|
| cargo test --workspace | 失败，E0425 | 当前 CLI 入口编译回归；不能宣称工作区通过。 |
| cargo test -p aiks-core | 113 个既有测试通过 | 92 个单元测试、15 个 review_repro、6 个 m1m2_fixes 的现有断言通过。 |
| 桌面 npm run build | 成功 | TypeScript/Vite 前端可构建；不证明 Rust 命令、窗口生命周期及真实服务可用。 |
| 本轮匿名缺陷探针 | 10/10 复现 | 覆盖 R02/R03/R04/R05/R10/R11/R12/R14/R15，R11 两个用例。 |
| Tauri Runtime | 静态与本地依赖源码核验 | 同类型 manage 不替换，未做真实进程退出实验。 |
| 真实外部端到端 | 未执行 | 未连接真实模型或修改真实 SiYuan，不能宣称完成产品验收。 |

探针使用临时 SQLite、虚构 Provider、localhost HTTP Mock 和匿名文本。测试源码保存在复现附件，临时 .rs 已移除。Core/探针运行日志分别位于本地 target/review-round2-core.log、target/review-round2-probes.log，不作为发布资产。

修复文档存在“辅助方法测试通过 → 产品路径已修复”的推断跳跃。例如 B04 测试直接写仓储 hash，B17 只测 typed parser，B09 回填只断言 pipeline_run 行出现。这些测试有价值，但需要补足真实调用入口断言。

## 5. 后续建议与规划

### 阶段一：恢复可构建、可查询、可更新的基线

建议顺序：R01 → R02 → R03/R05 → R04 → R06/R07。

验收门槛：

- 工作区测试和前端构建都成功。
- 已同步 Session 更新使用原文档 ID，手工编辑进入 CONFLICT。
- API 正文或属性失败不提交成功，重试不产生新的无主文档。
- dry-run 前后正式同步结果一致。
- 保存设置不丢已有配置，运行时重启/退出操作真实生效。

### 阶段二：保证中文、错误与敏感内容的正确处理

处理 R10/R11/R12/R15；把 helper 测试补充到 cleaner、AiStage、Renderer、SyncEngine 实际入口。

验收门槛：非法模型响应显式失败；中文与 Emoji 不 panic；长文本分片不丢尾部；输出完整文档脱敏；失败阶段准确且可重试。

### 阶段三：完成可恢复调度

处理 R08/R09/R13/R14。统一启动、Watcher、周期扫描、手动同步及历史回填的编排，并实现真正持久任务。

验收门槛：进程终止后重启可恢复；重复事件幂等；同 Session 新旧版本不互相覆盖；Provider 失败不误标删除；禁用开关有效。

### 阶段四：回归项目验收与发布文档

以 docs/implementation/acceptance.md 逐条验收四类 Provider、WAL、重写/截断、未知字段、附件、Archive、映射恢复与分发行为。对 AGENTS.md 的 V1 范围和当前 V3 扩展形成明确的负责人决策；审查本身不默认修改项目约束。

建议将“完成情况”改为证据表：用户入口、可复现用例、命令结果、剩余限制。没有入口和失败路径验证的项目标为“部分完成”。

## 6. 总结

当前修复方向有实际进展，但测试覆盖集中于局部方法，缺少跨模块真实调用链验证，导致文档完成状态领先于产品行为。下一轮应优先完成构建、列表查询、同步更新与冲突保护，再修复中文和模型失败路径，最后落实持久调度及完整验收。现阶段不建议以“全部 M0/M1/M2 已完成”作为发布结论。

