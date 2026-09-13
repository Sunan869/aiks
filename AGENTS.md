# AGENTS.md

本文件是 AI Coding Agent 实现 AI Knowledge Sync 时必须遵守的最高优先级项目约束。

## 1. 项目目标

实现一个本地优先的 AI Session 自动知识同步工具：

```text
Claude Code / Codex / Gemini CLI / OpenCode
                  ↓
          Session Provider Layer
                  ↓
          Canonical Session Model
                  ↓
             Sync Engine
                  ↓
           Markdown Renderer
                  ↓
              SiYuan API
```

V1 的目标是“可靠采集和同步”，不是重新开发知识库。

## 2. 禁止事项

除非项目负责人明确修改设计，否则禁止：

- 自研 Vector Database。
- 引入 Qdrant、Milvus、Chroma、pgvector 作为核心知识库依赖。
- 自研 Embedding Pipeline。
- 自研 RAG Engine。
- 自研全文搜索引擎。
- 开发独立笔记编辑器。
- 开发知识库 Web UI。
- Provider 直接调用 SiYuan。
- 直接修改 Claude/Codex/Gemini/OpenCode 原始 Session 数据。
- 直接修改 OpenCode SQLite。
- 直接操作 SiYuan 内部 SQLite 或 `.sy` 文件。
- 在已有成熟 Parser 的情况下无理由从零实现。

## 3. 必须遵循的模块边界

```text
Provider
   ↓
NormalizedSession
   ↓
Sync Engine
   ↓
Renderer
   ↓
KnowledgeSink
```

Provider 只负责“发现和解析”。

Provider 不知道 SiYuan。

SiYuanSink 不知道 Claude/Codex/Gemini/OpenCode 原始格式。

## 4. 开发前必须分析的上游仓库

正式写 Provider 之前，必须先 Clone 并分析：

- https://github.com/seastart/aicoder-session-viewer
- https://github.com/farion1231/cc-switch
- https://github.com/ccusage/ccusage
- https://github.com/mnemos-dev/mnemos
- https://github.com/openai/codex
- https://github.com/google-gemini/gemini-cli
- https://github.com/anomalyco/opencode
- https://github.com/anthropics/claude-code
- https://github.com/siyuan-note/siyuan

Clone 后必须填写 `docs/reference-analysis/` 对应分析文件，并记录：

- Repository URL
- Commit SHA
- Checked Date
- License
- Relevant Files
- Session Path
- Data Model
- Reusable Code
- Non-Reusable Code
- Compatibility Risks

禁止只写“参考 main 分支”。必须固定 Commit SHA。

## 5. Provider 实现优先级

### ClaudeProvider

PRIMARY：AICoder Session Viewer

REFERENCE：Mnemos、CC Switch、ccusage

SOURCE OF TRUTH：Anthropic Claude Code

### CodexProvider

PRIMARY：AICoder Session Viewer

REFERENCE：CC Switch、ccusage

SOURCE OF TRUTH：openai/codex

### GeminiProvider

PRIMARY：AICoder Session Viewer

REFERENCE：CC Switch、ccusage

SOURCE OF TRUTH：google-gemini/gemini-cli

### OpenCodeProvider

PRIMARY：AICoder Session Viewer

REFERENCE：CC Switch、ccusage

SOURCE OF TRUTH：anomalyco/opencode

## 6. 上游格式无法解析时的固定排查顺序

不得直接猜 Schema。

必须依次：

1. 检查对应官方仓库当前源码。
2. 检查 AICoder Session Viewer。
3. 检查 CC Switch。
4. 检查 ccusage。
5. 检查已有 Fixtures。
6. 添加新的匿名 Fixture。
7. 更新 Parser。
8. 提升 `parser_version`。
9. 跑完整 Parser Golden Tests。

## 7. Source 必须 Read Only

Claude/Codex/Gemini 文件只能读取。

OpenCode SQLite 必须使用只读模式，例如：

```text
mode=ro
```

需要考虑 OpenCode WAL，但不得修改 WAL，不得执行 migration。

## 8. Canonical Model

所有 Provider 必须输出项目自己的 Canonical Model。

不得把第三方项目的数据结构传播到 Sync/Renderer/Sink。

至少包含：

- NormalizedSession
- NormalizedMessage
- MessageRole
- ContentBlock
- ToolCall
- ToolResult
- Attachment/FileReference
- Usage（可选）
- Metadata

未知字段应尽可能保留在 `metadata` 或 `Unknown`，不要默默丢弃。

## 9. 增量同步

必须支持：

- 首次导入
- 后续增量同步
- Session 继续追加消息
- 文件重写/截断检测
- OpenCode Session 更新时间检测
- content hash
- parser version 变化后的重新解析
- 重复运行不产生重复文档

Watcher 不能作为唯一机制。

必须同时存在：

```text
File Watcher + Periodic Scanner
```

## 10. SiYuan

SiYuan 是 V1 唯一正式 Knowledge Sink。

只通过 HTTP API 集成。

必须支持：

- 创建文档
- 更新文档
- 设置自定义属性
- 上传附件
- 连接检测
- Token 鉴权

AIKS 管理的 Session 文档必须包含可恢复属性：

- `custom-aiks-managed=true`
- `custom-aiks-source`
- `custom-aiks-session-id`
- `custom-aiks-content-hash`
- `custom-aiks-parser-version`
- `custom-aiks-synced-at`

## 11. 冲突与删除

如果用户手动修改 AIKS 托管的 SiYuan Session 文档，默认不得静默覆盖。

应进入 `CONFLICT` 状态。

原始 Session 被删除时，默认保留知识库文档，并标记 source missing。

## 12. 安全

必须实现 Secret Sanitizer。

默认脱敏：

- API Key
- Bearer Token
- Authorization Header
- AccessKey / SecretKey
- password
- token
- secret
- `.env` 中常见凭据

任何日志不得输出完整 Secret。

## 13. 测试要求

每个 Provider 必须有匿名 Fixture。

每个 Provider 必须有 Golden Test。

必须测试：

- 首次解析
- 未变化
- 新消息追加
- 文件重写
- 损坏单 Session
- Unknown Event
- OpenCode WAL 场景
- Markdown Renderer
- Secret Sanitizer
- SiYuan Create/Update/Attrs/Assets
- Sync Conflict
- Missing Source

单 Session 失败不得使整个 Provider 失败。

单 Provider 失败不得使整个 Sync Run 失败。

## 14. License

复制或修改第三方代码时：

- 必须保留原 License 要求。
- 必须更新 `THIRD_PARTY_NOTICES.md`。
- 必须写清 Derived From 的仓库、Commit SHA 和文件。

## 15. V1 范围控制

V1 只完成：

- Claude Code
- Codex
- Gemini CLI
- OpenCode
- Canonical Model
- Sync State SQLite
- Incremental Sync
- Markdown Renderer
- SiYuan Sink
- Archive
- CLI
- Daemon
- Tests

Knowledge Extractor 放到 V1.5。

GUI、MCP Server、更多 Provider 放到后续版本。

## 16. 完成标准

不得以“核心逻辑基本完成”作为完成。

必须达到 `docs/implementation/acceptance.md` 的全部 V1 验收标准。
