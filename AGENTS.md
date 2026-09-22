# AGENTS.md

本文件是 AI Coding Agent 与贡献者的现行工程约束。历史文档用于理解演进；不要用历史进度覆盖当前代码、测试、批准的规格和最新实施记录。

## 1. 产品与代码边界

AIKS 是本地优先的 AI 工作知识处理系统。Provider 读取本地 Session，统一为 Canonical Model，经持久任务清洗、提炼、分块与可选向量进入知识/会话检索，并与 SiYuan 内容服务协同。

本服务化分支 S1 新增独立 `apps/aiks-service` 及桌面 HTTP 链路；原有个人模式保留。目录职责：

- `crates/aiks-core`：唯一业务实现，包含 Provider、模型、状态、Pipeline、知识、索引、搜索和内容适配。
- `apps/aiks-cli`：原 CLI。
- `apps/aiks-desktop`：React/Vite/Tauri；采集、client outbox、平台动作与 owned Service supervisor。
- `apps/aiks-service`：独立 HTTP/Worker 入口，不依赖 Tauri，不枚举员工电脑目录。
- `crates/aiks-core/migrations`：唯一正式业务 SQLite migration 链。

根目录不存在第二套可编译 `src/` 或业务 migrations；不得复制另一套 Pipeline/Knowledge/Search。客户端 outbox 使用独立 SQLite/schema，不得把业务 StateDb 当队列文件。

## 2. Provider 只读边界

Provider 只负责发现/读取并输出 Canonical Session，不得直接写思源或创建 KnowledgeItem，不得把上游专有结构泄漏到业务层。

Claude/Codex/Gemini 源文件只读；OpenCode、WorkBuddy、Cursor、Kilo SQLite 使用只读/WAL-aware 方式，不修改上游 schema、WAL 或数据。WorkBuddy 只允许会话元数据及 `projects/**/*.jsonl`，不得读取或发布 connectors、memory profile、MCP secrets、`.neodata_token`、file-history。

原五种稳定来源身份保留。新增 Antigravity、Cursor、Cursor Agent、Cline、Roo Code、Kilo Code、GitHub Copilot、Kimi Code、Qwen Code、Continue、Aider 共用 catalog 和 Canonical Model。新来源经 ScopedReader 读取允许文件与必要元数据，加载时复查身份、路径边界、读取预算和完整性，拒绝越界 symlink/reparse。不得枚举与会话无关的编辑器凭据。

Antigravity IDE token/usage 不是对话，禁止伪造用户/助手消息。Aider 不默认扫描主目录/整盘。部分扫描、单会话损坏、禁用或取消配置根，不能据此将未扫描的旧记录标为缺失。

格式问题先查匿名 fixtures、当前 parser 与 reference-analysis，再核对官方格式/参考实现；加对应 fixture，兼容语义改变时提升 parser_version。实际安装环境版本兼容与合成 fixture 测试分开记录。复制第三方代码必须遵守 License 并更新 THIRD_PARTY_NOTICES。

## 3. Service 与桌面运行方式

配置 backend.mode 仅 legacy/service_local，默认 legacy；Windows dev.ps1 显式选择 service_local，-Legacy 保留旧流程。未知值报错；Service 启动失败不能自动改回旧模式或创建第二套 Worker。

S1 只允许认证的 numeric-loopback personal Service，不得为了测试/部署方便放开 team 或公网监听。团队 ACL、远程多用户和思源受控写入属于 S2，必须先完成授权边界。

客户端上传完整标准化快照，Service 原子保存快照、版本、回执和任务。后台仅从已保存输入处理，不通过员工本机路径重新读取。source/project path 不是服务下载目标；正文 URL 不授权抓取。

监督器只允许原生代码选定的开发/资源二进制路径；IPC 不接受任意可执行文件/命令/URL/PID。启动令牌通过 stdin 传递，stdout 握手校验协议、实例、nonce、数字回环地址，并进行认证 capabilities 检查。凭据不进 Webview、队列、日志、URL 或 capability DTO。

窗口关闭入托盘保留进程；正常退出有界停止采集、drain 自有 Service 并停止自有思源。只有明确 opt-in 的 managed sidecar 具有父管道 EOF 生命周期；独立实例不因客户端 stdin EOF 停止。永不按陌生 PID 或远程 URL 执行 shutdown/kill。启动和退出串行化，错误/取消不得遗留第二个 Writer。

Service 默认新空间位于 data-root/service-local，普通启动不得迁移、清空、扫描或更改旧业务库/旧思源工作区。运行资源定位必须无修改副作用；legacy bridge 安装不得偷偷用于新模式。新 profile/config/identity 路径拒绝不符合边界的链接和 reparse points。

## 4. 持久状态与版本

SQLite 是本地业务状态与持久任务事实源。pipeline_job 保存 lease、heartbeat、attempt、retry 与失败终态；内存 channel 只作有界唤醒，禁止恢复为无界 payload backlog。任务重启恢复不依赖仅在内存中的输入。

Desktop/CLI/Service 写同一个业务库前统一获取跨进程所有权锁。不得通过共享目录让客户端直接打开服务端数据库；不得用已打开数据库副本绕过 ownership。

接收快照的身份至少区分可信主体/空间、来源注册、上游会话和版本。重复请求返回可核对回执；不能用外部 session ID 猜测属于哪个来源。每个客户端会话最多一个未完成 generation，重传不改变目标实例、空间、submission ID、payload hash 或 expected_revision。切换连接不能重定向旧队列。

所有派生写入（正文投影、chunks、FTS、向量、knowledge、成功状态）必须在同一事务中检查接受版本；晚返回的旧模型/向量不能覆盖新结果。真正 superseded 用明确错误类型，不把数据库错误归为正常替代。查询未知/旧版本派生数据不能拼接为当前状态。保留用户/已发布知识按条目保持源版本，不能继承另一个新提炼条目的版本。

配置为必需的阶段失败不能写成功。Embedding 已启用时分块/模型/索引失败不得 READY；明确关闭可以 SKIPPED。普通错误不能用 unwrap_or_default 伪装成功或零结果。故障隔离至合理最小范围。

## 5. Knowledge、搜索与思源

KnowledgeItem 稳定身份只在安全、唯一匹配时复用，目前以规范化 (category,title) 唯一匹配为准；不要按弱 category 猜 rename。被移除的已同步知识保留可追踪 REMOVED tombstone 与远端映射，不能静默抹掉。更新后使旧 chunk/embedding/FTS 失效并重建。

统一搜索复用 Core，不在 Tauri 再做一套算法。可信空间/主体过滤必须在候选 LIMIT 前执行，正文和引用还需再校验。文本查询参数化且安全处理 FTS grammar；FTS 不可用回退文本并报告 degraded；向量不可用保留文本结果并明确降级。普通查询不能无界加载整库向量；保持有界候选或实现经过验证的 ANN。

已发布正文、文档结构和编辑内容通过思源公开 HTTP API 读取；不得直接修改思源 SQLite 或 .sy 文件。SQLite 的已发布内容投影不是可伪装返回的规范正文，思源不可用报告 content_unavailable。未发布草稿需要明确标识。

保留用户编辑的 conflict/baseline 语义，不默认静默覆盖；并发知识发布不得为同一条目创建重复远端文档。慢思源网络 I/O 不占 raw/session sync 的粗粒度全局锁。异步网络等待期间不持有 SQLite 锁。

服务端思源不直接开放普通用户。严禁增加通用 proxy/SQL/file/URL 透传来绕过授权；内部内容适配固定 origin 和端点，拒绝重定向转发令牌。后续受控文档/块/附件读写需验证资源空间，不可只在页面隐藏入口。

## 6. 配置、日志与公开仓库

新 Service/公开新配置 AI 与 Embedding 必须 opt-in，部分 [ai] 表不能继承旧部署地址/自动启用。不要为使测试通过而改变用户部署配置。保留的 legacy 默认值不代表新的安全策略；不要重新复制内部地址到示例/新模块。

禁止提交真实 API Key、Bearer Token、AccessKey、密码和 `.workbuddy/` 本地记忆。测试用临时源、合成值和回环 fixture，必须显式配置模型；不消费真实账户。日志不得输出完整秘密/正文；服务错误返回固定安全码，不回显上游原始异常中的凭据。

规则脱敏不是绝对保证。数据本地保存不意味着发往可配置远程模型的内容不出本机；界面和文档不得混淆。开发 mock 必须显式 opt-in，生产/无 Tauri 环境不自动回退模拟结果。

## 7. Migration 与修改原则

正式业务 schema 变更只追加 crates/aiks-core/migrations，不原地改写已使用 migration；验证旧库而非只空库。默认值、回填、索引、事务和回滚边界明确，无法证明的历史源版本保持未知。

显式内部 adoption 与用户默认启动分开；不得宣称内部迁移自动提供备份。破坏性 reset 与普通 rebuild 区分；真实用户资料迁移/清空需要单独授权及备份。

优先修改已有 canonical 实现，不无关重构或全仓格式化。行为修复先加可复现测试，再实现最小改动。保留 CLI/Desktop 调用兼容；必须改变接口时同提交更新所有调用方与测试。

## 8. 验证与完成声明

重大 Core 变更运行 Core 测试与 CLI 检查；涉及工作区/Tauri 跑完整工作区，前端跑测试与构建：

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

Linux Tauri 需要 WebKit/AppIndicator 等系统依赖。CI 的思源占位目录仅编译验证，不是实际 runtime/安装包。Windows 执行真实 storage/worker/HTTP/lifecycle 测试，不只 check。S1 离线测试需要隔离外部网络并启用回环模型，不能仅关闭 AI 后声称完整离线 AI 验证。

CI 不自动提交生成的补丁或移动开发分支；最终验证 workflow 保持只读。每个完成声明绑定已执行的具体提交和结果；不能用之前绿色 SHA 或准备工作流成功代替当前正式测试。功能测试不等于供应链安全审计，既有 npm audit 告警必须记录/跟踪。

## 9. 范围与进度

S1 当前实现本地 Service 业务链路、原生监督器和开发入口。S2 团队身份/空间/共享与思源受控写入、S3 完整部署/安装包、S4 引用式知识库问答仍为后续阶段。

当前实施看 docs/implementation/aiks-service-s1-progress.md，使用与回滚看 aiks-service-s1.md。此前 task ledger 和未勾选的原始计划保留历史，不应触发重复开发已经完成的代码。不得把内部 publisher 测试当成对外写 API，也不得把首条 ServiceStatusPage 当成全部旧工作台功能已服务化。
