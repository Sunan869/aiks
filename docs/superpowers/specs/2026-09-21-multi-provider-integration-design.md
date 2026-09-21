# AIKS 多 AI 工具会话接入设计

日期：2026-09-21
状态：待用户审阅的设计稿；本提交不包含新 Provider 功能代码，不代表接入或测试完成。
开发分支：`feature/multi-provider-integration`
基线：`main` / `d676480bce84c9326078cd19341b3e7fae1d2b0d`（包含 WorkBuddy 和 PR #45 搜索修复）。

## 1. 用户目标与完成定义

在 AIKS 内接入用户明确列出的全部候选：Antigravity、Cursor、Cursor Agent、Cline、Roo Code、Kilo Code、GitHub Copilot、Kimi Code、Qwen Code、Continue、Aider。它们是 11 个新的来源身份；完成后与现有 Claude Code、Codex、Gemini CLI、OpenCode、WorkBuddy 共存。

这里的接入是本地历史会话只读导入，不是调用这些工具替用户执行任务。一个来源只有完成“发现实际会话 → 读取真实消息 → Canonical Model → 重复/增量同步 → 现有工作记录、知识处理与搜索链路”且有对应格式测试，才可记为接入完成。添加枚举、数据源卡片或始终返回空数组不能算实现。

用户已确认全部候选范围及复用 AIKS Provider/同步/提炼/搜索的方向。本稿进一步固定版本形态、读取边界、兼容性和验收标准；具体实施计划在本稿审阅后形成。

## 2. 当前代码事实与参考证据

### AIKS 基线

- `crates/aiks-core/src/providers/mod.rs` 已有 `SessionProvider`、`SessionSummary`、`ProviderRegistry`，当前注册五个来源。保留这些作为唯一接入入口。
- `crates/aiks-core/src/config/mod.rs` 当前来源配置采用 `enabled` 与 `path`，普通 Provider 默认 `enabled = true`、空路径使用默认目录。不得让旧配置失效，也不改 AI/Embedding 的 opt-in 默认。
- `crates/aiks-core/src/sync/engine.rs` 按 `(source, external_session_id)` 查找会话、加载 Canonical Session、计算内容 hash，再走现有同步链路。当前来源过滤发生在全 Provider discovery 之后；新增大量来源时要避免“只同步一个来源，仍遍历所有工具”。
- 桌面数据源与工作记录来源选项存在手工枚举；PR #45 刚修复过 WorkBuddy 显示名遗漏。本次来源身份/显示名/筛选值须统一管理，不能复制更多互不一致的映射。
- `AGENTS.md` 约束持续有效：源数据只读、坏会话隔离、显式错误、正式 migration 不原地改写、不得另建 Pipeline/Knowledge/Search 实现。

### 参考实现与事实分层

主要格式参考：`jhlee0409/claude-code-history-viewer`，勘察时默认分支 HEAD 为 `fdfc766ce7f0d76dceb03087aedac47add33d61b`。已查阅的模块包括 `providers/{mod,antigravity,antigravity_cli,cursor,cursor_agent,cline,kimi_code,qwen,continue_dev,aider,copilot}.rs`。

辅助参考：`seastart/aicoder-session-viewer`，尤其 Antigravity CLI 与现有 Canonical Model 适配方式。两项目对 Antigravity 可读日志的描述并不完全一致，不能照抄 README 路径就宣布通用兼容。

官方文档核对：

- GitHub Copilot CLI 配置目录与会话目录：`https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-config-dir-reference`、`https://docs.github.com/en/copilot/how-tos/copilot-cli/cli-best-practices`。
- Google Antigravity CLI 迁移：`https://antigravity.google/docs/cli/gcli-migration/`。该文档证明 CLI 与旧 Gemini 的迁移背景，不是 transcript schema 的官方保证。

已查到的关键差异：

1. Antigravity IDE 与 Antigravity CLI 的根目录、可读文件和消息表示不同；CLI 参考解析器明确说明格式是逆向观察所得。
2. Cursor IDE 使用 workspace/global SQLite 数据；Cursor Agent 的文件型 transcript 是另一来源，不应混为同一数据库解析器。
3. Cline 家族共享部分消息格式，但扩展 ID 和任务索引不同；Kilo 的部分任务索引位于 editor `state.vscdb`，不能只寻找 Cline 的索引文件。
4. Kimi Code 新布局含 `.kimi-code/.../agents/main/wire.jsonl` 事件日志，与旧 `.kimi/.../context.jsonl` 不是换个目录即可复用的相同格式。
5. Copilot CLI/Desktop 可共享本地会话存储；VS Code Chat 则走 editor workspace storage，需要明确来源形态与去重。

这些是设计依据，不是 AIKS 已通过这些格式的实测记录。实施中须继续核对完整解析代码、官方开源实现及匿名 fixture，并将实际参考文件、commit/blob SHA 和覆盖格式记入 `docs/reference-analysis/`。

## 3. 全量接入矩阵

下表中的 key 是拟新增的稳定标识；显示名与 key 分离，发布后不随品牌大小写变化。

| 显示名 | 稳定 key | 目标本地数据形态与范围 |
| --- | --- | --- |
| Antigravity | `antigravity` | CLI 根 `.gemini/antigravity-cli` 的可验证 transcript JSONL；IDE 根 `.gemini/antigravity` 下已存在、能还原消息的可读日志或本地消息缓存。形态写入 metadata。 |
| Cursor | `cursor` | Cursor User 下 global/workspace `state.vscdb`；兼容有 fixture 的 Composer/chat 布局，按会话 ID 合并消息引用。 |
| Cursor Agent | `cursor_agent` | `.cursor/projects/.../agent-transcripts/` 的已确认 JSONL 布局；其他版本的文本/数据库布局须另有格式证据和 fixture 后支持。 |
| Cline | `cline` | `saoudrizwan.claude-dev` 扩展存储中的任务历史、API 对话与 UI 消息。 |
| Roo Code | `roo_code` | `rooveterinaryinc.roo-cline` 任务存储；与 Cline 共用经过测试的消息转换，不共享来源身份。 |
| Kilo Code | `kilo_code` | `kilocode.kilo-code` 任务存储；包括实际版本需要的 SQLite 任务索引回退。 |
| GitHub Copilot | `github_copilot` | CLI/Desktop 的 `session-state/<id>/events.jsonl` 及必要的 workspace 元数据；VS Code Copilot Chat 的 `workspaceStorage/.../chatSessions`。 |
| Kimi Code | `kimi_code` | 新 `.kimi-code` 的 state + wire journal；旧 `.kimi` 中已确认的 context/wire 会话，通过布局适配器分开处理。 |
| Qwen Code | `qwen_code` | `.qwen/projects/.../chats/*.jsonl`，识别 session/cwd/parent UUID 以及消息 parts、思考和工具调用/结果。 |
| Continue | `continue` | `.continue/sessions/<sessionId>.json`，解析 `history[].message` 和已确认的工具记录；跳过索引文件 `sessions.json`。 |
| Aider | `aider` | 用户指定项目根下 `.aider.chat.history.md`；按真实会话头和 Markdown 状态解析，不把代码围栏内标题当会话分隔。 |

### 明确的覆盖边界

- Antigravity 同时纳入 CLI 与 IDE 的可读会话格式，但不启动代理、注入扩展、抓取鉴权令牌、调用非公开在线接口或自行生成 token-monitor 缓存。仅有目录、usage 统计或不可识别 protobuf/BLOB 时，显示“已检测到，但当前格式不可导入”，不生成空会话/伪造正文。
- GitHub Copilot 纳入 CLI、Desktop、VS Code 三种本地形态。读取范围是对话与必要的工作区信息，不导入 plans/checkpoints/任意 tracked files。云端历史下载不在本次范围。
- Kimi 新旧布局需独立 fixtures。事件流的 clear/undo/compaction 与已结束/未结束流式输出要明确还原为可解释的当前会话，不能逐行拼接造成重复回答、撤销内容复活或错误工具归属。原始事件只作为受控的会话内证据保留。
- 不承诺某一工具所有历史/未来版本均兼容。交付时逐形态列出“已验证、部分支持、不支持的可识别版本”；未完成约定消息形态时不得宣称该来源已完整接入。

## 4. 选定方案与替代方案

选择原生 Rust Provider，复用现有 `SessionProvider` 接口，并把适用的路径校验、JSONL 读取和消息 block 转换抽成小型内部公共模块。新增来源经相同 Canonical Model 进入已有同步/知识/搜索，不改写下游架构。

不选择“启动参考查看器后导出”：会新增安装/进程依赖，也难保持实时增量、错误归属和部署可控性。不选择“一个万能 JSON 解析器”：不同工具的事件重放、数据库身份、分支/撤销语义不同，泛化猜字段容易伪造内容或静默漏数据。

允许按许可证复用参考项目的独立解析片段，而不是为追求完全独立实现重复劳动；复用范围、修改点、原作者与 MIT 许可必须同步到 `THIRD_PARTY_NOTICES.md`。不引入它的 GUI、搜索、遥测、命令执行或数据删除逻辑。

## 5. 共同接入契约

### 5.1 来源定义与配置

- 每个来源有稳定 key、正式显示名、provider factory 和可描述的本地读取能力。Cline/Roo/Kilo 是三个独立来源；Copilot 多入口在同一来源下记录 `entrypoint`；Antigravity/Kimi 记录 `storage_layout`。
- Desktop 数据源卡片、工作记录筛选和来源显示从统一定义获得。新增兼容的 descriptor 传输接口；不直接破坏既有 `scan_by_source`、`provider_health` 或 CLI 参数。
- 旧五种来源的 key、配置节、档案路径和记录身份不变，保留 WorkBuddy 正确显示名与当前搜索修复。
- 新来源沿用 `enabled`、`path` 语义。只有需要多根的来源增加有默认值的可选路径列表；原有 `path` 仍作为单根覆盖方式。显式路径高于工具环境变量，再高于平台默认路径；不会忽略错误显式路径而偷偷导入另一处数据。
- 普通固定目录来源沿用当前 Provider 默认启用探测的行为；不因为本次接入打开 AI 或 Embedding。Aider 的递归发现只限用户配置的项目根，不默认扫描整个磁盘或凭 transcript 内 `cwd` 任意扩展读取范围。
- Windows/macOS/Linux 路径解析分离且可注入测试根。WSL 或远端挂载只在用户显式配置的可访问路径内读取，不自动启动 WSL/SSH 或全盘枚举用户目录。

### 5.2 发现、健康与边界

- 只列出明确允许的会话/元数据文件，限制目录深度、工作量与单次读取内存；JSONL 优先流式读取。达到限制必须报告未完成，不静默当成全量成功。
- 明确区分禁用、未安装/路径不存在、有效零会话、格式不支持、读权限/数据库错误和正常可导入。需要新增健康状态时兼容现有 `ProviderHealth::is_ok` 调用，不把未支持状态映射成健康空库。
- 不跟随会逃出配置根的符号链接或 Windows junction。加载阶段再次验证来源与路径，不能让 caller-supplied `source_path` 越过 discovery 边界。
- SQLite 采用只读、有限 busy timeout、WAL-aware 读取；不对运行中的数据库使用会忽略 WAL 的不合适快照假设。禁止修改上游 schema、数据、WAL 或配置。
- 故障隔离至单会话或明确受影响的 store；读失败不能变成“上游已删除”。一处索引损坏不能将其他有效数据源清空。
- 来源过滤应在 discovery 前生效，避免每次点击“仅同步 Qwen Code”都扫描其他 15 个来源。涉及阻塞文件/SQLite 工作时使用适当的阻塞执行边界，而非在异步线程中无界遍历。

### 5.3 身份、内容与增量

- Canonical 唯一身份仍是 `(source, external_session_id)`。优先采用上游真实会话 ID；不同 editor/profile/workspace 中非全局唯一的 ID 必须带确定的命名空间。
- 同一个物理 store 被多根发现时先规范化去重；Copilot CLI/Desktop 共享事件日志不得双重导入。不同工具碰巧相同的 ID 不合并；Cursor IDE 与 Agent 不仅凭标题/正文相似就猜为同一会话。
- 没有上游 ID 的 Aider 会话使用项目内历史文件身份、真实会话头与稳定出现序号组合；追加内容不改变已有会话身份。文件迁移导致身份变化的边界须在交付文档明确，不承诺无法证明的跨路径自动归并。
- 明确的文本、思考、工具调用与结果分别映射到已有 `ContentBlock`；未知消息块保留受控 `Unknown`，不伪造角色、时间戳、工具结果或 token 用量。
- JSONL 半写尾行与真正损坏行分别诊断；扫描中源文件持续变化可重试或标记不完整。部分解析不应静默覆盖之前完整导入的会话。
- 每种新解析器独立 `parser_version`。文件追加、截断、标题/元数据变化、SQLite WAL 更新与解析器升级均需触发正确刷新；不只看主数据库文件 mtime。
- 上游会话缺失只在对应来源成功完成可比范围扫描后判断。用户关闭来源/移除配置根不等于授权删除已导入知识或思源文档。

### 5.4 下游保持统一

全部新 Provider 只输出标准化会话；不得直接创建 KnowledgeItem 或写 SiYuan。继续使用当前持久任务、提炼、分块、FTS/Embedding、搜索结果与 SiYuan 发布契约。新来源显示名需要同步检查原始会话渲染与思源来源标题，但不批量重命名已有目录或改写用户内容。

## 6. 验证与交付条件

为 11 种来源分别提供匿名 fixtures 和 parser tests；共享解析器不能代替对 Roo、Kilo 等实际差异的独立测试。复杂形态（Cursor 全局/旧 workspace、Copilot 三入口、Antigravity 两类、Kimi 新旧）分别记录支持矩阵。

共同验收必须包括：

- 有效空库、未安装、路径错误、权限错误、schema 不支持、单条损坏/未知事件、中文与 Windows 路径。
- 用户/助手多轮、工具调用与结果关联、父子消息/已确认的分支语义、长文本、附件仅保留描述不越界取文件。
- 同源同会话重复同步不新增；同 ID 跨来源不碰撞；追加消息、仅改标题、WAL 内变更、parser 升级刷新正确。
- 故障扫描不把已有会话误标丢失；禁用来源不删除已导入数据。
- 上游只读校验、源码/测试 fixture 凭据扫描，以及日志不包含正文或密钥。
- 每种来源至少一条 fixture 通过现有同步入库与工作记录/来源筛选测试；搜索通过已知唯一短语验证；提炼/向量服务使用可控测试替身验证路由，不消耗真实账户。
- 现有五种来源的 parser/golden tests 和 PR #45 搜索回归持续通过。

合并候选必须通过仓库要求的 Core/CLI/工作区检查、前端测试与构建、Windows 桌面编译和仓库卫生检查。CI 编译/合成数据通过不代替真实安装环境验收；结果报告分别列出自动验证和真机验证状态。

不为本次任务清空数据库、不原地修改既有 migrations、不换默认模型、不重写搜索引擎。只有实施证明必须新增 schema 时才追加正式 migration 并验证旧库升级。

## 7. 分段交付组织

所有 11 个来源都在范围内，不以分批为由取消较复杂来源。

1. 共同接入基础：来源定义、兼容配置、健康/诊断与读取边界；建立 provider contract tests。
2. 文件型与家族：Qwen、Continue、Cursor Agent、Cline/Roo/Kilo、Aider、Kimi 的格式模块及各自 fixtures。
3. 多存储与复杂形态：Cursor、Copilot、Antigravity；补全 Kimi 的事件重放与新旧差异。
4. 全链路验收：Desktop/CLI/同步/渲染/搜索接线、旧数据兼容、完整 CI 和实际支持矩阵。

提交保持可独立 review，报告具体已完成来源和阻塞格式，不用“整体已接入”掩盖未实现分支。开发在独立分支进行；本设计提交不合入 `main`，功能分支通过验证并经用户确认后再合并，不自动发布安装包。

## 8. 待审阅决策

请审阅：11 个独立来源身份与共享内部适配器；Copilot CLI/Desktop/VS Code 均包含；Antigravity IDE/CLI 只读取已有可验证本地消息数据、不新增抓取/解密能力；Kimi 新旧两套布局；Aider 限定项目根；统一 UI 来源定义；保留旧配置与历史数据。

设计获批后进入具体实施计划与编码。当前尚未写入新 Provider 产品代码、运行其测试或宣称这些来源已经可用。
