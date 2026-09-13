# AI Knowledge Sync 系统详细设计 V1.0

## 1. 项目定位

AIKS 是一个本地优先的 AI Coding Session 自动知识同步系统。

核心目标不是保存聊天记录本身，而是让用户继续正常使用 Claude Code、Codex、Gemini CLI 和 OpenCode，同时自动把其中有价值的对话沉淀进长期个人知识库。

V1 只完成“自动采集 + 标准化 + 增量同步 + SiYuan”。

V1.5 再增加“Session → KnowledgeItem”的 AI 知识提炼。

## 2. 系统边界

AIKS 负责：

- AI Session 自动发现
- Session 读取
- Session Parser
- Canonical Model
- Incremental Sync
- 去重
- Content Hash
- Markdown Renderer
- Secret Sanitizer
- Attachment 处理
- SiYuan API Sink
- State SQLite
- Archive
- CLI/Daemon

SiYuan 负责：

- 手动录入知识
- Markdown 编辑
- 知识分类
- 标签
- 双链
- 数据库视图
- 全文搜索
- Embedding
- 语义搜索
- Rerank
- AI 问答
- 多端知识浏览

## 3. 总体架构

```text
┌────────────────────────────────────────────┐
│              AI Coding Tools               │
│ Claude   Codex   Gemini   OpenCode         │
└──────┬──────┬──────┬─────────┬────────────┘
       │      │      │         │
       ▼      ▼      ▼         ▼
┌────────────────────────────────────────────┐
│               Provider Layer               │
│ ClaudeProvider                             │
│ CodexProvider                              │
│ GeminiProvider                             │
│ OpenCodeProvider                           │
└─────────────────────┬──────────────────────┘
                      ▼
┌────────────────────────────────────────────┐
│            Normalization Layer             │
│ Session / Message / ContentBlock           │
│ ToolCall / ToolResult / Attachment         │
└─────────────────────┬──────────────────────┘
                      ▼
┌────────────────────────────────────────────┐
│                Sync Engine                 │
│ Change Detection / Hash / Retry / Conflict │
└─────────────────────┬──────────────────────┘
                      ▼
┌────────────────────────────────────────────┐
│             Markdown Renderer              │
│       + Secret Sanitizer + Assets          │
└─────────────────────┬──────────────────────┘
                      ▼
┌────────────────────────────────────────────┐
│                SiYuan Sink                 │
└─────────────────────┬──────────────────────┘
                      ▼
┌────────────────────────────────────────────┐
│                  SiYuan                    │
│ Search / Embedding / Rerank / AI / Notes  │
└────────────────────────────────────────────┘
```

## 4. 推荐技术栈

V1 推荐 Rust：

- tokio
- serde / serde_json
- rusqlite
- reqwest
- walkdir
- notify
- chrono
- sha2
- uuid
- thiserror / anyhow
- tracing
- tracing-subscriber
- clap
- toml

原因：AICoder Session Viewer 与 CC Switch 的核心代码同样以 Rust 为主，方便复用 Parser 和数据访问逻辑。

## 5. 目录结构

```text
src/
├── main.rs
├── cli/
├── config/
├── model/
├── providers/
├── sync/
├── renderer/
├── sink/
│   └── siyuan/
├── extractor/
├── storage/
└── util/
```

## 6. Provider 接口

建议：

```rust
#[async_trait]
pub trait SessionProvider: Send + Sync {
    fn source(&self) -> SourceKind;
    fn parser_version(&self) -> &'static str;
    async fn discover_sessions(&self) -> Result<Vec<SessionSummary>>;
    async fn load_session(&self, session: &SessionSummary) -> Result<NormalizedSession>;
    async fn health_check(&self) -> ProviderHealth;
}
```

Provider 不直接操作 SiYuan。

## 7. Canonical Model

### SourceKind

```text
ClaudeCode
Codex
GeminiCli
OpenCode
```

### NormalizedSession

至少包含：

- source
- external_session_id
- title
- project_name
- project_path
- source_path
- started_at
- updated_at
- model
- messages
- usage
- metadata

### NormalizedMessage

至少包含：

- external_id
- parent_id
- role
- created_at
- model
- blocks
- usage
- metadata

### MessageRole

```text
User
Assistant
System
Tool
Unknown
```

### ContentBlock

```text
Text
Thinking
ToolCall
ToolResult
Image
FileReference
Unknown
```

未知事件不要直接丢弃，应尽可能保存原始 metadata。

## 8. Provider 设计

### Claude Code

默认扫描：

```text
~/.claude/projects/
```

重点解析：

- user
- assistant
- system
- tool_use
- tool_result
- uuid
- parentUuid
- sessionId
- cwd
- timestamp

### Codex

默认扫描：

```text
~/.codex/sessions/
~/.codex/archived_sessions/
```

重点解析：

- session_meta
- user/assistant message
- response item
- tool call/result
- turn_context
- token_count

### Gemini CLI

必须实现 FormatDetector。

支持当前 JSONL，并为旧 JSON/未来格式预留 Parser。

Parser 失败必须 fail-soft。

### OpenCode

默认数据库：

```text
~/.local/share/opencode/opencode.db
```

必须只读。

启动时检测 sqlite_master 和当前 Schema。

必须考虑 WAL。

不得假定 session/message/part 永久存在。

## 9. Parser Version

每个 Provider 必须定义：

```text
claude-v1
codex-v1
gemini-jsonl-v1
opencode-sqlite-v1
```

Parser Version 改变后允许强制重新解析历史 Session。

## 10. 增量同步

文件型 Provider 保存：

- source_path
- file_size
- modified_at
- last_offset
- file_hash
- parser_version

JSONL 可采用 offset 增量。

如果文件截断或替换，完整重读。

普通 JSON 采用 mtime + size + SHA256 检测变化后完整重读。

OpenCode 采用 session updated_at/message id 等策略。

## 11. Watcher + Periodic Scanner

必须同时存在：

- OS File Watcher
- 每 5 分钟兜底扫描

默认 debounce 2 秒。

## 12. Canonical Hash

Hash 基于标准化后的稳定字段：

- message role
- content
- tool calls/results
- model
- timestamps

不得包含：

- last_seen_at
- 本地扫描时间
- volatile mtime

## 13. Markdown Renderer

每个 Session 对应一个可搜索文档。

建议格式：

```markdown
# Session Title

> 来源：Codex
> 项目：DataOcean
> Session：...
> 模型：...
> 开始：...
> 更新：...

## 👤 User
...

## 🤖 Assistant
...

### 🔧 Tool Call
...

### 📤 Tool Result
...
```

Thinking 默认不导入。

Tool Result 默认最大 10000 字符，超出截断。

## 14. SiYuan 结构

建议 Notebook：

```text
AI Knowledge
```

目录：

```text
00 Inbox
10 AI Sessions
  ├─ Claude
  ├─ Codex
  ├─ Gemini
  └─ OpenCode
20 Knowledge
90 System
```

V1 重点只要求 `10 AI Sessions`。

文档建议路径：

```text
/10 AI Sessions/{source}/{yyyy}/{MM}/{yyyy-MM-dd} {title} [{short_session_id}]
```

唯一映射依赖 `source + external_session_id`，不能依赖路径。

## 15. SiYuan 属性

必须写：

```text
custom-aiks-managed=true
custom-aiks-source=codex
custom-aiks-session-id=...
custom-aiks-content-hash=...
custom-aiks-parser-version=...
custom-aiks-synced-at=...
```

## 16. 冲突策略

如果用户手工修改托管文档：

- 默认不覆盖。
- 状态设为 CONFLICT。
- CLI 显示冲突。
- 仅 `--overwrite` 可强制覆盖。

## 17. 删除策略

原始 Session 删除时：

- SiYuan 文档保留。
- source_session.is_missing=true。
- 不自动删除知识。

## 18. Archive

建议保存：

```text
archive/{source}/{session-id}.json.gz
```

内容为 NormalizedSession，不是简单复制原始 Session 文件。

## 19. Secret Sanitizer

默认脱敏：

- API Keys
- Bearer Tokens
- Authorization Headers
- AccessKey/SecretKey
- password/token/secret/api_key

支持自定义正则。

## 20. CLI

必须提供：

```text
aiks doctor
aiks scan
aiks sync
aiks sync --source codex
aiks sync --dry-run
aiks status
aiks daemon
aiks resync
aiks rebuild-state
```

## 21. Windows

V1 优先使用普通用户进程 + 开机启动，不强制 Windows Service。

State DB：

```text
%LOCALAPPDATA%\AIKnowledgeSync\aiks.db
```

## 22. Fault Isolation

单 Session 失败不能中止 Provider。

单 Provider 失败不能中止整个同步。

SiYuan 离线时应保留 PENDING 状态，恢复后自动补同步。

## 23. V1.5 Knowledge Extractor

V1.5 才增加：

```text
NormalizedSession
      ↓
KnowledgeExtractor
      ↓
KnowledgeItem[]
      ↓
SiYuan /20 Knowledge
```

固定类型：

```text
solution
decision
howto
command
code
configuration
concept
project_context
prompt
lesson
todo
reference
```

必须保留来源 Session/Message Provenance。

## 24. 长期方向

未来 AIKS 可通过 MCP 提供：

- knowledge_search
- knowledge_get
- project_context
- recent_decisions

形成：

```text
AI Session → Knowledge → Next AI Session → Better Knowledge
```
