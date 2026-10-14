# 跨 Session 知识关联与演进：专项产品设计

> 阶段二 S2.3；状态：待开发的详细方案。现有语义去重/关联是基础，不等于已实现知识演进。

## 价值与边界

用户常在 Codex 中排查问题、在 Claude Code 中修改代码、在 OpenCode 中验证。AIKS 需要识别这些操作属于同一工程问题，并能回答“最终怎么解决”“为什么换方案”“哪个版本有效”。系统**不应**将相似文字直接合并成一个知识，也不能把未验证的说法当最终结论。

## 关键用户流程

1. AIKS 获取带 source、project、时间与 Session 消息来源的多个 KnowledgeItem。
2. 用 Embedding/现有语义关联找候选，仅在同项目（或用户显式跨项目确认）范围优先查找。
3. LLM/规则提出关系类型、证据、置信度及理由；低信心只提出候选，不直接确认。
4. 用户在知识详情中查看“相关知识 / 演进时间线”，可确认、撤销、纠正方向。
5. “问 AIKS”查某问题时展示最新已验证方案、旧方案为何废弃、对应原 Session。

## 关系类型

| 类型 | 含义 | 使用条件 |
| --- | --- | --- |
| related | 主题相关，不主张版本替代 | 证据不足以判断因果与时序 |
| supplements | 补充更多前提/细节 | 新知识不否定旧知识 |
| corrects | 指明旧结论的错误与纠正依据 | 有明确差异/复现/反证 |
| supersedes | 旧方案在明确适用范围内被新方案替代 | 证据可核实，记录生效时间 |
| resolved_by | 问题被特定修复/验证活动解决 | 有操作与验证依据 |

支持 `proposed / confirmed / rejected` 等审阅状态；自动断言只能在满足明确证据门槛的规则下发生。

## 领域设计（建议）

复用现有 `KnowledgeItem` 和语义关系存储，扩展 `KnowledgeRelation`：relation_id、from_knowledge_id、to_knowledge_id、type、direction、project_identity、confidence、evidence_session_id、message_range、explanation、judge_model/prompt_version、review_status、created_at/updated_at。另用 `KnowledgeRevision` 记录标题和内容变化，来源与用户编辑保持独立。

必须有 unique key/幂等策略，防止重复扫描时边成倍增长；来源删除、KnowledgeItem rename、提炼重跑时关系仍可迁移或保留可诊断 tombstone。跨项目默认不自动合并。

## UI 设计

- 知识详情：关系摘要卡（相关/补充/纠正/替代/解决）、依据片段、模型建议状态、人工确认入口。
- 项目记忆：按时间展示“提出问题→调查→采用方案→修正→最终验证”。
- 搜索/问答：允许切换“最新可用结论”和“完整历史”，链接每个证据 Session。
- 矛盾视图：仅标注“可能矛盾”，不替用户直接删除旧知识。

## 验收用例

- 同项目不同 Agent 三个 Session 串成正确链；旧方案可查看且不被覆写。
- 相似文字但不同项目不得自动连成 `supersedes`。
- 仅讨论未验证的技术方案不得标记为 `resolved_by`。
- 重跑提炼、重启与备份恢复后关系不重建重复，人工否决不被自动重新确认。
- 构建标注集评估关系 Precision、错误替代比例、来源可追溯率。

## 实施依赖

先完成阶段一的数据可靠性，再完成阶段二的基准与质量反馈机制。本文件设计为长期产品能力，不以一次迭代全部完成。
