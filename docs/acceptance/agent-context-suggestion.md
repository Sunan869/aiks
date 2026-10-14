# S3.3 Agent 知识上下文建议片段验收入口

项目记忆 → 工作回顾与 Agent 上下文：选择已确认来源、Token 预算，再选择**通用 Markdown 上下文**、**AGENTS.md 建议片段**或 **CLAUDE.md 建议片段**。点击「生成 Agent 上下文」，内容必须来自既有脱敏 Core 上下文服务；错误/过时反馈不能重新进入建议片段。格式切换只改变审核显示，不得隐式调用项目文件写入或联网功能。

建议片段需明确标为“参考资料”，把来自 Session/模型的 Markdown 标题、代码块和看似指令的内容全部变成 blockquote 引用，避免直接当成 Agent 的执行指令。生成后允许在 textarea 中人工编辑；按保存只产生独立名称 `AIKS-<project>-AGENTS-suggestion.md`、`AIKS-<project>-CLAUDE-suggestion.md` 或 `AIKS-<project>-agent-context.md`，而不是直接写入现存 `AGENTS.md`、`CLAUDE.md`。用户需自行检查差异，决定是否人工复制或合并。

人工集中测试需要检验：切换项目、来源、报告日期、Token 预算或建议格式时，旧可编辑预览立即清除；请求尚未完成时切换上述参数，过期响应不得出现于新视图。应以包含英文技术说明、中文摘要、Markdown 代码块、带敏感标识的模拟项目核对脱敏、证据引用与引用块保护。验证仅本机复制/保存、Provider 不写入、SiYuan 用户原文不覆盖。

自动化只验证格式转换、文件名净化和工作台调用契约；不得把这些测试或 Windows Desktop 编译成功当作真实 Agent 使用成功。S1.3 备份/恢复仍排在最后实现。
