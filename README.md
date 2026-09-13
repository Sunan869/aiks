# AI Knowledge Sync (AIKS)

AI Knowledge Sync 是一个本地优先的 AI Session 自动知识沉淀工具。

首期目标：自动读取 Claude Code、OpenAI Codex CLI、Gemini CLI、OpenCode 的本地 Session 数据，统一标准化后同步到 SiYuan 思源笔记，由 SiYuan 负责文档管理、手工录入、全文搜索、Embedding、语义搜索、Rerank、标签、双链和 AI 问答。

## 核心原则

- 不重新开发个人知识库。
- 不实现独立 Vector DB、Embedding Pipeline、RAG Engine、搜索 UI 或笔记编辑器。
- AI Session 原始数据必须只读。
- Provider、Canonical Model、Sync Engine、Renderer、Sink 必须解耦。
- 首期唯一正式 Knowledge Sink 为 SiYuan。
- Session Parser 优先复用/改造成熟开源项目，不允许无理由从零重写。

## V1 数据源

- Claude Code
- OpenAI Codex CLI
- Gemini CLI
- OpenCode

## V1 数据流

```text
Claude / Codex / Gemini / OpenCode
                ↓
          Session Providers
                ↓
        Canonical Session Model
                ↓
             Sync Engine
                ↓
         Markdown Renderer
                ↓
            SiYuan API
                ↓
 Personal Knowledge / Search / AI
```

## 主要上游项目

| 项目 | 地址 | 用途 |
|---|---|---|
| AICoder Session Viewer | https://github.com/seastart/aicoder-session-viewer | 四种 Session Provider/Parser 首要代码来源 |
| CC Switch | https://github.com/farion1231/cc-switch | Session 扫描、增量状态、OpenCode SQLite/WAL、路径发现参考 |
| ccusage | https://github.com/ccusage/ccusage | 多 Agent 数据源、未来 Provider 扩展、格式兼容参考 |
| Mnemos | https://github.com/mnemos-dev/mnemos | Session → Knowledge/Memory 提炼设计参考 |
| SiYuan | https://github.com/siyuan-note/siyuan | 正式个人知识库底座 |
| OpenAI Codex | https://github.com/openai/codex | Codex Session 格式事实来源 |
| Gemini CLI | https://github.com/google-gemini/gemini-cli | Gemini Session 格式事实来源 |
| OpenCode | https://github.com/anomalyco/opencode | OpenCode SQLite Schema 事实来源 |
| Claude Code | https://github.com/anthropics/claude-code | Claude Code 行为和格式变化事实来源 |

## 建议开发顺序

1. 阅读 `AGENTS.md`。
2. 阅读 `docs/design/01-system-design.md`。
3. 阅读 `docs/design/02-open-source-reuse.md`。
4. Clone `references/README.md` 中列出的上游仓库，并固定 Commit SHA。
5. 完成 `docs/reference-analysis/` 中的上游分析文档。
6. 按 `TODO.md` 从 Phase 0 开始实施。
7. 每完成一个 Provider，必须增加匿名 Fixture 和 Parser Test。

## 本目录当前状态

当前仅包含项目规范、设计、任务拆解、配置模板和数据库 DDL，不包含正式实现代码。

实际开发时建议建立：

```text
src/
├── cli/
├── config/
├── model/
├── providers/
├── sync/
├── renderer/
├── sink/
├── extractor/
├── storage/
└── util/
```

详细内容见 `docs/design/01-system-design.md`。
