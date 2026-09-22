# AI Knowledge Sync (AIKS)

AIKS 是一个本地优先的 AI 工作知识处理系统：从 AI 工具的本地 Session 读取工作记录，转换为 Canonical Model，经持久化 Pipeline 清洗和提炼形成 KnowledgeItem，并提供可选向量/混合搜索、桌面阅读以及 SiYuan 协同。

## 当前分支：独立本地 Service（S1）

`feature/aiks-service-extraction` 新增独立 Rust `aiks-service` 和本地桌面 HTTP 链路。Provider 在客户端采集，完整快照进入独立 outbox；服务持久接收后独立处理，不要求原会话文件继续存在。服务模式不再启动桌面内的旧业务引擎。

Windows 在仓库根目录一键启动：

```powershell
.\scripts\dev.ps1
```

该脚本先检查指定版本思源、前端依赖并构建 Service，再启动 Vite/Tauri；原生桌面控制器管理本机 Service 和思源。旧模式保留：

```powershell
.\scripts\dev.ps1 -Legacy
```

**新模式使用 `<AIKS 数据根>/service-local/` 独立空间，不会自动搬走或清空旧个人库。** 模型配置为该目录的 `config/models.toml`，首次 AI/Embedding 均关闭，不继承旧部署地址。启用前填写实际模型 endpoint/model，修改后重启。完全离线还需要预先准备运行资源和本机模型；配置远程模型会发送相应输入到远程 endpoint。

配置字段 `backend.mode` 默认仍为 `legacy`；开发脚本显式选择 `service_local`。直接 `npm run tauri dev` 遵循配置而不是自动启动新模式。服务启动失败不会悄悄回退旧引擎；mock 仅允许 `DEV + VITE_AIKS_MOCK=true` 显式启用。

新工作台支持选源采集/排除、分批继续、上传与回执、任务状态、会话/知识阅读和搜索。关闭到托盘保留进程，正常退出停止自有子进程。开发命令不等于新安装包；项目/部门权限、远程多用户、受控思源写入与知识库问答属于后续阶段。详见 [S1 使用、验证和回滚说明](docs/implementation/aiks-service-s1.md)。

## 多工具本地会话接入

原有 Claude Code、Codex、Gemini CLI、OpenCode、WorkBuddy，加上 Antigravity、Cursor、Cursor Agent、Cline、Roo Code、Kilo Code、GitHub Copilot、Kimi Code、Qwen Code、Continue、Aider，共用统一会话和后续业务逻辑。

**支持范围不是每种工具的所有历史版本。** Antigravity 只有 IDE usage/token 缓存时不会伪造对话；Cursor Agent 支持已验证 JSONL；Kimi 包含主 Agent wire 与旧 context；Copilot 包含 CLI/Desktop 本地事件与 VS Code 会话快照/补丁；Aider 必须指定项目根。具体文件布局、版本差异及测试入口见 [来源支持矩阵](docs/reference-analysis/multi-provider-support.md)。

目录和开关变更后重启，稳定 key 不随品牌显示名变化。关闭来源或删除配置根不会删除已导入记录。真实安装环境验收与匿名样例自动化测试分开记录。

## 能力与运行边界

- Canonical Session、内容 Hash、增量同步与来源跟踪。
- SQLite 正式 migration、持久 `pipeline_job`、lease、重试与崩溃恢复。
- OpenAI-compatible 知识提炼、KnowledgeItem/Chunk、可选 Embedding。
- FTS 与有界向量召回/重排，向量服务失败保留文本结果并明确 degraded。
- React/Vite/Tauri 桌面、独立 Service 与 Rust CLI。
- 原有 SiYuan Session/Knowledge 映射、发布冲突与 baseline 保护。
- 客户端 Secret Sanitizer、归档、只读 Provider fixtures 和回归测试。

新 Service 的模型配置强制 opt-in。Legacy 保留兼容行为，使用旧配置或默认值之前应核对其 AI/Embedding 开关和 endpoint；不要把旧部署默认值视为新 Service 的网络策略。

Service 通过内部固定适配器读取已发布的思源规范正文，SQLite 承担业务状态、草稿和可重建的检索投影；思源不可用不会冒充返回 SQLite 的发布正文。S1 不向普通用户提供思源通用代理、SQL、任意文件/URL或进程 API。当前只支持受认证的 numeric-loopback personal Service，不能直接作为团队服务对外开放。

## 代码目录

```text
crates/aiks-core/                 统一领域逻辑、Provider、Pipeline、索引、内容适配
  migrations/                    唯一业务 migration 链
apps/aiks-cli/                   原有 CLI，先获取业务数据库所有权
apps/aiks-service/               独立 HTTP/后台服务，不依赖 Tauri
apps/aiks-desktop/               React/Vite 与 Tauri
  src-tauri/src/service_client/  客户端通信、独立 outbox、采集、子进程管理
  src-tauri/src/lifecycle/       保留的 legacy 启动流程
scripts/dev.ps1                  Windows 本地 Service 开发入口，支持 -Legacy
docs/implementation/             当前实施和验收记录
docs/reference-analysis/         上游格式与版本边界
```

业务仍复用 `crates/aiks-core`，根目录没有第二套重复的 `src/`/migration。客户端队列有独立 schema，不得用业务数据库作为 outbox。

## 开发环境与测试

需要 Rust stable/Cargo、Node.js 22+、npm，以及目标平台 Tauri 2 系统依赖。首次准备运行资源和依赖需要网络，模型文件另行准备。测试默认使用临时文件和回环 fixture，不要求用户部署真实模型或思源。

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

纯 Linux CI 需要 WebKit/AppIndicator 依赖；CI 的思源占位目录只能支持编译，不是运行资源或安装包。Windows 测试会执行真实 Service 进程与 HTTP 契约。网络隔离验证仅在独立 Linux namespace 中禁用外部路由，不修改主机防火墙。

旧 CLI 使用：

```bash
cargo run -p aiks-cli -- --help
cargo run -p aiks-cli -- --config ./config.toml doctor
```

命令包含 doctor、scan、sync、status、daemon、resync、rebuild-state、reset-data、sync-knowledge。**reset-data 是破坏性操作**，不要与普通重建或新模式试用混用。不同程序同时写同一业务库时应明确锁冲突，不并行写入。

## 配置与数据安全

根目录 `config.example.toml` 描述旧版完整配置；来源配置继续使用 `[providers.*]`，同步使用 `[sync]`，脱敏使用 `[security]`。新 Service 的模型与生成的运行配置分开保存，详见 S1 说明。不要把真实 Token/API Key 或内部网络地址提交到仓库。

对发布内容的用户修改继续遵守冲突/baseline 保护。会话追加或重新提炼时，旧模型结果不得覆盖新版本；保留的用户知识不会被另一条知识的提炼结果标成当前版本。原始文件损坏、部分发现、禁用来源不等于授权删除已导入数据。

新空间不会自动共享本地历史。源路径只作为受控来源信息，服务不能根据员工电脑路径或任意正文 URL 抓取文件。排除和规则脱敏在进入上传队列之前执行，但不能宣传成绝对不会泄露敏感内容。

## 阶段与质量说明

S1 是个人本地开发工作流，不代表团队 ACL、文档协作、RAG、公开部署和多平台新安装包均已完成。真机显示、真实模型质量、版本兼容及大库性能仍单独验收。

最终验证以对应提交的 Actions 为准，而不是历史绿色记录；状态索引见 [实施进度](docs/implementation/aiks-service-s1-progress.md)。本分支不自动合入 main 或发布安装包。既有 npm 依赖审计告警需要单独跟踪，功能测试通过不等于供应链安全审计通过。不要向不可信网络开放开发服务器。

修改前阅读 [AGENTS.md](AGENTS.md)。历史设计用于理解演进，现行代码、测试、批准的范围与明确实施记录优先；不要重复实现已经存在的 Provider/处理/搜索。
