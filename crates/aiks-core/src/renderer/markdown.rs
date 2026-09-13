/// Renders a NormalizedSession as Markdown suitable for SiYuan.
/// Sections: metadata header, User/Assistant messages, Tool Calls and Results.
use crate::config::ContentConfig;
use crate::model::{ContentBlock, MessageRole, NormalizedSession};
use crate::util::SecretSanitizer;

pub struct MarkdownRenderer {
    config: ContentConfig,
    sanitizer: Option<SecretSanitizer>,
}

impl MarkdownRenderer {
    pub fn new(config: ContentConfig, redact_secrets: bool) -> Self {
        let sanitizer = if redact_secrets {
            Some(SecretSanitizer::new())
        } else {
            None
        };
        Self { config, sanitizer }
    }

    /// Render a NormalizedSession to a Markdown string.
    pub fn render(&self, session: &NormalizedSession) -> String {
        let mut out = String::with_capacity(4096);

        // Title
        let title = session.title.as_deref().unwrap_or("Untitled Session");
        out.push_str(&format!("# {}\n\n", self.escape_inline(title)));

        // Metadata block
        out.push_str("> **来源 (Source):** ");
        out.push_str(session.source.display_name());
        out.push('\n');

        if let Some(project) = session.infer_project_name() {
            out.push_str(&format!("> **项目 (Project):** {}\n", self.escape_inline(&project)));
        }

        if let Some(path) = &session.project_path {
            out.push_str(&format!("> **路径 (Path):** `{}`\n", path));
        }

        out.push_str(&format!(
            "> **Session ID:** `{}`\n",
            &session.external_session_id
        ));

        if let Some(model) = &session.model {
            out.push_str(&format!("> **模型 (Model):** {}\n", self.escape_inline(model)));
        }

        if let Some(started) = &session.started_at {
            out.push_str(&format!(
                "> **开始 (Started):** {}\n",
                started.format("%Y-%m-%d %H:%M:%S UTC")
            ));
        }

        if let Some(updated) = &session.updated_at {
            out.push_str(&format!(
                "> **更新 (Updated):** {}\n",
                updated.format("%Y-%m-%d %H:%M:%S UTC")
            ));
        }

        out.push('\n');

        // Messages
        for msg in &session.messages {
            // Skip system messages if not configured
            if msg.role == MessageRole::System && !self.config.include_system {
                continue;
            }

            let heading = match msg.role {
                MessageRole::User => "## 👤 用户 (User)",
                MessageRole::Assistant => "## 🤖 助手 (Assistant)",
                MessageRole::System => "## ⚙️ 系统 (System)",
                MessageRole::Tool => "## 🔧 工具 (Tool)",
                MessageRole::Unknown => "## ❓ Unknown",
            };
            out.push_str(heading);
            out.push_str("\n\n");

            for block in &msg.blocks {
                self.render_block(&mut out, block);
            }

            out.push('\n');
        }

        out
    }

    fn render_block(&self, out: &mut String, block: &ContentBlock) {
        match block {
            ContentBlock::Text { text } => {
                let sanitized = self.sanitize(text);
                out.push_str(&sanitized);
                out.push_str("\n\n");
            }

            ContentBlock::Thinking { text } => {
                if !self.config.include_thinking {
                    return; // Skip thinking blocks by default
                }
                out.push_str("<details>\n<summary>💭 Thinking</summary>\n\n");
                let sanitized = self.sanitize(text);
                out.push_str(&sanitized);
                out.push_str("\n\n</details>\n\n");
            }

            ContentBlock::ToolCall { id, name, input } => {
                if !self.config.include_tool_calls {
                    return;
                }
                out.push_str(&format!("### 🔧 工具调用 (Tool Call): `{}`\n\n", name));
                if let Some(id) = id {
                    out.push_str(&format!("*ID: `{}`*\n\n", id));
                }
                let input_str = if input.is_null() {
                    "{}".to_string()
                } else {
                    serde_json::to_string_pretty(input).unwrap_or_else(|_| input.to_string())
                };
                let sanitized = self.sanitize(&input_str);
                out.push_str("```json\n");
                out.push_str(&sanitized);
                out.push_str("\n```\n\n");
            }

            ContentBlock::ToolResult { id: _, content, is_error } => {
                if !self.config.include_tool_results {
                    return;
                }
                let prefix = if *is_error {
                    "### 📤 工具结果 (Tool Result) ⚠️ Error\n\n"
                } else {
                    "### 📤 工具结果 (Tool Result)\n\n"
                };
                out.push_str(prefix);

                // Truncate long tool results
                let truncated = if content.len() > self.config.max_tool_result_chars {
                    let truncation_msg = format!(
                        "\n\n*... [内容截断 / Content truncated: {} → {} chars]*",
                        content.len(),
                        self.config.max_tool_result_chars
                    );
                    let trunc = &content[..self.config.max_tool_result_chars];
                    format!("{}{}", trunc, truncation_msg)
                } else {
                    content.clone()
                };

                let sanitized = self.sanitize(&truncated);
                out.push_str("```\n");
                out.push_str(&sanitized);
                out.push_str("\n```\n\n");
            }

            ContentBlock::Image { .. } => {
                out.push_str("*[图片附件 / Image attachment]*\n\n");
            }

            ContentBlock::FileReference { path, name } => {
                let display = name.as_deref().unwrap_or(path.as_str());
                out.push_str(&format!("*[文件引用 / File: `{}`]*\n\n", display));
            }

            ContentBlock::Unknown { raw } => {
                out.push_str("*[未知内容块 / Unknown content block]*\n\n");
                if !raw.is_null() {
                    let raw_str = serde_json::to_string_pretty(raw).unwrap_or_default();
                    out.push_str("```json\n");
                    out.push_str(&raw_str);
                    out.push_str("\n```\n\n");
                }
            }
        }
    }

    fn sanitize(&self, text: &str) -> String {
        if let Some(sanitizer) = &self.sanitizer {
            sanitizer.sanitize(text)
        } else {
            text.to_string()
        }
    }

    /// Escape Markdown inline formatting characters (for use in headings/text).
    fn escape_inline(&self, text: &str) -> String {
        text.replace('`', "\\`")
            .replace('*', "\\*")
            .replace('_', "\\_")
            .replace('[', "\\[")
            .replace(']', "\\]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use std::collections::HashMap;

    fn make_renderer() -> MarkdownRenderer {
        MarkdownRenderer::new(ContentConfig::default(), false)
    }

    fn make_session() -> NormalizedSession {
        NormalizedSession {
            source: SourceKind::ClaudeCode,
            external_session_id: "test-sess-1".to_string(),
            title: Some("How do I sort a list?".to_string()),
            project_name: None,
            project_path: Some("/home/user/project".to_string()),
            source_path: None,
            started_at: None,
            updated_at: None,
            model: Some("claude-opus-4-5".to_string()),
            messages: vec![
                NormalizedMessage {
                    external_id: "msg-1".to_string(),
                    parent_id: None,
                    role: MessageRole::User,
                    created_at: None,
                    model: None,
                    blocks: vec![ContentBlock::Text {
                        text: "How do I sort a list in Python?".to_string(),
                    }],
                    usage: None,
                    metadata: HashMap::new(),
                },
                NormalizedMessage {
                    external_id: "msg-2".to_string(),
                    parent_id: Some("msg-1".to_string()),
                    role: MessageRole::Assistant,
                    created_at: None,
                    model: Some("claude-opus-4-5".to_string()),
                    blocks: vec![
                        ContentBlock::Text {
                            text: "Use sorted() or list.sort()".to_string(),
                        },
                        ContentBlock::ToolCall {
                            id: Some("tool-1".to_string()),
                            name: "bash".to_string(),
                            input: serde_json::json!({"command": "python3 -c 'print(sorted([3,1,2]))'"})
                        },
                    ],
                    usage: None,
                    metadata: HashMap::new(),
                },
                NormalizedMessage {
                    external_id: "msg-3".to_string(),
                    parent_id: Some("msg-2".to_string()),
                    role: MessageRole::Tool,
                    created_at: None,
                    model: None,
                    blocks: vec![ContentBlock::ToolResult {
                        id: Some("tool-1".to_string()),
                        content: "[1, 2, 3]\n".to_string(),
                        is_error: false,
                    }],
                    usage: None,
                    metadata: HashMap::new(),
                },
            ],
            usage: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn renders_session_with_title() {
        let renderer = make_renderer();
        let session = make_session();
        let md = renderer.render(&session);
        assert!(md.contains("# How do I sort a list?"));
    }

    #[test]
    fn renders_metadata_block() {
        let renderer = make_renderer();
        let session = make_session();
        let md = renderer.render(&session);
        assert!(md.contains("Claude Code"));
        assert!(md.contains("test-sess-1"));
    }

    #[test]
    fn renders_user_assistant_messages() {
        let renderer = make_renderer();
        let session = make_session();
        let md = renderer.render(&session);
        assert!(md.contains("## 👤 用户 (User)"));
        assert!(md.contains("## 🤖 助手 (Assistant)"));
        assert!(md.contains("How do I sort a list in Python?"));
        assert!(md.contains("Use sorted() or list.sort()"));
    }

    #[test]
    fn renders_tool_call() {
        let renderer = make_renderer();
        let session = make_session();
        let md = renderer.render(&session);
        assert!(md.contains("🔧 工具调用"));
        assert!(md.contains("bash"));
        assert!(md.contains("sorted"));
    }

    #[test]
    fn renders_tool_result() {
        let renderer = make_renderer();
        let session = make_session();
        let md = renderer.render(&session);
        assert!(md.contains("📤 工具结果"));
        assert!(md.contains("[1, 2, 3]"));
    }

    #[test]
    fn skips_thinking_by_default() {
        let renderer = MarkdownRenderer::new(ContentConfig { include_thinking: false, ..Default::default() }, false);
        let mut session = make_session();
        session.messages[1].blocks.push(ContentBlock::Thinking {
            text: "Let me think...".to_string(),
        });
        let md = renderer.render(&session);
        assert!(!md.contains("Let me think..."));
    }

    #[test]
    fn includes_thinking_when_configured() {
        let renderer = MarkdownRenderer::new(ContentConfig { include_thinking: true, ..Default::default() }, false);
        let mut session = make_session();
        session.messages[1].blocks.push(ContentBlock::Thinking {
            text: "This is my reasoning".to_string(),
        });
        let md = renderer.render(&session);
        assert!(md.contains("This is my reasoning"));
    }

    #[test]
    fn truncates_long_tool_result() {
        let renderer = MarkdownRenderer::new(
            ContentConfig {
                max_tool_result_chars: 10,
                ..Default::default()
            },
            false,
        );
        let mut session = make_session();
        // Replace tool result with long content
        session.messages[2].blocks = vec![ContentBlock::ToolResult {
            id: None,
            content: "a".repeat(1000),
            is_error: false,
        }];
        let md = renderer.render(&session);
        assert!(md.contains("Content truncated"));
    }

    #[test]
    fn sanitizes_secrets_when_enabled() {
        let renderer = MarkdownRenderer::new(ContentConfig::default(), true);
        let mut session = make_session();
        session.messages[0].blocks = vec![ContentBlock::Text {
            text: "Use Bearer eyJhbGciOiJSUzI1NiJ9.payload.signature for auth".to_string(),
        }];
        let md = renderer.render(&session);
        assert!(!md.contains("eyJhbGciOiJSUzI1NiJ9"));
        assert!(md.contains("[REDACTED]"));
    }
}
