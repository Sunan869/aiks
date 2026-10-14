# AIKS 单机版产品演进设计文档索引

> 版本：2026-10-09；状态：产品规划与方案设计，**不代表功能均已实现**。仅针对单机 local-first 版。

## 文档分工

- [产品能力蓝图](./offline-product-blueprint.md)：完整能力地图、产品定位、场景、模块边界、优先级与长期方向。
- [跨 Session 知识关联与演进](./cross-session-knowledge-evolution.md)：跨工具持续知识链、关系图谱、版本与决策证据。
- [知识质量闭环](./knowledge-quality-feedback.md)：质量评价、用户纠错、去重、复审与可追溯性。
- [同步与 AI 任务监控中心](./sync-and-ai-task-center.md)：同步、提炼、Embedding、发布、错误恢复与诊断。
- [自动日报/周报/项目回顾](./work-reports-and-project-reviews.md)：活动汇总、项目进度、证据和 Markdown 报告。
- [知识反哺 AI 编程工具](./knowledge-to-coding-agents.md)：AGENTS.md / CLAUDE.md、上下文包、可控文件导出与未来 MCP。
- [三阶段执行路线图](../plans/2026-10-09-offline-knowledge-evolution-roadmap.md)：分阶段实施任务、顺序、测试及退出门槛。

## 阅读建议

先阅读产品能力蓝图理解长期目标，再按专项文档确定产品设计，最后依据路线图实施。

**优先级**：第一阶段（可靠性与监控）→ 第二阶段（提炼质量与知识演进）→ 第三阶段（项目记忆、报告与 Agent 反哺）。专项设计可以提前完成，但代码开发遵循阶段门槛。

## 产品设计共识

1. 本地优先；不强制使用联网 API，不上传用户原始会话。
2. Provider 只读；用户修订、SiYuan 文档与 Agent 指令不被后台静默改写。
3. 每一条 AI 产生的结论必须有来源或者标记为推断。
4. 可观测、可撤销、可恢复；任何自动合并与覆盖必须遵守权限和冲突保护。
5. 与现有 SQLite、Pipeline、Embedding/FTS、SiYuan 和问 AIKS 保持兼容；不重复造相同能力。
6. 独立产品文档写需求/数据/交互/验收，执行计划写任务/阶段/里程碑。所有“待开发”不视为已实现。
