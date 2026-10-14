# S1.2 处理详情安全重试和首次 AI 入队验收

在 Windows 隔离 AIKS 数据目录使用真实 Codex、Claude Code、OpenCode Session，检查嵌入式 SiYuan 原文已完成同步。以下用真实失败/实际 SQLite job 状态逐项验证，而不是仅靠固定 Mock 测试。

1. 让已同步 Session 的真实 AI 提炼因上游故障进入 FAILED，打开历史处理详情并点击「安全重试失败阶段」。底层必须调用 `retry_failed_ai_task`，且只能产生一个新的持久 Job；不允许失败时自动退回无条件 `run_pipeline_for_session`。旧 runId 不得混同当前的重试结果。恢复模型后观察实际 PENDING → RUNNING → READY。
2. RAW_ONLY 会话可通过原始处理入口请求重新提交，但 Rust `load_initial_pipeline_session` 必须拒绝尚未写入 SiYuan sync_target、已产生 CONFLICT、Session 已标为 missing、有旧 FAILED/READY/PROCESSING/DISCOVERED V3 Pipeline 或已存在 PENDING/RUNNING Job 的状态。直接 Tauri invoke 也应被相同门禁拦截。
3. 已完成 READY 的历史 Session 即使数据库还保存更旧 FAILED Job，也不能再次按 FAILED 安全重试；当前有 PENDING/RUNNING 时拒绝重复提交。多个窗口同时点击验证只有一条有效当前队列工作，保留原有 Lease/attempt 代次保护。
4. 让用户手动编辑 SiYuan 的原 Session 文档制造同步 CONFLICT，两个入口均不能自动覆盖或将冲突当成可重试 AI 错误；保留用户手工编辑文本与原 Provider 源文件字节。
5. 停机重启后重复查看旧 ProcessingDetail、任务中心的状态与错误反馈，不得误显示「已重新提交」或把历史 FAILED 当成最新已验收处理结果。

检查命令/自动化入口：`crates/aiks-core/src/pipeline/first_run.rs` 真实 SQLite 测试与 `apps/aiks-desktop/src/processing-detail-action.test.ts`。Rust CI/Windows Provider Actions 仅是合成验证，真实模型质量与 Windows/SiYuan 故障注入仍需集中执行。完整备份恢复 S1.3 继续最后开发。
