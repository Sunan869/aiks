# S3.2 日报/周报草稿修订、复制与重启恢复验收

## Windows 实机操作

在独立数据目录启动 AIKS Desktop，连接真实 Codex、Claude Code、OpenCode 采集的多个同路径项目 Session。先确认 Project Memory 能正确区分「相同显示名、不同绝对路径」的项目。进入项目长期记忆 →「工作回顾与 Agent 上下文」，选择一个项目与来源，点击「今天（日报）」「本周（周报）」分别检查 Windows 本地日历日期；测试周一和周日、临近午夜（不得用 UTC 误切换日期）。生成回顾后检查：仅包含所选日期内**知识更新时间**范围的证据，并不将计划/讨论自动判定为已交付。

在可编辑预览中手工改写段落，必须显式点击「保存到本地报告草稿」才创建一个本地 draft ID/v1。继续修改并再次点击，检查出现 v2，旧 v1 内容保持不变。使用「已保存回顾草稿」与「回顾历史修订版本」依次打开 v1、v2；选择旧 v1 后修改并保存，必须新建 v3，而不是覆盖已有 v1/v2。退出 AIKS Desktop 并重新启动，所有这些版本仍应从现有 SQLite 恢复；另一项目的已保存草稿不得混入当前选择。

失败与竞态：在两个窗口中同时打开 v1 并各自修改，一窗口保存为 v2 后另一个持旧 expectedRevision 保存应明确失败、保留 v2；重新载入后才允许另存 v3。改变项目、日期、来源后不得继续以旧草稿 ID 保存改变范围的内容。无效日期/空白文本/超 250000 字节均应明确拒绝。首次保存之前关闭预览，不应留下任何新草稿。切换历史稿覆盖当前未保存编辑须弹窗确认。

安全边界：保存审阅草稿只写 AIKS 本地 `project_report_draft`/`project_report_revision` 表；不发送报告到外部模型、邮件、聊天工具或 SiYuan，不修改 AGENTS.md、CLAUDE.md、Provider 源文件。原 Markdown 下载/复制行为仍保留。输出事实与工作进度由人逐条核对原始 Knowledge/Session；CI/SQLite Mock 只证明代码边界，不等于真实 Windows 数据或模型质量验收成功。

代码级入口：`crates/aiks-core/src/knowledge/project_report_drafts.rs` 的 SQLite 单测，`apps/aiks-desktop/src/project-report-dates.test.ts` 的前端 Mock/日期/用户动作测试。S1.3 备份与恢复仍最后实施。
