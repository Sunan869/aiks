# AIKS Implementation TODO

## Phase 0 - 上游研究与冻结基线

- [ ] Clone `references/README.md` 中所有上游项目。
- [ ] 固定每个仓库的 Commit SHA。
- [ ] 填写 `docs/reference-analysis/aicoder-session-viewer.md`。
- [ ] 填写 `docs/reference-analysis/cc-switch.md`。
- [ ] 填写 `docs/reference-analysis/ccusage.md`。
- [ ] 填写 `docs/reference-analysis/mnemos.md`。
- [ ] 填写 `docs/reference-analysis/codex.md`。
- [ ] 填写 `docs/reference-analysis/gemini-cli.md`。
- [ ] 填写 `docs/reference-analysis/opencode.md`。
- [ ] 填写 `docs/reference-analysis/claude-code.md`。
- [ ] 填写 `docs/reference-analysis/siyuan.md`。
- [ ] 更新 `THIRD_PARTY_NOTICES.md`。

## Phase 1 - 项目骨架

- [ ] 初始化 Rust Cargo Workspace/Package。
- [ ] 引入 tokio。
- [ ] 引入 serde/serde_json。
- [ ] 引入 rusqlite。
- [ ] 引入 reqwest。
- [ ] 引入 tracing。
- [ ] 引入 clap。
- [ ] 引入 notify。
- [ ] 引入 sha2。
- [ ] 引入 uuid。
- [ ] 引入 chrono。
- [ ] 引入 thiserror/anyhow。
- [ ] 建立 `src/cli`。
- [ ] 建立 `src/config`。
- [ ] 建立 `src/model`。
- [ ] 建立 `src/providers`。
- [ ] 建立 `src/sync`。
- [ ] 建立 `src/renderer`。
- [ ] 建立 `src/sink`。
- [ ] 建立 `src/storage`。
- [ ] 建立 `src/util`。

## Phase 2 - Canonical Model

- [ ] SourceKind。
- [ ] NormalizedSession。
- [ ] NormalizedMessage。
- [ ] MessageRole。
- [ ] ContentBlock::Text。
- [ ] ContentBlock::Thinking。
- [ ] ContentBlock::ToolCall。
- [ ] ContentBlock::ToolResult。
- [ ] ContentBlock::Image。
- [ ] ContentBlock::FileReference。
- [ ] ContentBlock::Unknown。
- [ ] Usage Model。
- [ ] Metadata Model。
- [ ] Canonical serialization。
- [ ] Canonical SHA256 hash。

## Phase 3 - Provider Interface

- [ ] 定义 SessionProvider trait。
- [ ] ProviderRegistry。
- [ ] ProviderHealth。
- [ ] SessionSummary。
- [ ] ParserVersion。
- [ ] 错误隔离。
- [ ] Unknown Event 本地记录。

## Phase 4 - ClaudeProvider

- [ ] 移植/改造 AICoder Session Viewer Claude Parser。
- [ ] 对照 Mnemos。
- [ ] 对照 CC Switch。
- [ ] 对照 ccusage。
- [ ] 对照 Claude Code 官方仓库。
- [ ] 支持默认路径发现。
- [ ] 支持 user/assistant/system。
- [ ] 支持 tool_use/tool_result。
- [ ] 支持 parentUuid/uuid/sessionId/cwd/timestamp。
- [ ] 匿名 fixtures。
- [ ] parser golden tests。

## Phase 5 - CodexProvider

- [ ] 移植/改造 AICoder Session Viewer Codex Parser。
- [ ] 对照 CC Switch。
- [ ] 对照 ccusage。
- [ ] 对照 openai/codex。
- [ ] sessions 扫描。
- [ ] archived_sessions 扫描。
- [ ] session_meta。
- [ ] user/assistant message。
- [ ] tool calls/results。
- [ ] turn_context。
- [ ] token_count 可选解析。
- [ ] 匿名 fixtures。
- [ ] parser golden tests。

## Phase 6 - GeminiProvider

- [ ] 建立 FormatDetector。
- [ ] 移植/改造 AICoder Session Viewer Gemini Parser。
- [ ] 对照 CC Switch。
- [ ] 对照 ccusage。
- [ ] 对照 google-gemini/gemini-cli。
- [ ] 支持当前 JSONL。
- [ ] 如需要兼容 legacy JSON。
- [ ] Unknown Event fail-soft。
- [ ] 匿名 fixtures。
- [ ] parser golden tests。

## Phase 7 - OpenCodeProvider

- [ ] 移植/改造 AICoder Session Viewer OpenCode Parser。
- [ ] SQLite Read Only。
- [ ] SchemaDetector。
- [ ] WAL aware。
- [ ] 对照 CC Switch。
- [ ] 对照 ccusage。
- [ ] 对照 anomalyco/opencode migrations/schema。
- [ ] 解析 session/message/part 或当前新版结构。
- [ ] 匿名 fixtures/db fixture。
- [ ] parser golden tests。

## Phase 8 - State SQLite

- [ ] 执行 `migrations/001_init.sql`。
- [ ] source_session repository。
- [ ] sync_target repository。
- [ ] source_file_state repository。
- [ ] sync_run repository。
- [ ] parser_version 变更重新同步逻辑。

## Phase 9 - Markdown Renderer

- [ ] Session metadata header。
- [ ] User/Assistant sections。
- [ ] Tool Call 渲染。
- [ ] Tool Result 截断。
- [ ] Thinking 默认排除。
- [ ] Image/Attachment 渲染。
- [ ] Markdown escaping/sanitization。
- [ ] renderer golden tests。

## Phase 10 - Secret Sanitizer

- [ ] Bearer Token。
- [ ] OpenAI-style keys。
- [ ] AWS Access Key。
- [ ] password/token/secret/api_key。
- [ ] Authorization Header。
- [ ] 自定义 pattern 配置。
- [ ] 单元测试。

## Phase 11 - SiYuan Sink

- [ ] health check。
- [ ] Token auth。
- [ ] Notebook discover/create。
- [ ] createDocWithMd。
- [ ] updateBlock。
- [ ] setBlockAttrs。
- [ ] upload asset。
- [ ] 获取当前文档用于冲突判断。
- [ ] Integration Tests。

## Phase 12 - Sync Engine

- [ ] 首次发现。
- [ ] content hash。
- [ ] NEW/UPDATED/UNCHANGED。
- [ ] pending 状态。
- [ ] retry/backoff。
- [ ] 冲突检测。
- [ ] missing source。
- [ ] bounded concurrency。
- [ ] 单 Session fault isolation。
- [ ] 单 Provider fault isolation。

## Phase 13 - Incremental Scanner

- [ ] JSONL file_size/mtime/offset。
- [ ] 文件截断检测。
- [ ] 文件替换检测。
- [ ] JSON whole-file hash fallback。
- [ ] OpenCode updated_at/message id strategy。

## Phase 14 - Watcher/Daemon

- [ ] notify watcher。
- [ ] debounce 2s。
- [ ] periodic scan 300s。
- [ ] graceful shutdown。
- [ ] daemon command。

## Phase 15 - Archive

- [ ] NormalizedSession JSON archive。
- [ ] gzip 压缩。
- [ ] 可关闭。
- [ ] 原 source 删除后仍可保留归档。

## Phase 16 - CLI

- [ ] `aiks doctor`。
- [ ] `aiks scan`。
- [ ] `aiks sync`。
- [ ] `aiks sync --source`。
- [ ] `aiks sync --dry-run`。
- [ ] `aiks status`。
- [ ] `aiks daemon`。
- [ ] `aiks resync`。
- [ ] `aiks rebuild-state`。

## Phase 17 - Windows Packaging

- [ ] Windows 路径验证。
- [ ] `%USERPROFILE%` 默认路径验证。
- [ ] `%LOCALAPPDATA%` State DB。
- [ ] 单 EXE 或最小分发包。
- [ ] HKCU Run 或 Task Scheduler 文档。

## Phase 18 - 完整测试与验收

- [ ] 所有 unit tests。
- [ ] 所有 provider golden tests。
- [ ] SiYuan integration tests。
- [ ] 10k Session 级别扫描测试。
- [ ] resume session 不重复建文档。
- [ ] SiYuan offline pending/retry。
- [ ] conflict 场景。
- [ ] source missing 场景。
- [ ] 按 `docs/implementation/acceptance.md` 完成验收。

## V1.5 - Knowledge Extractor

- [ ] 先分析 Mnemos。
- [ ] OpenAI Compatible LLM abstraction。
- [ ] Structured JSON output。
- [ ] KnowledgeItem type enum。
- [ ] confidence。
- [ ] source provenance。
- [ ] Knowledge 去重。
- [ ] 写入 `/20 Knowledge`。

## V2+

- [ ] Cursor Provider。
- [ ] Windsurf Provider。
- [ ] Copilot CLI Provider。
- [ ] Qwen Code Provider。
- [ ] Kimi Provider。
- [ ] OpenClaw/Hermes/Goose 等 Provider。
- [ ] 可选 MarkdownFolderSink。
- [ ] 可选 ObsidianSink。
- [ ] MCP Knowledge Server。
