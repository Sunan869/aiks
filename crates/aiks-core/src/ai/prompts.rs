// CI lint baseline: pre-existing Clippy debt; remove allowances incrementally.
#![allow(clippy::empty_line_after_doc_comments)]

/// Prompt templates for the Knowledge Extractor.
/// Prompts are versioned to support future re-extraction with new prompts.

pub const PROMPT_VERSION: &str = "knowledge-extractor-v1";
pub const CHUNK_PROMPT_VERSION: &str = "chunk-summary-v1";

/// System prompt for knowledge extraction
pub const SYSTEM_PROMPT: &str = r#"你是企业研发工作知识整理器。

目标不是简单摘要，而是将 AI 编程会话转换为可复用的工程知识。

优先保留：
- 问题描述、错误信息和异常现象
- 最终解决方案（不是尝试过程）
- 设计决策和技术选型理由
- 关键代码片段、命令和文件路径
- API 接口、配置项、版本号
- 验证结果和性能数据

压缩：
- 重复的解释和尝试
- 冗余的编译日志和 stdout
- 机械性的 Tool Output
- 已被替代的中间步骤
- 闲聊和无效交互

禁止：
- 编造不存在的事实
- 猜测未在会话中出现的信息

输出格式：严格按照指定 JSON Schema 输出，不要包含 markdown 代码块标记。

JSON 硬性要求：
- 所有键名必须用双引号包裹
- 不要思考过程，直接输出最终 JSON
- 输出的第一个字符必须是 `{`

分类（category）必须是以下之一：
troubleshooting（故障排查）、implementation（实现）、design（设计）、research（研究）、decision（决策）、general（通用）

knowledge_score 评分标准：
- 0.9+：重要的技术问题解决、架构决策、关键实现
- 0.7-0.9：有价值的技术探索、解决方案
- 0.5-0.7：一般性技术讨论
- <0.5：简单修改、询问、测试性对话（不值得提炼）

/no_think"#;

/// User prompt template for extraction
pub fn make_extraction_prompt(session_text: &str) -> String {
    format!(
        r#"请分析以下 AI 编程会话，提取关键工程知识。

会话内容：
{}

请以 JSON 格式输出知识文档，包含以下字段：
{{
  "worth_extracting": true/false,
  "knowledge_score": 0.0-1.0,
  "title": "简洁标题（50字以内）",
  "summary": "2-3句摘要",
  "project": "项目名称（从路径或上下文推断，可为null）",
  "category": "troubleshooting/implementation/design/research/decision/general",
  "tags": ["标签1", "标签2"],
  "problem": "核心问题描述（troubleshooting类必填）",
  "symptoms": ["现象1", "现象2"],
  "root_causes": ["根因1", "根因2"],
  "solutions": ["解决方案1", "解决方案2"],
  "decisions": ["决策1", "决策2"],
  "key_commands": ["命令1", "命令2"],
  "key_files": ["文件1", "文件2"],
  "todos": ["待办1"],
  "confidence": 0.0-1.0
}}"#,
        session_text
    )
}

/// User prompt for chunk summarization
pub fn make_chunk_prompt(chunk_text: &str, chunk_index: usize, total_chunks: usize) -> String {
    format!(
        r#"这是一个长会话的第 {}/{} 部分。请提取这部分的关键信息。

内容：
{}

以 JSON 输出：
{{
  "chunk_index": {},
  "summary": "这部分的核心内容（100字以内）",
  "important_errors": ["关键错误1"],
  "decisions": ["决策1"],
  "commands": ["命令1"],
  "files": ["文件1"],
  "todos": ["待办1"]
}}"#,
        chunk_index + 1,
        total_chunks,
        chunk_text,
        chunk_index
    )
}

/// Final extraction prompt using chunk summaries
pub fn make_final_extraction_prompt(
    session_title: &str,
    project: Option<&str>,
    chunk_summaries: &[String],
) -> String {
    let summaries = chunk_summaries
        .iter()
        .enumerate()
        .map(|(i, s)| format!("## 第{}部分\n{}", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        r#"请基于以下会话各部分的摘要，生成完整的知识文档。

会话标题: {}
项目: {}

各部分摘要:
{}

请以 JSON 格式输出最终知识文档（同上述 Schema）。"#,
        session_title,
        project.unwrap_or("未知"),
        summaries
    )
}
