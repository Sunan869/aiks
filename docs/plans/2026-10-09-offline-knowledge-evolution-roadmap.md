# AIKS 单机离线版：稳定性、知识演进与长期记忆分阶段路线图

> 制定日期：2026-10-09  
> 实施分支：`feature/offline-knowledge-evolution-20261009`  
> 基线：`Sunan869/aiks` 的 `main`（建分支时 SHA `244d51ae8b50406e1433955e51d35c0262ea9caf`）  
> 状态：**执行中，S1.1 代码级测试已通过，S1.2 只读数据层启动**。按照阶段完成并验收，不将计划当作已实现。  
> 范围：个人单机 / local-first 模式。团队版 WeKnora、服务端账号权限、钉钉集成不纳入本计划。

## 1. 目标与约束

AIKS 从“跨工具 Session 归档 + 提炼”进化为“跨工具、跨会话的可追溯个人工作记忆系统”。最终实现：采集稳定可信 → 知识质量可控 → 记录知识演进 → 按项目维护长期记忆 → 在 Codex/Claude Code/OpenCode 等 Agent 中按需复用。

**必须保持的约束：**

- 保留原有个人离线运行模式；数据源只读，任何文件路径都不得被自动写回 Provider。
- 核心数据保存在本地 SQLite/SiYuan；AI 与 Embedding 继续允许离线模型或用户自选的兼容端点。**不引入必须联网或云服务强依赖**。
- 不修改已有 Session / Knowledge 稳定身份，不破坏索引、映射及 SQLite migration 的向前升级路径。
- 用户在 SiYuan 里编辑的文档不能被后台静默覆盖；冲突需可定位、可人工处理。
- 继续使用现有 Pipeline durable job / lease / stage、FTS+混合检索、SiYuan 工作台与“问 AIKS”；优先增强而非重建。
- 对每个阶段：先明确验收样例、指标和回滚方案，完成一个相对独立的模块后再运行相关 CI；**不要每改几行就触发完整跨平台 Release**。
- 跨平台以 Windows x64、macOS x64/arm64、Linux x64/arm64 为目标；新功能需有回归测试和跨平台验证。
- 不合并 `main`、不发布 Release，除非另行确认。计划分支上的开发允许分阶段提交。

## 2. 已完成/无需重复实施

仓库现有能力（需结合真实安装环境继续验证）：本地 Provider、Canonical Session、增量同步与 Watcher、SQLite 状态库、持久化 Pipeline、知识提炼、Embedding/FTS/混合检索、语义去重与关联、SiYuan Session/Knowledge 同步及冲突保护、“问 AIKS”、跨平台桌面发行链路。

近期已落地但仍需回归：OpenCode SQLite I/O 优化、长 Session 分块提炼与缓存、Codex `rollout-*` 路径碰撞修复、超过 5 MiB 的 SiYuan Session 自动分卷与远端基线。

**Copilot 损坏 JSON 兼容处理：用户已手动处理，标记为完成，不再列入待开发任务。** 仅在后续整体 Provider 验收中做回归，不为此创建重复修复任务。

## 3. 分阶段实施（完整备份移至最后）

### 阶段一：稳定性与可观测性（P0，优先完成非备份事项）

**目标**：可在真实的几百/几千个 Session 环境下持续运行，不需要反复重置数据库、人工盯日志、依赖盲目重试。该阶段通过后再进入阶段二。

#### S1.1 同步/分卷全生命周期验收

- [ ] 用真实匿名化 Codex 大 Session 回归：首次写入、2–N 分卷、再次同步幂等、追加消息、Session 缩短、远端部分文档删除、用户手动修改主目录或任一分卷。
- [ ] 校验主文档→分卷路径与 ID 唯一、SQLite `session_volume` 一致性、远端 Markdown 规范化后的 baseline 与冲突分类。
- [ ] 将“半途写入成功、记录失败/网络超时”纳入恢复测试；不得出现重复分卷、静默覆盖或误判成功。
- [ ] 对孤儿/过期分卷只报告并允许用户处理，**禁止静默批量删除**；制定手动安全清理策略。
- [ ] 增加错误分级及明确的恢复建议，安全冲突不得被当作无限重试任务。

**验收**：覆盖上述场景的自动化测试 + Windows 实例连续重复同步；每个 Session 与分卷身份稳定；编辑冲突保留原文；不丢数据。

#### S1.2 Pipeline 任务监控中心

- [ ] 在 Desktop 增加统一任务视图：按来源、Session、阶段展示 `DISCOVERED / SYNCED / QUEUED / EXTRACTING / EMBEDDING / PUBLISHING / READY / FAILED / CONFLICT`，状态名称需映射现有实际 schema，避免平行创建伪状态。
- [ ] 展示失败原因、发生时间、尝试次数、阶段耗时、待处理总量及最近成功时间。
- [ ] 支持按 Session 重试失败阶段、安全取消排队任务；重试需去重、带操作反馈，不能产生多个并行任务。
- [ ] 清晰区分“原始 Session 已同步”与“知识已提炼/已发布”。
- [ ] 可选诊断包导出：默认剔除 Session 正文、路径敏感片段、Token、私密配置，导出前有预览。

**验收**：可从 UI 定位并恢复一条可重试失败，冲突仅提供人工处理入口；无需查后台日志才能判断任务在哪一步。

#### S1.3 备份、恢复与升级安全（延期至所有其他功能和优化完成后）

- [ ] 对 SQLite 使用一致性备份方案（考虑 WAL）；将必要的 SiYuan 工作空间数据、配置与版本清单纳入备份（按需选择，仅备份合法可访问目录）。
- [ ] 恢复前校验文件完整性、应用/Schema 版本、空间容量；恢复操作必须由用户确认并防止覆盖运行中的数据。
- [ ] 验证旧版数据库升级、迁移失败回滚、断电/强退后恢复、备份跨平台迁移。
- [ ] 添加磁盘空间与 SQLite 健康检查、体积统计、只读诊断工具；不自动清除原始知识。

**验收**：使用真实规模样例完成备份 → 独立目录恢复 → 检索/分卷/知识数一致性核对；升级失败不会破坏原数据库。

#### S1.4 Provider 真实环境回归与基础性能

- [ ] 验收现有重要 Provider（至少 Codex、Claude Code、OpenCode、GitHub Copilot）在 Windows 环境的真实文件布局、持续写入、WAL/SHM、文件半写、历史大文件和增量追加；其他 Provider 分批补齐版本矩阵。
- [ ] 建立基线：空闲磁盘 I/O、首次扫描耗时、重复扫描耗时、峰值内存、单 Session 同步延迟、失败率。
- [ ] 保持输入只读与 bounded scan；对暂时不可读来源可隔离但不得静默删除已同步内容。
- [ ] 完善 CI / Rust 格式与测试、Desktop 构建、重要回归用例，消除阻塞当前迭代的测试失败。

**阶段一退出条件（更新）**：P0 同步/数据丢失风险已排除；任务状态可从 UI 解释；持续同步压力/故障注入测试有记录。**完整备份和恢复验收不再阻塞进入阶段二**，S1.3 统一留待所有其他功能及优化完成后实施。

### 阶段二：提炼效率、知识质量与跨 Session 演进（P1）

#### S2.1 提炼效率与质量基准

- [ ] 构建匿名化知识提炼评测集：短/长 Session、工具日志多、重复片段、技术决策变化、错误修复与中文混合场景。
- [ ] 建立可重复基线：耗时（P50/P95）、LLM 调用次数/Token、缓存命中率、知识遗漏率、结构化输出有效率、重复率、跨 Session 链接准确率。
- [ ] 将固定两路并发调整为可配置上限，并依据模型过载/429/超时安全降级；默认不能压垮本地模型。
- [ ] 优先考虑对**追加的 Session 消息**增量提炼，缓存依据包含模型、Prompt 版本、参数和源内容；修改已有内容时安全失效。
- [ ] 避免生成后无意义的重复 Embedding/LLM 去重，保证任务幂等。

**验收**：同一基线用例在性能优化后无明显知识遗漏或 JSON 有效率下降；性能收益以实测数字呈现，不能仅以并发数推断。

#### S2.2 知识质量反馈闭环

- [ ] 建立知识质量维度：准确性、完整性、可复用性、上下文充分性、可追溯性；自动分数仅作为建议，不宣称未经证实的事实准确。
- [ ] 支持用户反馈：有用、错误、重复、过时、需补充；保留修改历史与来源关联。
- [ ] 将反馈作为后续检索排序/复审的输入，但禁止自动删除高风险知识或覆盖用户修订。
- [ ] 支持定位提炼文本对应的原始 Session/消息区间与提炼模型版本。

**验收**：能够对错误知识标记、修正、溯源，并保证重跑提炼不擦除人工反馈。

#### S2.3 稳定 Knowledge Identity 与知识演进关系

- [ ] 评估安全的稳定 key 与 rename 机制；不依赖相似标题推断同一知识。
- [ ] 定义关系类型：`related`、`supplements`、`corrects`、`supersedes`、`resolved_by`；区别于当前单纯语义去重。
- [ ] 建立可追溯的关系证据（源 Session、消息范围、时间、判断置信度），弱证据只推荐不自动断言。
- [ ] 在知识界面展示“问题 → 决策 → 实施 → 验证”时间线，并允许确认/撤销关系。
- [ ] 关系变化应可增量更新、无重复，原知识与原 Session 不删除。

**验收**：跨 Codex/Claude Code/OpenCode 的同项目三段会话可关联成演进记录，人工修正后可稳定复现，不发生错误跨项目合并。

#### S2.4 搜索和“问 AIKS”的可追溯性增强

- [ ] 针对关键词、语义、项目、来源、时间等组合查询做质量基准；优化召回与 rerank，而非先引入新向量数据库。
- [ ] 问答展示支持跳转的证据来源（KnowledgeItem、Session、相关片段），区分引用原文与模型总结。
- [ ] 无命中/证据不足时明确表达，不编造工程结论；查询可按项目过滤。
- [ ] 比较混合搜索与纯 FTS 的时延/命中率；只有真实数据证明不足才评估 sqlite-vec/ANN。

**阶段二退出条件**：形成经过验证的高质量知识与演进链；问答能追踪证据；有公开于仓库的匿名化质量/性能基准和回归测试。

### 阶段三：项目记忆与知识反哺（P1/P2）

#### S3.1 Project Memory（项目级长期记忆）

- [ ] 明确项目身份：从 Provider 的 cwd / project_path / 用户手工绑定识别，避免同名不同目录误合并；允许修正归属。
- [ ] 自动整理项目技术栈、架构决策、环境配置（敏感字段脱敏）、历史问题与解决方案、当前约束、未完成待办。
- [ ] 提供项目记忆总览、时间线、关联知识入口；每条记忆带来源和更新时间。
- [ ] 支持过期/矛盾检测：新方案不直接覆盖旧方案，而是记录生效时间、证据和是否已确认。

**验收**：一个项目跨三个不同 AI 编程工具产生的 Session 能组成连续项目记忆，且不同项目不会误合并。

#### S3.2 日报、周报与项目回顾

- [ ] 按时间区间、项目、数据源生成“完成事项 / 关键决策 / 已解决问题 / 风险与下一步”。
- [ ] 标注来自会话的证据链接，避免把计划或模型猜测误写成已完成事项。
- [ ] 可审阅、修改、导出 Markdown/复制；默认不自动发送给外部服务。

**验收**：用户可一键生成、追溯和修订一周项目回顾，内容不泄露私密配置。

#### S3.3 向 AI 编程 Agent 反哺知识

- [ ] 支持按项目/任务检索、精选、预览并导出 Markdown 上下文包及 `AGENTS.md` / `CLAUDE.md` 建议片段。
- [ ] 默认**只导出到指定路径或复制**，不擅自修改项目的现有 Agent 指令文件；覆盖/合并需明确确认与 diff 预览。
- [ ] 定义 Token 预算、过时知识过滤、私密信息检查、引用来源和版本信息。
- [ ] 支持 Codex、Claude Code、OpenCode 等采用通用 Markdown 上下文包复用；不依赖对方开放特定 API。

**验收**：新 Agent Session 使用选定知识包，可以回答真实历史项目背景；不发生未经批准的文件写入。

#### S3.4 自然语言知识整理（受控操作）

- [ ] 在“问 AIKS”中支持草稿级操作：归类、比较、合并建议、过时项识别、整理为文档。
- [ ] 所有编辑类操作先预览 diff、引用来源、经用户确认后执行；删除或批量覆盖需要二次确认。
- [ ] 保留操作日志与撤销或版本恢复入口，不把对话文本当作无条件指令执行。

**验收**：可安全整理数条关联知识为一篇文档，不改变未经选中的资料。

**阶段三退出条件**：项目记忆持续更新、工作回顾可溯源、Agent 知识包可安全复用、自然语言编辑可审计和恢复。

## 4. 可选后续（本轮不抢优先级）

- 更广的数据源：Windsurf、Antigravity IDE 真实消息格式、未来 Provider 格式适配，基于真实证据决定。
- 高规模本地向量索引：只有数据和基准证明 512-candidate rerank 不足后再实施。
- 原生 MCP / Agent API：先通过只读搜索 + 经确认的导出验证价值，避免提前引入复杂权限面。
- 更完整的知识图谱可视化：先做可靠关系数据，再决定 UI 形态。

## 5. 执行节奏、Issue 与验收规则

每个子模块按统一节奏执行：

1. **检查远端 HEAD**：开发前确认目标分支最新提交，不覆盖其它会话新提交。
2. **用例先行**：写失败样例/匿名化 fixture；明确边界、安全约束与用户可见行为。
3. **代码与 migration**：最小增量、保持兼容；若必须迁移数据，先准备回滚/备份。
4. **相关测试**：Core Rust 测试、前端测试/构建、针对平台的真实验证；该模块稳定后再运行完整 CI。
5. **阶段验收**：更新本文件复选框、度量结果、已知风险；用户确认后才进入下一大阶段。
6. **发布**：只在阶段完成或有必要的修复后，按现有 `aiks-siyuan` Runtime 锁与 `aiks` Release 机制统一构建；不随每次小改动发布。

推荐拆分提交/PR：`S1-sync-volumes` → `S1-task-center` → `S1-provider-bench` → `S2-benchmark` → `S2-feedback` → `S2-evolution` → `S2-rag` → `S3-project-memory` → `S3-reports` → `S3-agent-export` → `S3-knowledge-actions` → **最后** `S1-backup`。

每项完成必须补充：关联 Issue/PR、测试命令、结果、重要性能指标、剩余问题和影响的 migration。不得凭提交成功宣布用户环境已验收。

## 5.1 当前开发记录（2026-10-09）

- **S1.1 已启动，尚未验收。** 增加 SiYuan 分卷切分回归测试，覆盖 UTF-8 多字节边界、分卷结果确定性、Session 追加后的内容无损与 4 MiB 上限。
- 测试提交：`05e2d031035f8d3cea961c443c1e3a1c1a17ebc4`。
- **仍需完成**：远端删除恢复的端到端测试、用户编辑冲突、写入中断、分卷缩短与状态持久化的集成测试和修复；没有验证这些场景之前不能勾选 S1.1 完成。
- 本记录不声称 Rust CI/实机测试通过；以最新功能分支工作流为准。

- 后续修复提交：`2d00143f4d32f60ff15ac8a191a6681f29b6e251`。仅当远端确认分卷文档已删除时，使用稳定路径恢复；若远端仍存在但读取失败，则明确报错并保留内容冲突保护。尚待 CI 与真实环境验证。

- 本模块进一步整理：用结构化请求收拢分卷同步参数，移除 Clippy 参数数量豁免；缩短后的历史分卷保留并在索引显示数量，待集成测试验证。

- 已补充针对分卷缩短、超长单行 UTF-8 的回归测试，并修复 `rustfmt` 格式阻塞项；仍需 CI 和远端集成场景验证。

### S1.2 增量验收记录（2026-10-09）

- 本阶段任务中心已整合 Session 原始同步、Pipeline、持久队列的独立状态与错误信息，支持筛选、单条失败 AI 重试、仅取消排队任务及最近任务汇总。
- 已验证 CI：`37891588192`（提交 `3a1840e998725bf29367997b5d246cbd1606ad0c`），包括 Windows Desktop、前端、Rust 和 Windows Provider。
- 新增的最近阶段耗时、任务刷新错误反馈和最新更新时间仍需本次后续 CI 验证。
- **未验收项**：真正的 Windows 本地操作、同步冲突人工处理入口、全量分页/统计、导出诊断包预览。未执行实机验收前不勾选 S1.2 完成。
- 无数据库迁移，继续读取现有 `pipeline_stage_run`、`pipeline_job` 与 `sync_target` 表。

### 2026-10-09 阶段进展（本轮 CI 已验证）

- S1.1 的新增 HTTP Mock 集成测试已通过 GitHub CI `37883520353`（提交 `2b7860005cabb13e24873bed89ef29874a6842bb`），覆盖首次分卷、重试幂等、用户编辑冲突、确认远端删除后的恢复、属性写入中断重试、会话缩短与历史分卷保护。
- **代码级集成测试已通过，但 S1.1 仍需 Windows + 真实内嵌 SiYuan 大 Session 实机连续同步验收**；仅凭 Mock 不将整个 S1.1 标为已完全验收。
- S1.2 已启动后端只读投影：`pipeline/task_center.rs` 从已有 `source_session`、`sync_target`、`pipeline_run`、`pipeline_job` 汇总状态；当前尚未接入桌面 UI，不提供未完成的重试/取消动作。
- 新投影没有迁移数据库或新建队列；新提交还需本分支 CI 验证。

## 6. 首个实际开发切入点

**从 S1.1 开始**：先完善 Codex 分卷同步的幂等性、变更/冲突/恢复回归测试，再做任务监控中心；本次提交仅建立路线图，不同时更改业务代码。

### S1.2 状态统计口径补充（2026-10-09）

- AI 异常优先按照当前持久任务代次统计；历史失败但已有新一代 PENDING/RUNNING 的 Session 不再算作当前 AI 异常。
- 当完全不存在持久任务时，仍可使用既有 Pipeline FAILED 状态识别历史问题。
- UI 保留最近成功刷新时间与刷新失败提示，避免用户误以为状态实时更新。
- 上述改动仍需当前开发提交的完整 CI 验证。实机运行、用户编辑冲突处置入口仍未完成验收。

### S1.2 冲突人工核查入口（2026-10-09）

- 任务中心对于 `CONFLICT` 显示“人工核查冲突”，转到本地会话详情；不会将 `CONFLICT` 视为可自动重试，不调用强制覆盖接口。
- 同步冲突涉及 SiYuan 用户编辑时仍需实机核查；本入口不表示已实现双向差异合并或人工冲突解决写入。
- 此轮改动须通过 CI，且真实 Windows/SiYuan 使用流程仍待用户验收。

### S1.2 状态取消与收尾验收（2026-10-09）

- 最近完整成功 CI：`37897976973`（提交 `0504063e77e31c140736f8a6060732c02b405026`），包含 Rust、Windows Desktop、前端、Provider 契约。
- 正在补充任务代次处于 `CANCELLED` 时的统一状态投影：队列取消不等于 Pipeline READY；隐藏旧处理阶段与旧错误，保留取消事实。
- 代码级测试不等于 Windows + 内嵌 SiYuan 实机验收。尚需人工验证取消排队、失败重试、冲突跳转，以及长时间运行情况下的状态刷新。
- 暂不勾选 S1.2 全部完成，也不合并 main、不发布 Release。

### S1.2 实机验收清单（待人工执行）

- [ ] Windows 桌面冷启动、退出与重开后，任务中心可加载相同的 SQLite 持久任务；无新建的并行队列。
- [ ] 使用 Codex/Claude Code 各一条真实 Session，检查原始同步成功与 AI 提炼状态分别显示；在真正的内嵌 SiYuan 中定位原始文档。
- [ ] 模拟 AI 上游故障，确认失败详情；上游恢复后点击单条「重试 AI」，观察 PENDING → RUNNING → READY；历史失败不能冒充当前失败。
- [ ] 在任务尚未被领取时点击「取消排队」，确认 CANCELLED 和全库统计正确；任务已运行时必须拒绝取消。
- [ ] 修改 SiYuan 内由用户管理的文档后触发同步冲突，点击「人工核查冲突」应能进入相应 Session，且不自动覆盖远端用户编辑。
- [ ] 将任务量提高到超过 200 条，确认全库统计不等于当前加载数；切换筛选、最小化/恢复窗口、长时间刷新均无错误状态残留。
- [ ] 预览、复制和导出 JSON 诊断摘要，确认无 Session 正文、日志错误原文、路径、Token 或 API Key。

**验收判定**：GitHub CI #37899002262 对提交 `9f80ce54032890df5cb14428516f88d13bd2b954` 已通过；最新代码及上述实机步骤尚待独立验证。未完成之前 S1.2 保持「代码级功能基本完成，实机未验收」，不进入 Release。

### S1.3 SQLite 一致性备份起步（2026-10-09）

- 使用 SQLite `VACUUM INTO` 创建事务一致快照，不直接复制 WAL 模式正在使用的 `state.db`。
- 目标文件必须不存在，避免覆盖用户已有备份；备份完成后进行 `PRAGMA integrity_check` 并记录 SHA-256、文件大小与格式版本。
- 提供只读快照验证；目前**没有自动覆盖式恢复**，也未打包内嵌 SiYuan 工作空间、配置、附件及版本清单，因此不能称为完整 AIKS 备份。
- 新增 SQLite WAL 与篡改检测测试。真实多 GB 性能和 Windows 实机备份、独立目录恢复仍需验证。

### S1.3 备份清单校验（2026-10-09）

- SQLite 快照之外新增独立 JSON 清单的写入与读取，使用 `create_new` 防止覆盖同名文件。
- 读取清单时验证版本、名称、大小、SHA-256、SQLite 完整性和关键 AIKS 表是否存在，再允许恢复到全新文件。
- 目前尚不包含 SiYuan 工作空间及配置，也不支持覆盖当前运行中数据库；真实 Windows 大数据量和端到端恢复仍未验收。

### S1.3 大文件与校验回归（2026-10-09）

- SHA-256 检查从整文件读入内存改为 64 KiB 缓冲分块读取，避免备份文件大小直接决定内存峰值；不改变摘要算法及清单格式。
- 增加错误 JSON 清单、文件大小摘要不一致、已有恢复目标不可覆盖测试。
- 本阶段仍只覆盖 SQLite 状态数据库，不包含完整 SiYuan 工作空间和配置。尚未确认新提交的 CI 结果，真实大文件性能有待实机验证。

### S1.3 独立恢复保护补充（2026-10-09）

- 加入恢复目标与原始快照路径相同的拒绝检查；已存在目标始终拒绝覆盖。
- 加入备份文件损坏后的恢复拒绝测试，确保校验失败时不创建恢复目标。
- 修复新备份模块的 `rustfmt` 检查问题。仍需通过最新 CI 验证。

### S1.3 FTS5 完整性检查兼容修复（2026-10-09）

- CI `37903942678` 中 8 项备份测试共享同一失败原因：SQLite FTS5 的 `PRAGMA integrity_check` 在只读连接触发内部只读错误，不能据此认定快照损坏。
- 改为仅针对隔离的备份文件短暂使用 SQLite 读写连接执行完整性检查，关闭连接后再计算 SHA-256；从不对运行中的状态数据库执行恢复操作。
- 该方案需要备份文件具备写权限；完全只读介质上的离线验证、实际大型备份资源占用和真实 Windows 恢复仍需专项验收。

### S1.3 数据库恢复兼容性门禁（2026-10-09）

- 独立备份校验不仅确认表存在，还校验核心列（Session 身份、同步状态、Pipeline 版本、持久任务代次与关联 ID），拒绝缺失字段的未完成迁移快照。
- 增加只读 `pragma_foreign_key_check` 检查，并在校验清单版本时拒绝未知未来格式。
- 补充伪造不完整 schema、清单格式版本不兼容的回归测试；这些检查不迁移/覆盖原始备份。
- 已知限制：仍只覆盖 SQLite 数据库；SiYuan 工作区、配置与附件的整体备份尚未实现，也未在用户 Windows 环境验证。

### S1.3 兼容性门禁与本轮验收（2026-10-09）

- 最新已验证基线：CI `37906126610`（`670d8bcc79e8b541d608adc8082590792fbdd34f`），SQLite 备份与 FTS5 重复检查通过。
- 本轮修正备份兼容性查询的 Rust 格式阻塞，并增加非法关系快照的拒绝路径回归测试；该测试尚待 CI 验证。
- 备份仅包含 `state.db`，尚缺 SiYuan 工作区、配置和文件资源。对完整应用的备份与恢复仍处于开发中，不能将现有数据库测试宣称为实机恢复验收。

### S1.3 外键一致性校验回归（2026-10-09）

- CI `37907988755` 共 197 项 Rust 库测试，其中 196 项通过，1 项外键损坏夹具失败：SQLite 在原测试插入数据时即拒绝非法外键，未进入备份校验阶段。
- 现改用完整 AIKS 数据库的一致快照，独立连接关闭外键约束后注入非法 `sync_target.session_id`，再验证 `load_and_verify_backup` 因真实外键违规而拒绝恢复，同时确认运行中的原数据库未改变。
- 待新 CI 验证；S1.3 完整备份尚需纳入 SiYuan 工作区和配置。

### S1.3 外键校验结果专项复核（2026-10-09）

- CI `37908899595` 的 Rust 质量任务仅在 `backup.rs` 两处格式检查失败，尚未运行当前外键夹具。
- 在独立快照中注入真实外键违规后，除验证安全恢复拒绝外，额外断言 `pragma_foreign_key_check` 实际检测到 1 条违规，且在线状态数据库未被更改。
- 备份/恢复仍仅针对 SQLite，不能替代 SiYuan 工作空间、配置、附件等完整备份。


## 7. 优先级调整决定（2026-10-09，用户确认）

- **唯一延期事项：S1.3 完整备份/恢复。** 先完成所有其他功能和优化，包括 S1.4、S2.1–S2.4、S3.1–S3.4，之后再重新启动 S1.3；S1.1/S1.2 的实机验收仍需补齐，但不为等待完整备份而阻塞后续模块。
- 已完成的 SQLite 备份实验代码保留在 `crates/aiks-core/src/storage/backup.rs`，但从 `storage/mod.rs` 移除 `pub mod backup`，暂时不参与正常编译/测试，也不暴露公共入口。**不要把此当作备份功能已完成。**
- 桌面设置页、导航及 Tauri 调用清单没有完整备份/恢复入口；后续开发不得提前重新开放备份、恢复、自动覆盖按钮或调用。
- 最新综合 CI `37909736935` 失败与暂停中的备份测试有关。本次移除实验模块导出以解除对非备份功能的 CI 阻塞；新提交的结果仍需 CI 验证。
- 下一实际开发项：**S1.4 Provider 真实环境兼容性与性能基线**，然后按上述顺序推进。每批尽量完成后端、前端和测试后再一次提交/推送，避免频繁触发 CI。

### S1.4 Provider 扫描基线第一批（2026-10-09）

- 暂停备份模块后，综合 CI `37912350877` 已通过；转入 S1.4 并优先补全扫描安全和重复扫描基线。
- 增加 12,000 行 JSONL 两次扫描的耗时报告及内容不变断言；CI 日志输出 `AIKS_PROVIDER_BASELINE`，不设置受共享 Runner 负载影响的硬性耗时阈值。
- 增加半写 JSONL → 追加完成 → 再次读取的回归测试，确保半写不被视为同步成功；限制单行字节预算防止无界读取。
- 此批测试验证共用只读读取层，而非 Windows 用户本机真实 Codex/Claude Code/OpenCode/Copilot 文件分布；真实布局、WAL/SHM 及持续增量的实机性能仍待验收。

### S1.4 Provider WAL 并发读与目录资源预算（2026-10-09）

- 增加 SQLite WAL 并发写入读取测试：只读 Provider 连接在第一次查询后维持原有快照，作者连接追加记录不影响当前快照；重新打开连接才看到新增记录。
- 增加目录扫描 `max_entries` 预算测试：超过预算时明确报错，文件保持原样，不出现「空结果即成功」的伪信号。
- 这些是可复现的跨平台合成回归，不等同于用户 Windows 真实文件环境的 Provider 兼容性验收。

### S1.4 端到端 Provider 发现基线（2026-10-09）

- 增加 160 份不同身份的 Continue Session 合成输入，使用真实 NativeProvider discovery API，验证首次与重复发现结果身份一致且无重复。
- CI 日志统一输出 `AIKS_PROVIDER_BASELINE provider=continue`、Session 数量和两轮耗时；不设共享 Runner 上不稳定的硬耗时门槛。
- 该基线为合成环境接口级测试，不能推断 Windows 本机真实磁盘吞吐和真实 Provider 格式兼容性；仍需单独实测。

### S1.4 重复发现的增删场景（2026-10-09）

- 补充 NativeProvider 增量重扫：首次发现单个 Session、随后添加第二个来源、再移除新增来源，验证原 Session 的稳定标识不会被重编排；重新加载保留的 Session 仍成功。
- 保持输入只读，测试只操作临时构造数据；不会让 AIKS 删除真实 Provider 文件。
- 该合成测试不替代 Windows 真实环境多版本 Provider 文件布局验收。

### S1.4 真实 Windows 契约测试反馈（2026-10-09）

- `37921141948` Windows Provider Contracts 暴露新增测试夹具使用相同 `sessionId` 的错误；NativeProvider 正确以 `ambiguous_session_identity` 拒绝模棱两可的来源，而非静默合并。
- 改为两个不同的 Session 身份，增加在第二个 Session 存在时追加原 Session 消息的回归验证：Session ID 稳定、内容增量可见；移除第二个 Session 后原 ID 和追加内容仍保留。
- Linux CI `37921141959` 的两处 `rustfmt` 差异一并修正；新测试结果待后续 CI 验证。

### S2.1 提炼质量基线第一批（2026-10-09）

- 新增 5 种脱敏合成样例：短会话修复、长会话块归纳、重复工具输出噪声、技术决策变化、中英文混合。
- 使用 V3 实际提炼 JSON Schema 对固定结果做反序列化与约束断言，统计样本数、结构有效率、应跳过比例、预期关键术语命中及解析耗时；统一输出 `AIKS_EXTRACTION_BASELINE`。
- 这是不依赖模型端点的确定性基线夹具，不代表真实模型达到相同质量。模型调用次数、Token、缓存命中率、P50/P95、人工知识遗漏标注和真实模型评估仍需实现。

### S2.1 结构化提炼测试负例补充（2026-10-09）

- 为固定脱敏评测集补上必填正文缺失的 Schema 反序列化拒绝测试，以及置信度超过 1 的范围门禁断言。
- 注意：现有 `V3ExtractionResult` 的反序列化本身不负责校验数值上下界；范围验证属于调用方/评测约束，不能误称反序列化会自动拒绝 1.5。
- 同批修正 `37923727413` 的两处格式差异，最新测试结果待 CI 确认。

### S2.1 噪声样本质量门禁（2026-10-09）

- 综合 CI `37924607559` 在 `extraction_quality_baseline.rs` 的单处 Rust 格式检查失败，尚未运行该轮新增负例。
- 修复格式并单独锁定 `tool_noise` 样例应跳过提炼、无知识项、低知识分；这是固定评测标签的约束，不是实际模型推理效果证明。

### S2.1 提炼分块与阶段计数补充（2026-10-09）

- 综合 CI `37927274330` 在新增分块上下文回归测试的多余空行处被 rustfmt 拦截；修复提交 `831bc60a3b79f0613a2d1fcc48478254e246a8ec`。
- 对长 Session 的分块提炼汇总实际 `cache_hits` 与 `llm_calls`，同时将单块提炼记为 1 次调用。成功或跳过提炼时，写入既有 `AI_EXTRACTED` stage 的详情 JSON，不新增数据库迁移，也不宣称它们是 Token 统计。对应提交 `43b08ce7a6e1baa04e3fcba66da5cec8683b4539`。
- 新增的计数与 Prompt 边界回归仍待本分支 CI 验证。S2.1 尚缺真实 Token、调用耗时分位数、缓存命中率趋势、429/过载降级及真实模型质量基准；不得将本批计数作为模块验收完成证明。

### S2.1 缓存命中率与批次性能度量（2026-10-09）

- 将 `cache_hit_percent` 写入现有 `AI_EXTRACTED` stage 详情 JSON（提交 `d152bba28d50d1dfe08b5cc4e5157d306f9b7b3c`），不新增数据库迁移。
- 针对每个实际调用批次采集完成时间，输出最近一轮分块提炼批次 P50/P95 耗时及对应样本数量，添加 nearest-rank 分位数回归测试（提交 `21a85000f45af79b2d55005ee2ffbc94d4447058`）。零缓存批次记录 0 毫秒；这不是单请求 P95，也不代表质量提升。
- Windows Provider CI `37928391127` 成功；最新实现仍须等待新的综合 CI 及 Rust 测试、Windows 实机验收。实际 Token usage、429 过载降级和固定样例真实模型评测仍未完成，S2.1 不勾选。

### S2.1 模型用量与流式过载安全（2026-10-09）

- `1c875430446a9274e0c62c3e1010a0e5acfa8b92`：普通与 JSON-fallback 流式响应解析 OpenAI-compatible `usage`，仅在服务端实际报告时记录 Prompt/Completion/Total Token；未报告时维持未知，不用字符数量冒充真实 Token。含带 usage 与不带 usage 回归测试。
- `7dba8099a04f440c4143f579515aa9e22a65e99f`：流式 Completion 对 HTTP 429/503 采用最多两次、250/500ms 有界退避，与非流式一致；不自动重试凭据/配置错误。
- 新提交的 CI 和真实模型压力测试仍待验证。Token 指标目前在结构化日志中，尚未建立每 Session 的数据库聚合、UI 报表；真实质量基准和高负载验收未完成。

### S2.2 本地知识反馈存储起步（2026-10-09）

- 新增 `pipeline/feedback_repo.rs` 和 `FeedbackRepo` Core API，反馈类型为 useful/incorrect/duplicate/outdated/needs_detail，append-only 保留历史，不自动更改提炼知识的正文与 stable ID。
- 反馈记录仅允许关联现有知识项，单条说明限定 4000 字符，非法种类和不存在知识 ID 均拒绝；新增真实 SQLite 负例回归。当前使用按需创建独立反馈表，后续需要纳入正式 migration、旧库升级验证、Desktop 命令及交互、关联溯源与再提炼持久性回归。
- 这批实现是 **S2.2 Core 数据层第一步，尚未完整验收**；禁止将其宣称为有用户反馈 UI 的成品。此前 CI `37930183908` 在 Token usage 测试夹具格式上失败，已另行修正。

### S2.2–S2.4 质量反馈、演进关系与可溯源检索增量（2026-10-09）

- S2.2 本地反馈：五类用户标记、追加式历史、正式迁移、Desktop 详情界面及重开数据库后的保护回归；CI `37931929699` 的四项综合作业成功。检索排序影响和 Windows 用户场景仍待验收。
- S2.3 关系建议：related/supplements/corrects/supersedes/resolved_by，必须有证据、人工确认或拒绝、审核记录；阻止已知跨项目的错误关联及重复关系。提交 `8c7df9e405c4dad9ff1713240f77954f0edf9d78`，随后 `5169e3b3267ed105efdc1155c0e4b97a073371a1` 修复测试预期与格式，综合 CI `37932859731` 全部作业成功。尚缺自动安全关联与时间线级实机体验验收。
- S2.4 原 RAG 已有可跳转知识/Session 证据引用，本轮只新增可选 project 精确检索范围、工作台“详情/反馈/演进”入口，以及把最新“错误/过时/需补充”人工标记作为模型证据的显式风险提示；不得将用户负面反馈解释为自动删除。此批仍待 CI 与真实查询验收。
- 以上模块均未进行用户 Windows 实机验收，不能勾选完整完成；不涉及完整备份入口与团队服务端逻辑。

### S1.4 与 S3.1 安全验收补充（2026-10-09）

- 综合 CI `37937650955` 在 `d8a796ebce94ba50006b09f6cb94fa3593b25d8e` 对应四项作业均成功（Rust fmt/Clippy/tests、前端、Windows Desktop、卫生）；Windows Provider 契约独立校验已成功。
- 增加 `scripts/windows-provider-baseline.ps1` 与 `docs/acceptance/windows-provider-baseline.md`，在 Windows 真机隔离 AIKS 状态目录运行只读 Provider scan，并只输出总耗时/退出状态；此脚本本身尚未在真实用户机器执行，不能当作通过验收。
- 项目记忆身份只把绝对路径（Unix `/...`、Windows `C:/...` / UNC）视为可跨来源聚合的目录；相对路径按单 Session 隔离，避免同名不同工作区的知识错误合并。
- 保留 S1.1/S1.2 实机验收缺口，S2/S3 的真实模型/SiYuan 人工体验仍须验收；完整备份仍最后处理，旧 SQLite 备份实验模块未开放。

### S3.2 / S3.3 审核后 Markdown 导出（2026-10-10）

- 在项目记忆工作台沿用已有后端 `createProjectReview` / `createAgentContextPack` 与可编辑预览，不重复搭建报告或上下文生成服务。
- 新增“保存审核后的 Markdown”本地文件下载，报告与 Agent 上下文使用不同文件后缀；使用受限的项目显示名作为下载名，避免路径字符进入文件名。复制与保存均以用户编辑后的预览内容为准；不自动写入 `AGENTS.md` / `CLAUDE.md`，也不写回 Provider 源目录。
- 新增日期区间前端校验、Token 预算界限、项目切换时的请求代次保护（旧生成请求结果不可覆盖新项目预览）和成功/失败反馈。
- 对应提交 `de650f54d387a11fbe12556d6c0814b17b4bc787`；**待该 HEAD 的 CI 与 Windows Desktop 真机交互测试**，包括下载权限、可编辑预览、切换项目时并发生成。S3.2/S3.3 尚缺真实多来源完整报告、独立的内容质量基线与用户环境验收，不能标成全部完成。
- S3.4 仍需补齐可审计、可撤销的自然语言受控知识操作。S1.3 完整备份仍保持暂停，最后实施，不开放入口。


### 2026-10-10 S3.4 来源证据边界与 CI 核查

- CI `38047715942` 四个作业均成功（Rust 质量和测试、Windows Desktop 编译、仓库卫生、Desktop 前端），Windows Provider Contracts `38047715904` 成功；对应 HEAD `e5e4672572a7bd06e26a91b5047df595771d1a3e`。
- S3.4 的模型上下文构建层新增独立来源身份校验：拒绝空来源、重复 ID、非法 ID、超量来源；将知识标题内换行转为空格以防伪造上下文资料段落，保留原文证据。
- `knowledge-organization-audit.test.ts` 补齐上述负例。原有的来源内容版本复核、人工审核后创建独立知识、归档与恢复继续保留；未开放自动改写原文或批量删除。
- **尚未宣布第一步完成**：S2.1 真实模型提炼质量、性能与重试压力数据仍未收齐；S3.4 的用户操作体验及跨会话安全恢复仍需集成/实机验收；S1.1/S1.2 的真实内嵌 SiYuan 与 Windows 场景未验收。S1.3 完整备份保持暂停，不提前暴露入口。

### S1.2 持久任务 Lease 领取代次保护（2026-10-11）

- 对 `pipeline_job` 的心跳续租、成功提交、失败/重试状态转移增加原子 SQL 领取代次 `attempt`、`RUNNING` 状态和未过期 Lease 条件；旧 Worker 不得续租、完成或更改后继代次的任务状态。不新增 migration。
- Worker 在心跳发现租约失效或续租报错后停止继续执行当前异步 Pipeline；只有成功占有有效 Lease 的失败处理才能更新 Pipeline 回退错误状态。
- 增加超时 Lease、重新领取后的陈旧成功/失败/续租拒绝与重试次数的回归测试。此记录仅描述代码级变更，CI 结果以后续工作流为准；异步阶段已执行的外部副作用仍需在真实故障注入与 Windows/SiYuan 环境继续验收。
- 保持 S1.3 完整备份/恢复最后开发，不开放备份入口；六项优化、五项功能及实机验收仍未完成。

### 2026-10-11 S1.2 Lease 重试预算与 CI 回归

- 上一提交 `0d063290` 的 Windows Provider 契约检查成功，但综合 CI `38102164925` 在新增测试的 `cargo fmt --all --check` 阶段失败，Clippy 和 Rust tests 因此未执行；不将 Windows Desktop 编译通过等同于 Rust 测试通过。
- 本批根据日志修正测试格式，并统一 `claim_next` 与独立恢复的 Lease 超时逻辑：超时领取按已消费的 `attempt` 计入上限，第三次超时应终止为 `FAILED`，不允许崩溃后无界重新领取；未耗尽预算的 Lease 仍可安全恢复。
- 增加第三次超时、重复恢复幂等、明确重新入队可用等 SQLite 回归用例。未改变表结构，不写 Provider、不覆盖 SiYuan 原文；新提交的综合 CI 和 Windows 实机仍待验证。

### 2026-10-11 S1.2 CI 失败定位与超时诊断补全

- HEAD `e1c912a3` 的 Windows Provider `38103074206` 已通过；综合 CI `38103074202` 因 `p1_pipeline_job_queue.rs` 一处 `rustfmt` 闭包换行要求失败，Rust Clippy/tests 均未执行。按原样应用 CI 提供的格式修正。
- 对可恢复的 Lease 超时记录固定非敏感诊断原因（`Worker lease expired; retry scheduled`），避免只残留先前无关错误；第三次超时仍保留终止原因。测试增加超时诊断及终止后显式新任务从 attempt=1 开始的断言。
- 代码已提交仍须以相应新 HEAD 的 GitHub Actions 结果验证；不会将 CI/模拟测试视为 Windows 真机或真实模型通过。S1.3 完整备份继续保持暂停、最后实施。


### 2026-10-11 S2.1 真实模型质量/性能验收入口增强

- 在已存在的 opt-in `extraction_live_benchmark` 基础上完成独立报告模块：按样例与重复轮次统计真实请求失败、生产 Schema 有效率、工具噪声跳过正确率、技术术语命中、完全同名重复知识、请求 P50/P95 和仅来自服务端 usage 的 Token。全部指标来自实际返回值，不用固定输出冒充真实模型。
- 支持 `AIKS_BENCH_REPEATS=1..5` 与显式 `AIKS_BENCH_REPORT_PATH`；JSON 文件 create-new，拒绝覆盖已有报告，不保存会话原文、模型输出、API Key 或端点 URL。未运行真实模型时只有合成评分/报告测试，不会自动访问任何模型端点。
- 新增 `docs/acceptance/extraction-live-benchmark.md` 作为 Windows 真机验收入口说明，声明术语命中不等于准确率，模型请求时延不等于整体 Pipeline 时延，匿名化质量和真实业务质量需分别验证。保持 S1.3 完整备份最后实施。
- 本次仅完成评测工具代码和自动化测试入口，**尚无真实模型实测结果、人工遗漏率评审或 Windows 实机验收数据**；新 HEAD 综合 CI 仍须确认，不勾选 S2.1 完成。

### 2026-10-11 S2.1 质量门禁修复与 CI 状态

- 综合 CI `38104969064` 在 `extraction_live_benchmark.rs` 的五处 `rustfmt` 检查失败；Windows Provider `38104969158` 成功，综合 CI 的 Clippy/Rust 测试未运行。根据 CI 原始 diff 逐处修正，不把 Windows 编译成功误认为 Rust 测试成功。
- 修复实时模型评测的正例判定：模型声称 `worth_extracting=true` 但知识列表为空时不能计入正确提炼；增加生产 schema 可以通过但实际没有提炼到应有内容的负例回归，保留原始 `term_total` 遗漏统计。
- 更新对应验收文档。新 HEAD 仍需通过完整自动化 CI，真实模型输出/Windows 实机评审没有执行；完整备份继续留待最后。

### 2026-10-11 S2.1 实时评测 CI 失败收尾（最新 HEAD 待自动化验证）

- 检查 `a0440174` 的 `38105589829`：Windows Desktop、前端和仓库卫生均通过；Rust 质量任务在唯一一处 `cargo fmt --all --check` 代码换行失败，Clippy/Rust tests 被跳过；Windows Provider `38105589845` 成功。
- 本次按 CI 的格式差异直接修复 `extraction_live_benchmark.rs`，保留上轮已经实现的正例空知识项拒绝逻辑及其回归断言。未来必须以新 HEAD 的完整 Rust 测试和 Clippy 结果判定是否合格。
- 未执行真实模型或 Windows 实机评测；S1.3 完整备份与恢复仍待全部其他功能/优化后最后实施。

### 2026-10-11 S3.2 / S3.3 项目报告与 Agent 知识包按来源筛选

- 基线 HEAD `f662994c` 的综合 CI `38106036554` 与 Windows Provider `38106036564` 全部通过。根据尚未实现的「项目+来源」报告范围需求，在已有工作台新增按实际 Provider 来源筛选回顾和 Agent 上下文。切换来源时清空旧预览、作废未完成请求，避免用户将别的来源输出误作当前导出。
- Rust Core 通过 `render_project_review_scoped` 与 `render_agent_context_scoped` 校验来源必须存在于当前项目，过滤原始证据后再渲染，拒绝伪造来源；保留旧函数和未指定来源的兼容行为。Markdown 输出注明来源范围；有界快照已截断时，筛选后仍保留不完整提醒。
- Tauri 可选参数、TypeScript API、Mock 和回归覆盖此链路；不改 SQLite migration、不写 Provider 源、不自动修改 AGENTS.md/CLAUDE.md 或 SiYuan 原文。
- 新提交尚需综合 CI，真实跨工具 Windows/SiYuan 质量与体验验收仍待集中进行；完整备份/恢复继续最后实施。

### 2026-10-11 S3.2/S3.3 来源过滤完整性与 CI 复核

- `8108e17` 的 Windows Provider `38106785877` 已通过，但综合 CI `38106785880` 在新增 Rust 测试的 `rustfmt` 处失败，Clippy/Rust tests 被跳过。本次按照 CI diff 修正所有四处格式。
- 实质修复：项目全来源前 200 条列表先截断再按 Provider 过滤，会让低频数据源的回顾和 Agent 上下文漏掉本来存在的知识。新增 `ProjectMemoryService::get_scoped`，来源需属于选定项目，并在 SQL 层**先过滤来源再按 200 条上限截断**。Tauri 的两个生成命令调用该路径；原有 `get` API 保持兼容。
- 增加真实 SQLite 数据层回归，覆盖跨 Provider 排序、来源范围、来源内截断、未知来源拒绝及未指定来源兼容。无 SQLite migration、无 Provider/SiYuan 原文写入；S1.3 完整备份最后实施，Windows/真实模型验收尚未执行。

### 2026-10-11 S3.2 项目历史回顾范围与 CI 格式修复

- 最新远端 HEAD `bf19966e` 已实现按来源与日期范围在 SQL 证据读取前过滤，避免历史区间被最新项目知识的有界样本挤出；CI `38109390183` 仅在 `project_memory.rs` 的 `anyhow::ensure!` 一处 `rustfmt` 失败，Rust Clippy/测试尚未运行。Windows Provider `38109390182`、Desktop 前端/Windows 编译和仓库卫生已通过。
- 本轮根据 CI 原始格式 diff 修正唯一阻塞，维持该完整模块已编写的真实 SQLite 范围、非法日期和来源隔离回归；无 Schema 迁移，Provider 只读，SiYuan 原文保护不变。最新提交的 Rust 测试及完整 CI 尚需单独验证。
- Windows 真机和真实模型质量/性能验收尚未执行；S1.3 完整备份/恢复留至所有其他功能、优化的代码级交付之后。

### 2026-10-11 S3.1 项目记忆主题归档与可追溯更新时间线

- 综合 CI `38110213966` 和 Windows Provider `38110213961` 均在 `aae3ce20` HEAD 成功；在同一现有功能分支继续 S3.1 的实际 UI 业务缺口。
- 项目长期记忆增加按现有知识类别确定性归组的主题概览：架构/决策、环境配置、实施、问题解决、研究、明确标记的约束与待办、其他。仅根据结构化 `category` 字段分类，不利用自由文本推断已完成事项、未完成任务、技术事实或矛盾结论。每条知识只属于一个组，未知类别归“其他”。
- 增加类别筛选与用户人工质量标记的组合过滤，以及按**知识最后更新时间**排序/分组的独立时间线；同时间使用稳定 Knowledge ID 排序，缺失或不合法日期单列“时间未记录”。列表与时间线都保留知识详情、原始 Session 的可点击证据入口，项目切换时重置视图范围。
- 主题计数/时间线严格基于后端已加载最多 200 条知识样本，明确提示截断，不能视为项目全量、知识生效时间或事件发生时间。保留 S3.2/S3.3 已存在的按来源和日期范围 SQL 后端生成逻辑，不复制或改写原 Session/SiYuan 文档。
- 新增独立 TypeScript 过滤、类别、时间线与无效时间的回归用例，及 `docs/acceptance/project-memory-topic-timeline.md` 验收入口。无迁移、Provider 写入或新云依赖。本批尚需 CI 与实际 Windows/真实 Session 确认，S3.1 不能直接宣称完全验收；S1.3 完整备份/恢复仍按用户要求最后实施。

### 2026-10-11 S3.3 Agent 指令文件安全建议片段与预览过期保护

- 基线 `af813f60` 的综合 CI `38111416816` 和 Windows Provider `38111416827` 均通过。复核 S3.3 发现：虽然已有通用 Markdown 上下文包、可编辑预览和手动复制/保存，但还缺可直接选择的 `AGENTS.md` / `CLAUDE.md` **建议片段格式**，并且切换报告日期或 Agent Token 预算不会让旧预览失效，容易导出参数不一致的旧草稿。
- 本批提供独立的纯 TypeScript `agent-export.ts`：通用 Markdown 原样保留；建议片段含目标类型、人工审核警示，并将所有外部/模型知识正文逐行放入 Markdown blockquote 作为**待核实的引用证据**，不当作执行指令。始终生成独立的 `AIKS-<project>-AGENTS-suggestion.md` / `CLAUDE-suggestion.md` 文件名，不直接生成或覆盖项目目录的 `AGENTS.md` / `CLAUDE.md`。
- 项目记忆工作台增加三个明确格式选项，继续使用已有 Core 后端（可选来源、边界内 Token 预算与质量筛除），只对后端返回的可审核正文作显示封装。切换项目、来源、日期范围、Token 预算或建议格式时清空已编辑的旧预览，并递增请求代次使未完成的旧返回无法覆盖新上下文；实际复制/保存仍由用户点击触发。
- 新增匿名化字符串、伪造标题/指令、跨平台不安全文件名与手动导出格式等前端回归测试，附 `docs/acceptance/agent-context-suggestion.md` 集中验收入口。保持个人本地模式、Provider 只读、SiYuan 手工编辑保护、SQLite migration 兼容，未开放任何自动 Agent 指令文件写入、Release 或 main 合并。
- **代码级新实现仍需本轮综合 CI 验证**；真实项目 Agent 是否理解历史背景、真实模型提炼质量和 Windows 实机运行均待最终集中验收。其他未完成项继续实施，S1.3 完整备份与恢复最后实施。

### 2026-10-11 S2.4 「问 AIKS」按来源证据范围检索与引用完整性（代码级）

- 上一 HEAD `73dc1d23` 的综合 CI `38111962832` 和 Windows Provider `38111962833` 均通过。本批在已有项目过滤、统一混合检索和来源跳转基础上补齐 **Provider 来源过滤**，避免跨来源的 Session/Knowledge 误混入本次检索。Rust `RagAskRequest` 保留可选 `project` 的旧版 JSON 兼容，并新增可选 `source`；trim/长度校验后将两者作为绑定参数传给既有 `UnifiedSearchService` 的 Knowledge 与 Session 双语料，不创建第二套检索和状态。
- 「问 AIKS」来源选项来自本地项目 Session 元信息（不是硬编码或知识表前 200 条猜测），可选择所有来源/某个 Provider；切换项目或来源即清空旧对话，避免把另一查询域的先前答案当作当前历史。Tauri 的流式/非流式 API 都复用同一 Core RAG 请求结构；浏览器 Mock 检索同步实现真实项目+来源过滤而非忽略用户参数。
- 新增模型引用编号完整性审查：最终回答中 `[n]` 必须在后端实际选入的证据编号内；检测虚构编号或没有有效编号时标为 degraded 并附显式警告，不创建假的来源链接。UI 不再只显示第一条 warning，以免“语义降级”遮住模型引用错误。该检查**不能证明模型结论真实或与证据对应**，仍须人工核验。
- 新增真实 Core 单测、浏览器 Mock 作用域回归和 UI/类型连通性回归，新增 `docs/acceptance/rag-source-provenance.md` 验收入口。没有新 SQLite migration、不写 Provider、SiYuan 原文不被 AI 自动覆盖，也不引入新数据库、强制联网或 Release。
- **本批新 HEAD 的完整 CI 仍待执行/确认，真实模型提示词有效性、召回质量/时延和 Windows 实机跨来源人工验证未执行**；S2.4 尚不等于已完成所有真实验收。S1.3 完整备份与恢复仍需所有其他功能/优化完成后最后实现。

### 2026-10-11 S2.4 CI 格式与大编号引用保护

- `a8294eae` 的 Windows Provider `38112690353` 成功、Desktop 前端及 Windows 编译成功，综合 CI `38112690367` 仅在 Rust `cargo fmt --all --check` 的 4 处格式检查失败，Clippy 和 Rust tests 尚未执行。根据 CI 原始 diff 修复 4 处格式。
- 补齐模型引用完整性的一处边界：以前只识别不超过 3 位的 `[n]`，`[1234]` 或整数溢出编号可能不被检查；现在识别任意位数字编号，对不存在的编号和超出整数范围编号分别发出可见警告，保持实际检索证据列表不变，不把模型输出当成数据库来源。
- Rust 单元测试新增大编号/溢出引用回归，验收文档补充相应真机用例。无迁移或 Provider 写回；新 HEAD 自动化仍待验证，S1.3 完整备份最后实施。

### 2026-10-11 S2.4 证据二次范围复核与 CI 格式修复

- 当前 HEAD `6408b295` 的 Windows Provider `38113198262` 已通过，综合 CI `38113198256` 因新增引用告警与测试两处 rustfmt 差异失败，Clippy/Rust tests 未运行。按 CI 原始格式 diff 一并修正。
- 发现范围过滤只发生在 Unified Search 候选返回时，RAG 在单独读取全文片段阶段未复核对象当前状态；若两个阶段之间知识被归档、源 Session 被标为缺失或更换项目/Provider，存在陈旧证据被送进模型的 TOCTOU 风险。本批在读全文前按 Knowledge/Session 类型、当前 active/missing 状态、项目和来源精确过滤条件执行只读 SQLite EXISTS 校验，拒绝陈旧候选。
- 新增真实 SQLite 状态变化负例回归，覆盖知识归档、Session 来源变化、项目变化、来源缺失和无效 ID，不修改数据库/Provider/SiYuan 原文；新 HEAD 的完整 CI 与真实 Windows/SiYuan 并发测试尚需独立验收。S1.3 备份/恢复仍最后实施。

### 2026-10-11 S2.4 RAG 证据读时失效保护与 rustfmt 清障

- HEAD `d88f5924`：Windows Provider `38115039027` 成功，综合 CI `38115039010` 在 `rag.rs` 三处测试 SQL 的格式检查失败，前端、仓库卫生、Windows Desktop 编译均成功；Clippy/Rust tests 因 fmt 未运行。本批一次修复三处 rustfmt 差异。
- 同时排除残留的过期证据兜底问题：先前检索命中在读取真实 Knowledge/Session 内容时若已被删除、移动或清空，对 `None` 使用搜索阶段旧 snippet 兜底，可能绕过此前加入的实时身份复核。现在要求从 SQLite 当前实体/Chunk 确实读到非空原文才能纳入模型上下文；读取失败时忽略候选，不使用旧索引片段代替真实来源。
- 保留原有检索阶段和模型提示词接口及受控证据数量/长度上限；此变更不写 Provider、不覆盖 SiYuan 编辑、不更改 SQLite migration。对应自动化仍需新 HEAD CI 验证；Windows 与真实模型人工证据准确性验收尚未完成。完整备份仍最后实施。

### 2026-10-11 S1.2 处理中心安全诊断包：可审核状态聚合

- HEAD `5dc17899` 的综合 CI `38117811333` 和 Windows Provider `38117811261` 均成功。检查发现处理中心此前只有 `aiks-task-diagnostics-v1` 的六项全局计数，没有来源/同步和 AI 状态分布、重试和耗时分布，也无法从共享诊断中判断哪些类型的故障占主导。
- 新增 `task-diagnostics.ts` 聚合器，严格**正向字段白名单**：全库任务统计与最近最多 200 条任务的按来源（固定允许集合）、同步状态、AI 状态、故障类型、尝试次数分布、阶段延迟 p50/p95。未知来源/状态归并为固定标签；错误正文、原始会话 ID、标题、路径、源文件、历史 Prompt、服务地址、Token 和各项可识别身份均不复制到 JSON；任何自由文本字段均不序列化。
- 处理中心沿用只读 `getTaskCenterEntries` / `getTaskCenterStats`，打开预览时创建**冻结快照**。轮询更新不会替换已经审核的导出内容；手动点击「更新预览快照」才重建 JSON。只有看到预览后可显式复制或保存独立 JSON 文件；不写入项目目录或 SiYuan。注明统计和有界样本不同范围，非全量诊断且非已验证的性能基线。
- 增加字段脱敏、恶意来源字符串、边界数量、故障计数与 p50/p95、空样本和用户预览行为回归，提供 `docs/acceptance/task-diagnostics-safe-export.md` 验收步骤。本次不涉及 migration 和 S1.3 备份；新 HEAD 的 CI、真实 Windows/SiYuan 与工作负载故障注入仍待确认，不能宣称 S1.2 完整实机验收。

### 2026-10-11 S3.1 项目知识演进视图：人工确认与待审核关系

- 基线 HEAD `5b512784` 的综合 CI `38118508697` 与 Windows Provider `38118508692` 已通过。在现有跨 Provider Project Memory 工作台补齐原来缺少的项目级「修正 / 替代 / 解决」证据列表；**不将标题相似、模型摘要或未经验证的猜测视为矛盾事实**。
- 新建只读 `ProjectEvolutionService`，依赖现有正式 migration 013 的 `knowledge_relation` 与当前 Knowledge/Session SQLite 记录；仅展示 `corrects/supersedes/resolved_by` 和 `confirmed/suggested`，拒绝 rejected、已归档 Knowledge、缺失 Session。每条关系的**两个 Knowledge 端点**都重新用当前 canonical session source/path 计算项目身份，必须属于所选同一项目；Session 迁走、来源丢失后不会错误混入旧项目。原始关系审核表和知识内容保持不变。
- Tauri 命令 `list_project_evolution`、TypeScript API/Mock 与项目记忆 UI 一并接入。明确区分用户确认与待人工审核、提示审核时间不等于生效时间，提供双方 Knowledge 详情入口和证据文本。结果最多展示 100 条且提示截断，项目切换时抛弃旧的异步结果；不自动确认、发布、删除、覆盖任何知识或 SiYuan 原文。
- 增加真实 SQLite 回归覆盖同项目跨 Provider、跨项目假关联、确认/建议/拒绝状态、项目迁移、Session 缺失、知识归档与截断行为，另有前端交互/API 边界测试和 `docs/acceptance/project-evolution-reviewed.md` 验收入口。无 Schema 迁移、Provider 只读、个人离线模式不变。
- 本模块仍需新 HEAD 综合 CI 和真实 Windows/内嵌 SiYuan 跨来源验收；项目手工归属修正及自动矛盾候选检测等更大功能仍待独立核对，不能把本项标作整个 S3.1 完成。S1.3 完整备份/恢复继续排在所有其他代码模块最后。

### 2026-10-11 S3.1 演进关系 CI 修复与项目内人工审核闭环

- `b60000dd` 的 Windows Provider `38119254947`、Windows Desktop 编译、前端与仓库卫生已通过，但综合 CI `38119254904` 的 Rust fmt 报告 `project_memory_commands.rs`、`knowledge/mod.rs` 与 `project_evolution.rs` 中导入排序、导出换行、元组解构及测试元组布局不符；Rust Clippy/tests 因而尚未运行。本次直接按完整日志修正全部差异。
- 项目知识演进原来只有展示，人工需另跳转知识详情才能审核建议。本批补齐项目内审核闭环：只对 suggested 关系显示「核对原文后确认关系」「拒绝关系建议」；点击需要显式确认，操作写入**已有** `knowledge_relation_review` 审计日志并刷新只读演进视图，拒绝后关系不再显示。按钮处理期间防重复提交；项目切换的异步返回不会污染新项目。
- 新增前端接口/交互边界回归；现有 Core SQLite 回归保持原关联身份、状态和跨项目隔离要求。不得自动确认、删除知识、修改 Provider 原始文件或覆盖 SiYuan 用户文档；无需 migration。新 HEAD 自动化和 Windows 真机人工审核体验仍待验收，S1.3 完整备份最后开发。

### 2026-10-11 S3.1 人工项目归属修正（仅本地覆盖，含撤销）

- 基线 HEAD `59fa109f` 的综合 CI `38119878967` 和 Windows Provider `38119878971` 成功。在现有以 Provider cwd/project_path 归集的 Project Memory 模块中补齐此前缺失的**用户手工修正**：通过项目记忆页选择一个本地 Session 和一个已有**绝对路径且仍然存在**的目标项目，显式确认将此 Session 作为该项目的记忆成员；可随时撤销恢复自动归组。没有自动识别同名项目或擅自合并路径不明 Session。
- 新增向前兼容的幂等 SQLite migration `014_project_session_binding.sql`（StateDb V15），存储本地 Session ID、用户选择的目标项目路径和更新时间。Tauri 只接受 Session ID 和**不暴露目录路径的现有项目不透明 ID**；Core 在 SQLite 事务内核验来源仍存在、目标实际是当前发现的绝对路径项目，拒绝不存在的项目/Session，避免调用方传入任意目录或注入路径。覆盖表与源 Session 表分离，**不修改** canonical `source_session.project_path`、任何 Provider 文件、SiYuan 用户原文/Workspace 或知识 ID。无自动发现回写。
- Project Memory 列表、知识样本/报告/Agent 导出和跨 Session 关系读取使用 `COALESCE(binding.project_path, source_session.project_path)` 计算**当前本地归组**；关联关系在人工确认时同样使用已修正项目身份约束，不能绕过跨项目隔离。绑定删除后原始 Provider 路径立刻生效，旧关系与知识不被删除。
- Desktop 项目记忆页增加明确的 Session/目标项目选择、手工确认、已绑定状态及撤销；来源候选最多显示 200 条、提示列表不完整；归属成功后重载项目概览/证据与关系视图。Mock/API 类型/命令和后端 SQLite 持久性、重开数据库、非法对象拒绝、撤销及原始目录不变回归一并添加。
- 新增 `docs/acceptance/project-session-binding.md` 作为 Windows 真实环境验收入口；迁移旧库数据不搬迁、不覆盖，SQLite 新表可以重复创建；本次新 HEAD CI/Windows 实机集中验收尚待核查。不将单元测试或 GitHub Windows 编译冒充用户机器三 Provider 真机验证；S1.3 完整备份与恢复仍安排在其他模块代码级收尾后最后实施。

### 2026-10-11 S3.1 手工归组 CI 格式修复与身份元数据一致性

- 当前远端 HEAD `715f7609`；综合 CI `38120701419` 的 Windows Desktop 编译、Desktop 前端和卫生检查成功，Windows Provider `38120701418` 成功，Rust 质量任务仅在 `project_binding.rs` 两组测试断言的 rustfmt 换行失败，Clippy/Rust tests 尚未开始。本批严格按该 CI 原始 diff 完整修正格式，未将 CI 失败声称为 Rust 测试通过。
- 检查项目人工归属后的真实业务一致性发现：项目分组的展示标题取本组先遇到的 Session，而原 SQL 按最近 `source_session.updated_at` 排序。如果绑定过来的 Session 较新且它的旧项目名与目标项目不同，可能错误把目标项目标题改成旧名字。修复为先从仍然按 Provider 原始绝对路径归组的 Session 选择项目显示标题，再聚合手工绑定成员；组内来源、数量、知识和分组身份仍完整计算，用户人工归属不会更名原项目或更改 canonical 路径。
- 同步修复前端 Mock 数据的一致性：只按 canonical 未绑定 Session 优先确定项目标题，项目知识计数根据实际源 Session 所属组汇总，而非靠可变项目显示名字符串推断。增加迁入/撤销 Session 时知识数、归组证据与原始来源 `project_name` 不变的回归测试。这些只是 SQLite/Mock 代码验证，不是用户真实 Windows 试运行或模型质量测量。
- 新 HEAD 自动化必须独立验证；沿用 local-first、Provider 只读、SiYuan 人工文档不覆盖、V15 migration 前向兼容和完整备份 S1.3 最后实施的约束。

### 2026-10-11 S3.4 受控多来源知识整理：后端权威项目身份校验

- `2b80a7f` 的综合 CI `38121719301` 与 Windows Provider `38121719376` 均成功。再次审查受控多来源整理入口发现：已有 `validateOrganizationSources` 仅比较客户端获取的可变 `project_name`，未校验底层真实 Session 工作目录。两个**同名不同绝对路径**的项目知识可能被当作同项目传给用户配置的模型并创建合并草稿，存在跨项目证据混合风险。
- 本批实现独立 `OrganizationScopeGuard`，后端直接查询**当前** `knowledge_item`、`source_session` 与 S3.1 已有的人工项目归属 `project_session_binding`。拒绝无效/重复/超过 5 个知识 ID、归档知识、缺失 Session；对于多来源要求双方当前都有**相同已验证绝对路径身份**，或者确实来自同一个 canonical Session。来源是手动知识或路径无法核实时只允许单条整理；不能仅凭展示标题/项目名相同合并多个来源。
- 通过只读 Tauri 命令 `validate_organization_scope` 和 TypeScript API 调用，在**模型请求前**、以及**用户审阅草稿后即将创建独立知识前**分别验证一次。原有前端内容快照/人工确认/双击写入门禁继续使用；这项改动补足权威项目身份门禁，不承诺跨网络/SiYuan 事务的完全原子化。若校验失败应显式提示用户并停止该次流程，不自动更改任何资料。
- 包含真实 SQLite 测试：同名不同路径拒绝、跨 Provider 同路径接受、归档/缺失拒绝、人工归属覆盖后安全通过、原始 Provider 路径不变、无来源/手动知识与超限拒绝。Mock 与 UI 回归验证两次后端调用，新增 `docs/acceptance/organization-source-scope.md` 作为 Windows/真实模型负例入口。
- 不涉及数据库迁移，不把模型输出、固定 Mock 或 CI 测试结果视为 Windows 真机与真实模型验收。仍保留个人离线、Provider 只读、SiYuan 原文人工编辑保护；S1.3 完整备份和恢复继续最后实现。

### 2026-10-11 S3.4 最终创建入口的后端来源强制校验与 CI 格式修复

- HEAD `ed4ba933` 的 Windows Provider Contracts `38122454943`、Desktop 前端、Windows 编译及仓库卫生均成功，但综合 CI `38122454975` 仅在 `organization_scope.rs` 4 处 `rustfmt` 差异失败，Rust Clippy/tests 尚未执行；本批按原始日志一次修正全部 Rust 格式问题。
- 复核受控知识整理链路：原本 UI 在模型调用前及写入前已调用 `validate_organization_scope`，但最终写入仍走通用 `create_knowledge`，Tauri 命令本身并不要求携带任何证据来源身份。这意味着调用方绕过前端校验或使用旧调用路径时，可能把未经核验的跨项目合并草稿作为已审核结果提交。
- 新增专用 Tauri `create_organized_knowledge`，**必选 `sourceKnowledgeIds`**，在最终 SiYuan 创建动作前再次由 Rust `OrganizationScopeGuard` 查询当前本地 SQLite 活跃知识、Session 缺失与项目/人工归属；来源为空、重复、过量或跨项目一律拒绝。Tauri 不接受从前端直接传入项目真实路径。原 `create_knowledge` 命令原样保留给普通用户手动创建以确保 API 兼容，不把多来源审核规则施加到普通文档。
- S3.4 UI 仅针对经用户预览确认的整理草稿改用专用创建命令，传入已审核的原始知识 ID。保留前端完整内容快照/来源复核/确认/防双击门禁，以及生成后单独归档或恢复。前端 Mock 和接口同步实现强制证据校验，增加被绕过的空来源、同名不同目录、正常同项目整理创建及用户普通手工创建不受影响的回归。
- 此门禁是**最终写入请求开始前的**来源合法性检查，不是覆盖后续 HTTP/SiYuan 异步时段的原子锁；真正的并发故障注入、真实模型质量、SiYuan 远端人工编辑保护仍须 Windows 真机集中验收。没有新的 SQLite migration、Provider 写回、main 合并或 Release；S1.3 完整备份与恢复继续最后实施。新提交 CI 尚待执行/确认，不能宣称已通过。

### 2026-10-11 S3.4 人工整理记录持久化与重启后归档/恢复入口（代码级）

- 已确认基线 `3cfc40fa` 对应综合 CI `38122920089` 与 Windows Provider `38122920088` 全部成功，远端 HEAD 未变化。本批继续补齐 S3.4 的**实际业务缺口**：此前整理草稿创建后，仅保存 `createdKnowledgeId` 在 React 临时状态；关闭窗口或切换知识后无法再从原始知识视图查找此次整理操作、恢复与撤销入口。Markdown 内虽然附有用户审核备注，但无法可靠检索派生条目与关联状态。
- 新增 additive、幂等 SQLite V16 migration `015_organization_audit.sql`，独立持久保存原始知识到审核后新建文档的**顺序来源映射**、操作类型、用户确认时间；不修改现有 Knowledge ID、源 Session、原始 SiYuan 文档或 Provider 文件。新库和旧库均可重复打开，旧数据无需破坏性重建。
- Core `OrganizationAuditRepo` 强制审计操作白名单、来源 ID 数量/去重/格式、比较或合并必须有至少两个来源、新文档必须确实已存在且是 active/user-managed/manual，事务内一次写入审计和来源映射；主键禁止重复记录冒充第二次审核。历史查询从**当前 canonical Knowledge.status** 读取 active/archived/missing，绝不维护伪造的第二套归档状态；源文档变化与重启不删除审计记录。
- Tauri `create_organized_knowledge` 在请求 SiYuan 前校验操作和来源；新建成功后记录本地不可覆盖的审核关系。若远端文档**已经创建**、但本地审核写入失败，明确返回新文档 ID 与「不要重试」说明，防止前端重试生成重复 SiYuan 文档。新增 `list_organization_audit` 只读查询；普通 `create_knowledge` 不受审核条件影响。
- Desktop 知识详情展示当前知识相关的最近最多 100 条持久整理记录，提供重新打开独立知识、人工归档、恢复当前状态入口；旧的当次创建后归档仍兼容，新增审核记录变更时刷新查询。经用户确认的归档只改变 AIKS 本地新知识状态，不自动删除 SiYuan 文件或其他原始知识。前端 Mock/接口及 Rust/TS 回归含数据库重开、来源次序、重复拒绝、归档恢复、无效操作与新命令门禁。
- 已补充 `docs/acceptance/organization-persistent-audit.md` 真实 Windows + SiYuan 故障注入入口。本批没有运行真实模型或 Windows 用户环境；新 HEAD CI 及跨 HTTP 不确定提交情况仍须验证。**当前方案不能保证 SiYuan HTTP 写入与 SQLite 审计原子提交**，更高等级的异常中断/跨机一致性仍需真实验收和后续恢复策略复核。S1.3 完整备份恢复仍最后实施。

### 2026-10-11 S3.4 V16 审核记录 CI Clippy/TypeScript 修复

- 上一 HEAD `04bb4943`：Windows Provider `38123961195` 成功，综合 CI `38123961198` 失败。**Windows Desktop 编译、仓库卫生、Rust 格式、前端 152 项测试成功**，但前端 TypeScript 构建因 `AiAssistOperation`（含 `summary/title/tags/category`）被赋值给只允许 5 种「整理操作」的持久审计字段而失败；Rust Clippy（`-D warnings`）拒绝一处单元素 `[first.clone()]`，Rust tests 因 Clippy 失败未运行。两项均来自工作流原始日志，而非推断。
- 一次批次修复两个独立 CI 阻塞点：新增 `ReviewedOrganizationOperation` 编译期受限类型及运行时收窄判别函数，**只有** `structure/rewrite/key_conclusions/compare/merge_draft` 可以进入已审核的整理审计；普通 `summary/title/tags/category` 仍可用于原有 AI 辅助，但不允许伪装为已人工确认的跨知识整理。Rust 单元素切片改为 `std::slice::from_ref`，不关闭 Clippy 规则或跳过测试。
- 增加五种允许操作和四种非整理操作的负例回归，继续保留非法来源拒绝及正式 SQLite V16 审计数据层；未新增迁移、对 Provider/SiYuan 的写回或新的云依赖。新 HEAD 尚需综合 CI 和 Windows Provider 独立确认，真实模型/Windows 真机验收仍待集中进行；S1.3 完整备份和恢复继续留至其他代码级模块最后。

### 2026-10-11 S3.4 已创建文档的审计修复（不重建 SiYuan 文档）

- 基线 `0ee7146e`：Windows Provider `38124472173` 已成功，综合 CI `38124472136` 仅因 `organization_audit.rs` 单处 `rustfmt` 换行失败，Clippy/Rust tests 尚未运行；修复该格式差异并把恢复流程作为一整批收尾，避免单次格式提交浪费 Actions。
- 实际业务缺口：当独立 SiYuan 知识创建成功但本地审核记录写入失败时，旧 UI 只有报错和新知识 ID，点击重新创建可能产生第二份文档；缺少无需重新触发 SiYuan HTTP 的恢复入口。新增 `OrganizationAuditRepo::recover_existing`：必须由用户提供**现有本地 Knowledge ID**、整理方式和按顺序列出的来源 ID；先重新检查当前 canonical Knowledge/Session/人工项目归属，再核对已创建文档在本地保存的 `## AIKS 整理审核记录` 尾注、操作类型和来源顺序，最后通过既有 SQLite 事务式 `record` 补录审计。已存在记录、伪造尾注、错误顺序、非 active 或非用户管理的目标/来源必须拒绝。**不调用任何 SiYuan 写 API、不修改已创建文档、更不覆盖原始资料**。
- 增加只做本地审计修复的 Tauri API、TypeScript API/Mock、知识详情页明确的人工修复入口。若异常错误中明确返回「知识已创建（ID: ...）」则预填现有 ID，停用旧草稿的重复创建确认。其余不确定 HTTP 错误仍提示先人工查看已有文档，不声称已经创建/已经失败；用户仍须确认修复，避免自动绑定任何来源。
- Rust/TypeScript 回归覆盖已存在且有正确人工审核尾注的单来源补录、重复补录拒绝、错误操作/来源/伪造文档拒绝、补录不增加知识数、重启后来源关联可查、默认 Provider/SiYuan 原文安全边界。真实 SiYuan HTTP 创建成功但 SQLite 未落库的情况，只有文档已被 AIKS 存成本地 canonical Knowledge 记录的**特定失败窗口**才能修复；完全没有本地记录的远端孤儿、跨系统原子事务仍未解决，必须在 Windows 真机实验中实证，不能将这一批当作完成 S3.4 所有故障恢复验收。
- 无新 migration、无 main 合并或 Release，完整备份恢复 S1.3 依用户要求继续最后实施。新 HEAD CI 状态以后续工作流为准。

### 2026-10-11 S3.4 审核补录安全收尾与新旧限额一致性（待新 HEAD CI）

- 远端基线 `47acc101`：Windows Provider `38125244442` 成功；综合 CI `38125244462` 失败仅发生在 `organization_audit.rs` 的 `rustfmt`，Rust Clippy/tests 未执行，前端、Windows 编译、仓库卫生成功。按作业日志修正原有单行断言与来源比较链式换行，并把必要的真实业务安全问题汇集为**一个**提交以降低 Actions 次数。
- 核对恢复功能后发现：旧实现仅匹配正文中的操作文字和来源行，**未核验审核创建时间、审核副本声明或 SiYuan 文档绑定元数据**。一个没有远端创建记录的普通本地手工草稿可能被人工冒充为已创建的审核派生文档。本批收紧为只接受 active/manual/user 且有 `siyuan_doc_id`、`generated_hash` 的本地 Knowledge；严格验证追加的分隔符、审核操作、UTC 时间、保持原文的声明以及按顺序逐项匹配的来源 ID，拒绝任何额外证据行。恢复仍仅调用 SQLite 审核记录事务，不创建也不覆盖远端。
- 在 `create_organized_knowledge` 内对 **SiYuan HTTP 创建返回后** 和审计入库前补一次 canonical 来源/项目归属的后置校验；异步期间来源被归档或用户修正了项目归属则必须作为「已经创建文档但尚未落审核记录」的显式异常返回，避免记录过期身份。不能使跨 HTTP/SQLite 写入成为原子操作。
- 前端草稿、来源快照与模型来源合成的数量上限由原先不一致的 100 项统一为与后端一致的 **5 项**；Mock 模拟真实 SiYuan 创建后的绑定元数据、使用单调知识 ID、防止同毫秒 Mock ID 撞车，并修复模拟编辑后保存的内容读模型不同步，减少 CI/浏览器演示对恢复结果的误判。对应更新测试及人工验收入口。
- 仅对已有本地且带 SiYuan 绑定的知识做人工审计恢复，**不实时联网确认 SiYuan 文档存活或内容未变更**；远端孤儿、HTTP 不确定提交、模型实际输出质量、Windows 真机三 Provider/内嵌 SiYuan 测试仍不得声明通过。保持个人离线、Provider 只读、用户 SiYuan 原文不可自动覆盖、旧数据库兼容；S1.3 完整备份与恢复仍排在其他功能代码级完成之后。

### 2026-10-11 S1.2 处理详情安全重试、首次入队与遗留命令防绕过（代码级）

- 基线 HEAD `aef65cc` 的 Windows Provider `38126077463` 成功，综合 CI `38126077458` 仅在 `organization_audit.rs` 一处 `rustfmt` 换行失败；Windows Desktop、前端和仓库卫生均通过，Rust Clippy/tests 尚未执行。本批根据日志修正该格式问题，并同时交付独立 S1.2 任务处理模块，避免每次格式修复触发单独 Actions。
- 复核发现历史处理详情页将 `FAILED` 和 `RAW_ONLY` 一律动态调用遗留 `run_pipeline_for_session`，而后端该遗留命令只核对 Session 存在，不验证原文同步、SiYuan 编辑冲突、历史 Pipeline/Queue 状态。可能绕过已实现的 `retry_failed_ai_task` 安全重试和持久队列去重，导致历史失败或 READY Session 错误重入队。
- 新增统一前端 `submitProcessingDetailAction`：FAILED 只调用已有持久队列安全失败重试；RAW_ONLY 仅走已收紧的首次/原始入队；READY、PROCESSING、CONFLICT 和非法 Session ID fail closed。历史 UI 改为经 `getApi()` 调用单一路由，不再自行 `import("@tauri-apps/api/core")` 绕开 API。成功和失败提示分别反馈实际动作，禁止任何安全重试失败时退回无条件入队。
- Rust Core 新增 `pipeline/first_run.rs` 只读检查：源 Session 必须仍在、SiYuan sync_target 是 SYNCED/UNCHANGED、没有 PENDING/RUNNING 持久任务，且不存在除明确 RAW_ONLY 外的现有 V3 Pipeline 记录；遗留 Tauri 命令改为调用该 Core 检查后再走原有 `enqueue_pipeline_for_session`，**不能通过直接 IPC 重放绕过验证**。另给 `retry_failed_ai_task` 加当前 READY/PROCESSING 的优先拒绝，即使数据库仍保留旧 FAILED job，也不得将已完成/当前进行中 Pipeline 当成失败重试。
- 新增真实 SQLite Core 测试覆盖原文未同步/冲突、正确同步、缺失源、历史 FAILED/READY/PROCESSING/DISCOVERED、RAW_ONLY、持久队列 PENDING/RUNNING/CANCELLED；前端 Vitest 覆盖状态路由、错误传播和原生命令契约。无 SQLite 迁移，不创建第二套队列，不修改 Provider/SiYuan 原始数据；新增 `docs/acceptance/processing-detail-safe-retry.md` 作为后续 Windows、嵌入式 SiYuan 故障注入入口。
- 本轮只解决代码边界与自动化测试，**不能把固定 SQLite/Mock 通过视为 Windows 真机连续同步或真实模型通过**；异步入队与用户真实环境的竞态仍需集中验收。S1.3 完整备份/恢复继续等所有其他代码级事项完成后最后开发，不合并 main、不发布 Release。

### 2026-10-11 S2.3 关系来源隔离：阻止同名/无路径 Session 的错误关联

- 基线 `396bd84f` 的综合 CI `38127158109`、Windows Provider `38127158018` 已成功。复核真实 SQLite 的 `RelationRepo::ensure_related_knowledge_is_current` 发现：如果关系的任意一端没有**已核实绝对路径**，旧代码回退为对比可变 `project_name`；不同的 Codex/Claude Code/OpenCode 会话即使都没有来源路径，只要项目显示名称相同就可提交乃至确认关系。若一个端点有可靠路径、另一个没有路径，同名时也会通过。另一个缺口是没有检查源 Session `is_missing`，可继续确认已缺失 Session 所属的旧关系。
- 本批重新定义权威关系边界：两端都有可靠路径身份（含人工项目归属 `project_session_binding`）时必须相同；两端路径都无法核实时仅允许**确属同一 canonical Session** 的两条知识；两端皆为用户手动知识且明示相同非空项目名时，仍允许用户主动提出证据关系，保留原有手工工作流；**不同路径状态的混合、不同的无路径 Session 同名匹配、缺失源 Session 一律拒绝**。人工可以先使用 S3.1 已有本地项目归属修正显式归组，再重新提交。确认时在事务中再次校验当前范围，拒绝时保留原有审核证据而不要求来源可用。
- Rust Core 补真实 SQLite 回归，覆盖同名无路径跨 Provider 拒绝、同 Session 无路径允许、人工归属之后允许、混合已验证/未验证拒绝、源 Session 丢失后禁止新增及确认但允许拒绝；保留现有人工手工同项目关系案例。Desktop Mock 建立同等建议/确认边界，避免前端测试误将显示名作为关系凭据；知识详情候选不再仅凭 mutable `project_name` 隐藏同路径跨 Provider 合法来源，说明候选列表本身不是授权结果。
- `docs/acceptance/project-relation-source-isolation.md` 提供真实 Windows 三 Provider + SQLite/SiYuan 负例入口。**不将来源绑定/同路径关系推导为客观事实矛盾**，关系仍先建议、待用户确认；不修改来源知识、Provider、SiYuan 文档，也不新增 migration。新 HEAD 尚待独立 CI 和 Windows 真机人工确认，S1.3 完整备份恢复继续排在其他代码级功能之后。
