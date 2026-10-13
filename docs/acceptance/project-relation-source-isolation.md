# S2.3 同名项目 / 未知路径知识关联来源隔离验收

在 Windows 独立 AIKS 数据目录准备 Codex Session A、Claude Code Session B，两者故意使用相同项目显示名称，但来源目录分别是 `C:/repo/alpha`、`D:/repo/alpha`；另准备 OpenCode Session C，真实绝对路径与 A 相同但 Provider 展示名不同，以及两个都只有项目显示名、**没有可信 cwd/project_path** 的 Session D/E。每条 Session 提炼两条活跃 Knowledge，且都能打开原始证据。

进入知识详情 → 「跨 Session 知识演进关系」人工创建带来源证据的关系：A+B 必须拒绝，A+C 必须允许提交建议（但不能自动确认），D+E 即使显示名称相同也必须拒绝，A+D 或 A+手动知识必须拒绝。D 内两条来自同一个 canonical Session 的知识允许建议；两条用户手动知识如果明确填写了同一个项目名，可继续沿用手工显式审核入口。将 E 在项目记忆页**人工**归到 A 的经核实路径后，E 与 A 的候选可重新提交，再由人核对原文决定确认与否。候选下拉列表可展示 200 条以内的知识，不表示候选都被后端授权为同一项目。

故障场景：先让 A+C 形成 suggested，再在用户审核前把 C 标记为 missing，或把其路径改为另一个项目；确认操作必须失败，且原关系继续处于 suggested，既有知识内容不变。用户仍可拒绝该错误建议，留下 `knowledge_relation_review` 人工审核历史。对已归档/删除端点同样拒绝新关联或确认。关系更新时间不得冒充方案生效时间。

CI/Core SQLite/Mock 测试只能证明固定场景的代码分支，不能声称真实 Windows 三 Provider/内嵌 SiYuan 的实际来源扫描或模型事实质量已经验收。任何关联操作不得写回 Provider 原始文件、删除 Knowledge 或覆盖 SiYuan 用户人工修改。S1.3 完整备份恢复最后实施。
