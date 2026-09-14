/// V3 Prompt templates — produce 0~N KnowledgeItems per session
pub const PROMPT_VERSION_V3: &str = "knowledge-v3";

pub const SYSTEM_PROMPT_V3: &str = r#"你是企业研发工作知识整理器（V3）。

目标：将 AI 编程会话转换为 0~N 条独立的、可复用的工程知识条目。

每个知识条目代表会话中的一个独立主题或问题。
一个长会话可能包含多个不同问题——请将它们拆分为独立条目，而非混在一起。

优先保留：
- 问题、错误信息、根因分析、解决方案
- 设计决策、技术选型
- 关键代码、命令、配置、文件路径
- API 接口、版本号、环境变量

category 必须是：troubleshooting, implementation, architecture, configuration, decision, research, general

输出格式：严格 JSON，无代码块标记。

JSON 硬性要求：
- 所有键名必须用双引号包裹（不要输出 JS 风格的裸键名）
- 字符串一律用双引号，不要用单引号
- 不要输出注释、省略号或 JSON 以外的任何文字
- 不要思考过程，直接输出最终 JSON
- 输出的第一个字符必须是 `{`

/no_think"#;

/// Prompt for a single session (short sessions)
pub fn make_v3_extraction_prompt(session_text: &str) -> String {
    format!(
        r#"请分析以下 AI 编程会话，提取所有独立的工程知识条目。

会话内容：
{}

输出 JSON：
{{
  "session_summary": "整个会话的一句话摘要",
  "knowledge_score": 0.0-1.0,
  "worth_extracting": true/false,
  "items": [
    {{
      "title": "简洁标题（50字以内）",
      "category": "troubleshooting/implementation/architecture/configuration/decision/research/general",
      "summary": "2-3句摘要",
      "content": "详细内容（Markdown格式，包含所有关键信息）",
      "problem": "问题描述（可为null）",
      "root_causes": ["根因1", "根因2"],
      "solutions": ["解决方案1"],
      "key_commands": ["命令1"],
      "key_files": ["文件路径"],
      "decisions": ["决策说明"],
      "tags": ["标签1", "标签2"],
      "confidence": 0.0-1.0
    }}
  ]
}}"#,
        session_text
    )
}

/// Prompt for chunk summarization (for long sessions)
pub fn make_v3_chunk_prompt(chunk_text: &str, chunk_index: usize, total_chunks: usize) -> String {
    format!(
        r#"这是长会话的第 {}/{} 部分。提取这部分的关键信息。

内容：
{}

输出 JSON（纯文本，无代码块）：
{{
  "chunk_index": {},
  "topics": ["主题1", "主题2"],
  "key_errors": ["错误1"],
  "key_commands": ["命令1"],
  "key_files": ["文件1"],
  "decisions": ["决策1"],
  "summary": "这部分的核心内容（200字以内）"
}}"#,
        chunk_index + 1,
        total_chunks,
        chunk_text,
        chunk_index
    )
}

/// Final extraction from chunk summaries (Map-Reduce)
pub fn make_v3_final_prompt(title: &str, project: Option<&str>, chunk_summaries: &[String]) -> String {
    let sums = chunk_summaries
        .iter()
        .enumerate()
        .map(|(i, s)| format!("## 第{}部分摘要\n{}", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        r#"基于以下各部分摘要，提取完整的工程知识条目。

会话标题: {}
项目: {}

各部分摘要:
{}

输出完整 JSON（同上述 Schema，items 数组包含所有独立知识点）："#,
        title,
        project.unwrap_or("未知"),
        sums
    )
}
