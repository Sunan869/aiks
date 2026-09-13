# 给 Coding Agent 的启动 Prompt

你现在需要完整实现此仓库中的 AI Knowledge Sync（AIKS）项目。

开始写代码前必须先阅读：

1. `AGENTS.md`
2. `README.md`
3. `docs/design/01-system-design.md`
4. `docs/design/02-open-source-reuse.md`
5. `TODO.md`
6. `docs/implementation/development-plan.md`
7. `docs/implementation/acceptance.md`

随后执行 Phase 0：

- Clone `references/README.md` 中的全部上游仓库。
- 固定每个仓库 Commit SHA。
- 完成 `docs/reference-analysis/` 中的上游分析。
- 更新 `THIRD_PARTY_NOTICES.md`。

在 Phase 0 完成以前，不允许开始自己编写 Claude/Codex/Gemini/OpenCode Parser。

实现时必须遵守以下硬性边界：

- 不开发新的知识库系统。
- 不开发 Vector DB / Embedding / RAG / Search UI / Note Editor。
- 首期唯一 Knowledge Sink 是 SiYuan。
- 所有 AI Session Source 严格只读。
- Provider 不直接调用 SiYuan。
- Canonical Model 是模块之间唯一的数据契约。
- Parser 优先从 AICoder Session Viewer 移植/重构。
- CC Switch 和 ccusage 用于交叉验证采集和兼容逻辑。
- 官方仓库是格式变化时的 Source of Truth。
- 每完成一个 Provider 必须提交匿名 Fixture 和 Golden Test。
- 单 Session 错误必须 fail-soft。
- 所有第三方派生代码必须写入 THIRD_PARTY_NOTICES.md。

请严格按照 `TODO.md` 的 Phase 顺序实施，并在每个 Phase 完成后运行测试和更新进度。

V1 完成的唯一判据是 `docs/implementation/acceptance.md` 全部关键项通过，而不是“代码基本完成”。
