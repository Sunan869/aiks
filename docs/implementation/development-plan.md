# 开发实施计划

## Phase 0：研究上游

目标：在写任何 Parser 前确认上游实现。

产物：

- 完整 `docs/reference-analysis/*.md`
- 固定 Commit SHA
- THIRD_PARTY_NOTICES.md 更新

## Phase 1：基础工程

目标：构建 Rust 项目骨架、配置系统、SQLite migration、CLI 框架。

完成后应能：

```text
aiks doctor
aiks status
```

即使 Provider 尚未实现，也应有结构化输出。

## Phase 2：Canonical Model

目标：所有 Provider 后续只输出 Canonical Model。

要求先写 model tests，再写 Provider。

## Phase 3：四种 Provider

顺序建议：

1. Codex
2. Claude
3. OpenCode
4. Gemini

原因：Codex/Claude JSONL 较适合先验证统一模型；OpenCode 验证 SQLite；Gemini 最后处理多格式兼容。

每个 Provider 完成条件：

- 自动发现路径
- discover_sessions
- load_session
- health_check
- 匿名 fixtures
- golden tests
- Unknown Event fail-soft

## Phase 4：Renderer + Sanitizer

先将 NormalizedSession 稳定转换为 Markdown。

此阶段不连接 SiYuan也可以独立验收。

## Phase 5：SiYuan Sink

实现：

- Health
- Auth
- Notebook
- Create Document
- Update Block
- Attrs
- Assets

所有接口先做 integration tests。

## Phase 6：Sync Engine

组合：

```text
Provider → Hash → State → Render → Sink
```

完成 NEW/UPDATED/UNCHANGED/FAILED/PENDING/CONFLICT。

## Phase 7：Incremental + Daemon

实现：

- File Watcher
- Periodic Scanner
- JSONL incremental state
- OpenCode changed session detection
- bounded concurrency

## Phase 8：Archive + Recovery

实现 NormalizedSession gzip archive 和 `rebuild-state`。

## Phase 9：Windows Release

完成：

- Windows paths
- config 初始化
- 日志目录
- 单 exe/发行包
- 开机启动说明

## Phase 10：V1 验收

严格执行 `acceptance.md`。

未通过全部关键场景不得标记 V1 完成。

## V1.5

再开发 Knowledge Extractor。
