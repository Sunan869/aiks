/// Renders a KnowledgeDocument to SiYuan Markdown (spec §21-22).
use chrono::Utc;

use crate::ai::schema::{KnowledgeCategory, KnowledgeDocument};
use crate::model::SourceKind;

pub struct KnowledgeRenderer;

impl KnowledgeRenderer {
    /// Render a KnowledgeDocument to Markdown for SiYuan.
    pub fn render(
        doc: &KnowledgeDocument,
        source: SourceKind,
        session_id: &str,
        raw_session_siyuan_id: Option<&str>,
    ) -> String {
        let cat = KnowledgeCategory::from_str(&doc.category);
        let category_name = cat.display_name();
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let mut md = String::with_capacity(2048);

        // Title
        md.push_str(&format!("# {}\n\n", doc.title));

        // Metadata block
        md.push_str(&format!(
            "> 来源：{}  \n> 类型：{}  \n> 日期：{}\n",
            source.display_name(),
            category_name,
            date,
        ));
        if let Some(project) = &doc.project {
            md.push_str(&format!("> 项目：{}  \n", project));
        }
        md.push_str(&format!("> 知识评分：{:.2}  \n", doc.knowledge_score));
        if !doc.tags.is_empty() {
            md.push_str(&format!("> 标签：{}\n", doc.tags.join("、")));
        }
        md.push('\n');

        // Summary section
        md.push_str("## AI 摘要\n\n");
        md.push_str(&doc.summary);
        md.push_str("\n\n");

        // Category-specific sections
        let category = KnowledgeCategory::from_str(&doc.category);
        match category {
            KnowledgeCategory::Troubleshooting => {
                render_section(&mut md, "## 问题", doc.problem.as_deref());
                render_list(&mut md, "## 现象", doc.symptoms.as_deref().unwrap_or(&[]));
                render_list(
                    &mut md,
                    "## 根因",
                    doc.root_causes.as_deref().unwrap_or(&[]),
                );
                render_list(
                    &mut md,
                    "## 解决方案",
                    doc.solutions.as_deref().unwrap_or(&[]),
                );
            }
            KnowledgeCategory::Implementation | KnowledgeCategory::General => {
                render_list(
                    &mut md,
                    "## 解决方案",
                    doc.solutions.as_deref().unwrap_or(&[]),
                );
                render_list(
                    &mut md,
                    "## 设计决策",
                    doc.decisions.as_deref().unwrap_or(&[]),
                );
            }
            KnowledgeCategory::Design => {
                render_section(&mut md, "## 背景与目标", doc.problem.as_deref());
                render_list(
                    &mut md,
                    "## 关键设计决策",
                    doc.decisions.as_deref().unwrap_or(&[]),
                );
                render_list(
                    &mut md,
                    "## 方案要点",
                    doc.solutions.as_deref().unwrap_or(&[]),
                );
            }
            KnowledgeCategory::Research => {
                render_section(&mut md, "## 研究问题", doc.problem.as_deref());
                render_list(&mut md, "## 结论", doc.solutions.as_deref().unwrap_or(&[]));
            }
            KnowledgeCategory::Decision => {
                render_section(&mut md, "## 决策背景", doc.problem.as_deref());
                render_list(
                    &mut md,
                    "## 决策内容",
                    doc.decisions.as_deref().unwrap_or(&[]),
                );
                render_list(&mut md, "## 理由", doc.solutions.as_deref().unwrap_or(&[]));
            }
        }

        // Common sections for all categories
        render_list(
            &mut md,
            "## 关键命令",
            doc.key_commands.as_deref().unwrap_or(&[]),
        );
        render_list(
            &mut md,
            "## 关键文件",
            doc.key_files.as_deref().unwrap_or(&[]),
        );
        if let Some(todos) = &doc.todos {
            if !todos.is_empty() {
                render_list(&mut md, "## 待办事项", todos);
            }
        }

        // User editable section
        md.push_str("## 补充说明\n\n*（可在此处添加个人补充内容）*\n\n");

        // Link back to raw session
        md.push_str("---\n\n");
        if let Some(_raw_id) = raw_session_siyuan_id {
            md.push_str(&format!(
                "原始会话：{{{{原始对话 - {}}}}}  \nSession ID: `{}`\n",
                source.display_name(),
                session_id
            ));
        } else {
            md.push_str(&format!("原始会话 Session ID: `{}`\n", session_id));
        }

        md
    }

    /// Build the SiYuan document path for a knowledge document.
    pub fn build_doc_path(
        doc: &KnowledgeDocument,
        _source: SourceKind,
        session_id: &str,
    ) -> String {
        let cat = KnowledgeCategory::from_str(&doc.category);
        let category = cat.display_name();
        let project = doc.project.as_deref().unwrap_or("General");
        let short_id: String = session_id.chars().take(8).collect();
        let title = doc
            .title
            .chars()
            .take(40)
            .collect::<String>()
            .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-");

        let date = Utc::now().format("%Y-%m-%d").to_string();
        format!(
            "/20 Knowledge/{}/{}/{} {} [{}]",
            project, category, date, title, short_id
        )
    }
}

fn render_section(md: &mut String, heading: &str, content: Option<&str>) {
    if let Some(c) = content {
        if !c.is_empty() {
            md.push_str(&format!("{}\n\n{}\n\n", heading, c));
        }
    }
}

fn render_list(md: &mut String, heading: &str, items: &[String]) {
    if !items.is_empty() {
        md.push_str(&format!("{}\n\n", heading));
        for item in items {
            md.push_str(&format!("- {}\n", item));
        }
        md.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::schema::KnowledgeDocument;

    fn sample_doc() -> KnowledgeDocument {
        KnowledgeDocument {
            worth_extracting: true,
            knowledge_score: 0.85,
            title: "测试知识文档".to_string(),
            summary: "这是一个测试摘要。".to_string(),
            project: Some("TestProject".to_string()),
            category: "troubleshooting".to_string(),
            tags: vec!["Rust".to_string(), "Tauri".to_string()],
            problem: Some("构建失败".to_string()),
            symptoms: Some(vec!["编译错误".to_string()]),
            root_causes: Some(vec!["类型不匹配".to_string()]),
            solutions: Some(vec!["修复类型注解".to_string()]),
            decisions: None,
            key_commands: Some(vec!["cargo build".to_string()]),
            key_files: Some(vec!["src/main.rs".to_string()]),
            todos: None,
            confidence: 0.9,
        }
    }

    #[test]
    fn renders_markdown() {
        let doc = sample_doc();
        let md = KnowledgeRenderer::render(&doc, SourceKind::OpenCode, "sess-123", None);
        assert!(md.contains("# 测试知识文档"));
        assert!(md.contains("## 问题"));
        assert!(md.contains("构建失败"));
        assert!(md.contains("修复类型注解"));
        assert!(md.contains("cargo build"));
    }
}
