# AIKS Roadmap

本文件只记录**当前仍未完成**或明确需要后续推进的工作。已经落地的 V1/V3 能力不再保留成未勾选的初始化清单；历史实现计划可从 Git 历史和 `docs/` 中追溯。

## 当前基线

截至当前 V3 集成分支，以下主链路已经存在：

- Claude Code / Codex / Gemini CLI / OpenCode Provider。
- Canonical Session Model、增量同步、Watcher/Scanner、Archive。
- SQLite 状态库与正式 migration 链。
- 持久化 Pipeline Job、lease、重试和重启恢复。
- AI Knowledge Extraction、KnowledgeItem/Chunk。
- 可选 Embedding、FTS + bounded vector rerank 混合搜索。
- Rust CLI。
- React/Vite + Tauri Desktop。
- SiYuan Session / Knowledge 同步、冲突检测和映射。
- Secret Sanitizer 与较完整的 Rust 回归测试。

P0 与 P1 代码评审问题已经在 `review/integration` 中集中处理；后续不要按照旧 V1 Phase 重新实现这些能力。

## P2 — Repository Engineering

### #11 Permanent CI / Frontend Test / Repository Hygiene

- [ ] 增加正式 GitHub Actions CI，至少覆盖：
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - Rust workspace tests
  - Desktop frontend build
- [ ] 为 Desktop 增加最小可执行前端测试基线，并接入 CI。
- [ ] 若项目继续声明 MIT，补齐仓库根 `LICENSE` 并与 Cargo metadata 保持一致。
- [ ] 清理/忽略 `.icon-output` 等生成物，明确 canonical icon/source assets。
- [ ] 在 CI 中阻止已知内部 endpoint、`.workbuddy` 等公开仓库卫生回归。

跟踪：GitHub Issue #11。

### #12 Public History / Sensitive-data Audit

- [ ] 对 Git 历史执行一次专门的敏感信息和内部拓扑审计。
- [ ] 区分真实凭据、私网拓扑、普通历史文本，输出影响清单。
- [ ] 如果发现真实 secret，先轮换凭据，再决定 history rewrite。
- [ ] 如果仅需要移除历史内部拓扑，制定 rewrite 范围、备份和协作者迁移方案。
- [ ] **未经明确确认，不直接 force-rewrite `main` 或所有公开 refs。**

跟踪：GitHub Issue #12。

## V3 后续产品演进候选

以下不是当前 P2 阻塞项，实施前应单独建 Issue、定义验收标准并评估兼容性。

### Knowledge Identity

- [ ] 在 Extraction schema 中评估显式 stable key，安全支持 KnowledgeItem rename，而不是依赖 title/category 模糊猜测。
- [ ] 为 `REMOVED` SiYuan mapping 定义可选的远端 archive/delete 生命周期策略。

### Search / Vector

- [ ] 当数据规模证明 512-candidate rerank 不足时，评估 sqlite-vec 或其它本地 ANN/vector index。
- [ ] 增加可重复的搜索质量/性能 benchmark，而不是仅以最终 top-N 数量判断扩展性。

### Providers

PR #46 的 11 个本地来源已有原生适配器，精确格式与自动化证据见 `docs/reference-analysis/multi-provider-support.md`；不再将同一批来源作为尚未开始的重复开发任务。

- [ ] 对上述来源完成真实安装环境与版本矩阵验收（尤其 Windows 自动目录发现）。
- [ ] Antigravity IDE 的真实消息存储：当前仅统计缓存不支持，不把 usage 伪造为对话。
- [ ] 尚未覆盖的历史/未来格式按匿名 fixture 增量适配；Cursor Agent 非 JSONL、新 Kimi 子 Agent 等不因来源名称已注册就视为已验证。
- [ ] Windsurf 仍是单独候选，不属于本次 11 个来源范围。

### Distribution / Operations

- [ ] 固化 Windows/macOS/Linux Tauri 打包验证。
- [ ] 明确 SiYuan runtime resources 的开发、CI 与发行包来源，消除空 checkout 需要临时 placeholder 的情况。
- [ ] 建立版本发布、升级和已有 SQLite migration 兼容性检查流程。

## 开发规则

- 当前任务状态以 GitHub Issues 为准；完成项及时关闭 Issue，不在本文件复制完整实施细节。
- 新的可靠性问题按 P0/P1/P2 或明确 severity 单独建 Issue。
- 行为修复先补能复现问题的回归测试。
- 不为局部修复引入无关全仓格式化或大范围重构。
- `README.md`、`AGENTS.md` 和本文件必须描述当前代码，不得重新退化为早期 V1 设计说明。
