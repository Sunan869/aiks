# AI Knowledge Sync (AIKS)

AIKS（AI Knowledge Sync）是一个本地优先的 AI 工作知识处理系统。它从 Claude Code、OpenAI Codex CLI、Gemini CLI、OpenCode、WorkBuddy 的本地 Session 中读取工作记录，统一成 Canonical Model，经持久化 Pipeline 清洗和 AI 提炼后形成结构化 KnowledgeItem，并提供可选 Embedding / Hybrid Search、桌面端浏览以及 SiYuan 同步。

当前仓库已经包含正式实现，不再是只有设计文档的 V1 骨架。

## 多工具本地会话接入

新增原生只读来源：Antigravity、Cursor、Cursor Agent、Cline、Roo Code、Kilo Code、GitHub Copilot、Kimi Code、Qwen Code、Continue、Aider。它们与原有五个来源复用 Canonical Session、同步、持久任务、知识提炼和搜索，不依赖安装另一个会话查看器。

**支持范围不是各工具的所有历史版本。** Antigravity 当前可导入已有 CLI transcript；只有 IDE token/usage 缓存时会明确显示格式不支持，绝不把统计数据拼成用户/助手对话。Cursor Agent 当前支持 JSONL，Kimi 覆盖主 Agent wire 与旧 context 布局，Copilot 覆盖 CLI/Desktop 本地事件和 VS Code 会话快照/补丁。Aider 必须显式配置项目根目录。

数据源页面统一展示正式名称、状态和目录配置；保存目录/开关后需要重启。来源稳定 key 不随品牌名变化，关闭来源或移除配置根不会删除已导入记录。自动化样例与实际安装环境验收分别记录；具体文件布局、测试入口和已知限制见 `docs/reference-analysis/multi-provider-support.md`。

## 当前能力

- 原有五个 Session Provider，加上上述 11 个本地来源适配器；精确支持范围见支持矩阵。
- Canonical Session Model、内容 Hash、增量同步与 source 状态跟踪。
- SQLite 状态库与 migration。
- 持久化 `pipeline_job` 队列、lease、重试、崩溃恢复和 Pipeline stage 可观测性。
- OpenAI-compatible AI Knowledge Extraction。
- KnowledgeItem / KnowledgeChunk / Embedding 持久化。
- FTS + 可选向量 rerank 的混合搜索；向量后端不可用时保留文本结果并报告 degraded 状态。
- React + Vite + Tauri Desktop。
- CLI：doctor、scan、sync、status、daemon、resync、rebuild/reset、knowledge sync。
- SiYuan Session / Knowledge 同步、映射、冲突检测和 baseline。
- Secret Sanitizer、Archive、Provider fixtures 与 Rust 回归测试。

AI 与 Embedding 在公开/新安装配置中默认关闭，配置兼容的服务后再启用。

## V3 数据流

```text
Claude Code / Codex / Gemini CLI / OpenCode / WorkBuddy
                  ↓
          Session Provider Layer
                  ↓
          Canonical Session Model
                  ↓
       Parse / Clean / Durable Pipeline
                  ↓
            AI Extraction
                  ↓
            KnowledgeItem
             ↙       ↘
        FTS/Text      Embedding
             ↘       ↙
           Hybrid Search
            ↙       ↘
       Desktop/CLI    SiYuan
```

SiYuan 是支持的 Knowledge Sink / Session Archive 目标，而不是 AIKS 唯一的数据底座。核心状态、Knowledge 和搜索能力均由 AIKS 自身维护。

## Repository Layout

```text
.
├── crates/
│   └── aiks-core/              # 核心领域逻辑、Provider、Pipeline、SQLite、Knowledge/Search
│       ├── src/
│       └── migrations/         # 唯一正式 migration 链
├── apps/
│   ├── aiks-cli/               # Rust CLI
│   └── aiks-desktop/           # React/Vite + Tauri
│       └── src-tauri/
├── docs/                       # 设计、实施、历史评审和参考分析
├── references/                 # 上游项目研究说明
├── config.example.toml         # 示例配置
├── AGENTS.md                   # 当前工程约束
└── TODO.md                     # 当前剩余路线图
```

根目录没有另一套可编译 `src/`；核心实现以 workspace 中的 `crates/aiks-core` 为准。

## Requirements

基础开发环境：

- Rust stable / Cargo。
- Node.js 22+ 与 npm（Desktop frontend）。
- 构建 Tauri Desktop 时，需要对应操作系统的 Tauri 2 系统依赖。
- SiYuan、AI endpoint、Embedding endpoint 都是按需配置的外部服务，不是运行 Core tests 的前置条件。

## 配置

从示例开始：

```bash
cp config.example.toml config.toml
```

Windows PowerShell 可使用：

```powershell
Copy-Item config.example.toml config.toml
```

重要配置段：

- `[providers.*]`：各 Session Provider 是否启用及自定义路径。
- `[sync]`：扫描、Watcher、debounce、并发等。
- `[ai]`：OpenAI-compatible Knowledge Extraction；默认 `enabled = false`。
- `[embedding]`：可选 Embedding；默认 `enabled = false`。
- `[siyuan]`：SiYuan HTTP API、Notebook 和根路径。
- `[security]`：Secret redaction。

不要把真实 Token、API Key 或内部网络地址提交到仓库。SiYuan Token 建议通过环境变量或本地配置提供。

## Build & Test

### Rust workspace

```bash
cargo build --workspace
cargo test --workspace
```

只验证 Core：

```bash
cargo test -p aiks-core
```

### CLI

查看帮助：

```bash
cargo run -p aiks-cli -- --help
```

使用指定配置做环境检查：

```bash
cargo run -p aiks-cli -- --config ./config.toml doctor
```

常用命令包括：

```text
doctor
scan [--source ...]
sync [--source ...] [--dry-run] [--overwrite]
status
daemon
resync
rebuild-state
reset-data
sync-knowledge
```

其中 `reset-data` 是破坏性操作；不要把它与只重建同步索引的 `rebuild-state` 混用。

### Desktop frontend

```bash
cd apps/aiks-desktop
npm ci
npm run build
```

前端开发服务器：

```bash
npm run dev
```

运行 Tauri Desktop：

```bash
npm run tauri dev
```

## 搜索行为

文本搜索是基础能力；Embedding/Vector 是可选增强。

- 用户 FTS 输入会按字面量安全处理，不直接暴露给 FTS5 grammar。
- FTS 不可用或执行失败时，会回退到参数化 LIKE 并标记 degraded。
- Vector 服务不可用时，文本结果仍会返回，并报告 `VectorUnavailable`。
- 当前向量 rerank 使用有界候选集（上限 512），不会在普通查询里加载并排序全库所有 embedding。

## Pipeline 可靠性

Pipeline 工作保存在 SQLite `pipeline_job` 中，而不是依赖无界内存队列。Worker 使用 lease / heartbeat / persisted attempts / retry budget 支持重启恢复，并通过 `pipeline_run` / stage 状态提供可观测性。

如果 Embedding 已明确启用并配置，它属于必需阶段：分块、Embedding 或索引失败不会被错误标记为 `READY`。Embedding 未启用时对应阶段可以显式 `SKIPPED`。

## Knowledge 与 SiYuan

AI 提炼结果以 KnowledgeItem 为核心。本地 Knowledge identity 会尽量在安全匹配时保持稳定，以避免重复的远端 SiYuan 文档；无法安全判断为同一知识时宁可创建新身份，也不做模糊映射迁移。

SiYuan 同步保留 conflict/baseline 保护。已同步知识被后续提炼移除时，本地保留 `REMOVED` 映射 tombstone 以维持远端可追踪性。

## Provider 与上游兼容

Provider 的源数据必须只读。OpenCode SQLite 与 WorkBuddy SQLite 均按只读/WAL-aware 边界访问；WorkBuddy 仅读取会话元数据与 `projects/**/*.jsonl` transcript，不读取 connectors、memory、MCP secrets 或 file-history 内容。

上游格式变化时，先查看现有 fixtures/tests、当前 Provider 和 `docs/reference-analysis/`，再核对对应官方项目。不要凭猜测修改 parser schema。

主要参考项目及历史分析见 `references/README.md` 和 `docs/reference-analysis/`。

## 项目成熟度

AIKS 当前适合作为持续演进中的内部 Alpha/Beta 使用：核心链路和较完整的 Rust 回归测试已经存在，P0/P1 可靠性问题已进行集中修复；仓库 CI、前端测试基线、发布/打包卫生和历史敏感信息审计仍在工程化路线图中。

当前未完成项请以 GitHub Issues 与 `TODO.md` 为准，历史 `docs/design/` / `docs/implementation/` 文档不应被当作比当前代码更高优先级的现行约束。

## Contributing

修改前请先阅读 `AGENTS.md`。行为修复优先补回归测试，避免把无关重构或全仓格式化混入小型 PR。重大改动至少验证 Core；涉及 Desktop 时同时验证前端 build / Tauri Rust 部分。
