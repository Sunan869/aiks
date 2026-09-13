# 开源参考项目与代码复用方案

## 1. 原则

AIKS 必须优先复用成熟开源项目已经解决的问题。

区分四类：

1. 直接复制/改造代码。
2. 移植 Parser/算法。
3. 只参考架构。
4. 只作为官方事实来源。

“功能合并，架构不硬合并”。

## 2. AICoder Session Viewer

Repository:

https://github.com/seastart/aicoder-session-viewer

定位：V1 Session Provider/Parser 的首要代码来源。

重点研究：

- Claude Provider
- Codex Provider
- Gemini Provider
- OpenCode Provider
- Session discovery
- Message/content parsing
- SQLite reading
- ToolCall/ToolResult parsing
- Path discovery

使用策略：

```text
AICoder Session Viewer
       ↓ 提取 Parser/Provider 逻辑
AIKS Provider Layer
       ↓ 输出
NormalizedSession
```

不应让 AIKS 运行时依赖 AICoder Session Viewer 应用本身。

## 3. CC Switch

Repository:

https://github.com/farion1231/cc-switch

定位：采集可靠性和增量同步参考。

重点研究：

- 默认路径发现
- Session scanning
- Usage sync state
- OpenCode SQLite/WAL
- Codex/Gemini/Claude session usage
- 本地 SQLite 管理

不复用：

- Provider switching
- Proxy
- API Gateway
- Quota dashboard

## 4. ccusage

Repository:

https://github.com/ccusage/ccusage

定位：多 Coding Agent 数据源和未来 Provider 扩展参考。

重点研究：

- Agent 数据位置
- Parser/data loader
- Usage/session compatibility
- 新 Provider 扩展方式

长期可用于新增：

- Qwen Code
- Kimi
- Copilot CLI
- OpenClaw
- Hermes
- Goose
- Amp
- Droid

## 5. Mnemos

Repository:

https://github.com/mnemos-dev/mnemos

定位：V1.5 Knowledge Extractor 设计参考。

研究重点：

- Session refinement
- Knowledge distillation
- Decisions
- Problems/solutions
- Project memory
- Provenance
- Next-session recall

Mnemos 的 Claude-specific 代码不得直接污染 AIKS Canonical Layer。

## 6. SiYuan

Repository:

https://github.com/siyuan-note/siyuan

定位：正式个人知识库底座。

AIKS 通过 HTTP API 集成，不 Fork，不嵌入源码。

SiYuan 负责：

- 编辑器
- 手工录入
- 文档树
- 标签
- 双链
- 数据库视图
- 全文搜索
- Embedding
- Semantic Search
- Rerank
- AI Q&A

AIKS 不重写这些功能。

## 7. 官方事实来源

### OpenAI Codex

https://github.com/openai/codex

用途：Codex session persistence/schema 最终事实来源。

### Gemini CLI

https://github.com/google-gemini/gemini-cli

用途：Gemini session persistence/schema 最终事实来源。

### OpenCode

https://github.com/anomalyco/opencode

用途：OpenCode SQLite schema/migrations 最终事实来源。

### Claude Code

https://github.com/anthropics/claude-code

用途：Claude Code 行为、session 格式变化、issue/changelog 最终事实来源。

## 8. Provider 来源矩阵

| Provider | Primary | Reference | Source of Truth |
|---|---|---|---|
| Claude | AICoder Session Viewer | Mnemos / CC Switch / ccusage | Claude Code |
| Codex | AICoder Session Viewer | CC Switch / ccusage | openai/codex |
| Gemini | AICoder Session Viewer | CC Switch / ccusage | google-gemini/gemini-cli |
| OpenCode | AICoder Session Viewer | CC Switch / ccusage | anomalyco/opencode |

## 9. 上游分析要求

开发前必须记录：

- Commit SHA
- License
- Relevant Source Files
- Default Session Paths
- Data Model
- Parser Behavior
- Incremental Behavior
- Risks

模板见 `docs/reference-analysis/_template.md`。

## 10. 为什么不能整体 Fork 合并

整体 Fork 多项目会导致：

- 前端/UI 重复
- 依赖冲突
- License 边界复杂
- 升级困难
- 大量无关代码

AIKS 应只吸收需要的 Parser、兼容策略和设计经验。
