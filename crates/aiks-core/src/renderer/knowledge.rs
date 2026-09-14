/// V3 knowledge item → SiYuan markdown renderer.
///
/// Renders a `knowledge_item` row (the V3 pipeline output) as a knowledge
/// document whose SiYuan tree is knowledge-first: category directories with
/// one doc per distilled item, each linking back to the raw session doc.
use std::fmt::Write;

/// Category display names — kept in sync with the desktop UI labels
/// (apps/aiks-desktop/src/pages/KnowledgeDetailPage.tsx CATEGORY_LABELS).
pub fn category_display_name(category: &str) -> &'static str {
    match category {
        "troubleshooting" => "故障排查",
        "architecture" => "架构设计",
        "implementation" => "实现方案",
        "configuration" => "配置管理",
        "research" => "技术探索",
        "decision" => "决策记录",
        _ => "通用",
    }
}

/// Data needed to render one knowledge doc. Decoupled from the DB layer so
/// the renderer stays testable and knows nothing about SiYuan.
pub struct KnowledgeItemDoc<'a> {
    pub knowledge_id: &'a str,
    pub title: &'a str,
    pub category: &'a str,
    pub project_name: Option<&'a str>,
    pub summary: &'a str,
    pub content: &'a str,
    /// JSON array string, e.g. `["Rust","Cargo"]`
    pub tags_json: &'a str,
    pub confidence: f64,
    /// Source display name, e.g. "OpenCode"
    pub source_display: &'a str,
    /// Source external session id
    pub session_ext_id: &'a str,
    /// Source session title
    pub session_title: Option<&'a str>,
    /// SiYuan doc ID of the raw session doc, if it has been synced.
    /// When present, a clickable siyuan:// deep link is rendered.
    pub session_doc_id: Option<&'a str>,
}

/// Render the knowledge doc markdown (this is also the content hash input).
pub fn render_knowledge_item_md(k: &KnowledgeItemDoc<'_>) -> String {
    let mut md = String::with_capacity(2048);

    let _ = writeln!(md, "# {}\n", k.title);

    // Metadata blockquote. The block reference to the raw session doc sits on
    // the FIRST line so the jump is always one click away without scrolling.
    // Native SiYuan block refs (`((id "label"))`) are clickable in both the
    // desktop app and the browser UI — unlike `siyuan://` protocol links,
    // which only resolve when an OS protocol handler is registered.
    let cat_label = category_display_name(k.category);
    if let Some(doc_id) = k.session_doc_id {
        let _ = writeln!(md, "> 原始工作记录：{}  ", block_ref(doc_id, "点击查看原始 Session"));
    }
    let _ = write!(
        md,
        "> 来源：{}  \n> 类型：{}  \n> 置信度：{:.0}%\n",
        k.source_display,
        cat_label,
        k.confidence * 100.0
    );
    if let Some(project) = k.project_name {
        let _ = writeln!(md, "> 项目：{}  ", project);
    }
    if let Ok(tags) = serde_json::from_str::<Vec<String>>(k.tags_json) {
        if !tags.is_empty() {
            let _ = writeln!(md, "> 标签：{}  ", tags.join("、"));
        }
    }
    md.push('\n');

    // Summary
    md.push_str("## 摘要\n\n");
    md.push_str(k.summary);
    md.push_str("\n\n");

    // Detailed content (AI-distilled, already markdown-ish)
    md.push_str("## 详细内容\n\n");
    md.push_str(k.content.trim_end());
    md.push_str("\n\n");

    // Source tracing — provenance footer (the clickable block ref is at the
    // top; here we keep the human-readable title and the raw session id).
    md.push_str("---\n\n");
    if let Some(title) = k.session_title {
        let _ = writeln!(md, "原始工作记录：{}  ", title);
    }
    let _ = writeln!(md, "Session ID: `{}`", k.session_ext_id);

    md
}

/// SiYuan native block reference — single click jumps to the target block
/// (works in desktop and browser UI) and registers in the backlink panel.
fn block_ref(doc_id: &str, label: &str) -> String {
    // Ref labels are quoted inside `((id "label"))` — neutralize embedded quotes.
    let safe_label = label.replace('"', "'");
    format!("(({} \"{}\"))", doc_id, safe_label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> KnowledgeItemDoc<'static> {
        KnowledgeItemDoc {
            knowledge_id: "k12345678-xxxx",
            title: "PowerShell stderr 误报修复",
            category: "troubleshooting",
            project_name: Some("AIKS"),
            summary: "ErrorActionPreference=Stop 导致正常命令误报失败。",
            content: "## 问题\n\n构建误报失败。\n\n## 解决方案\n\n临时切回 Continue。",
            tags_json: r#"["PowerShell","Cargo"]"#,
            confidence: 0.92,
            source_display: "OpenCode",
            session_ext_id: "ses_f717xxxx",
            session_title: Some("AIKS Build Pipeline 修复"),
            session_doc_id: Some("20260914-doc-id"),
        }
    }

    #[test]
    fn renders_with_session_block_ref() {
        let md = render_knowledge_item_md(&sample());
        assert!(md.contains("# PowerShell stderr 误报修复"));
        assert!(md.contains("## 摘要"));
        assert!(md.contains("## 详细内容"));
        // Native block ref on the first metadata line (quick jump without
        // scrolling) — no siyuan:// protocol link anywhere.
        assert!(md.contains("> 原始工作记录：((20260914-doc-id \"点击查看原始 Session\"))"));
        assert!(!md.contains("siyuan://"), "protocol links are not clickable in the browser UI");
        // Provenance footer keeps title + raw session id as plain text.
        assert!(md.contains("原始工作记录：AIKS Build Pipeline 修复"));
        assert!(md.contains("Session ID: `ses_f717xxxx`"));
        assert!(md.contains("故障排查"));
        assert!(md.contains("92%"));
        assert!(md.contains("PowerShell、Cargo"));
    }

    #[test]
    fn renders_without_session_link() {
        let mut k = sample();
        k.session_doc_id = None;
        let md = render_knowledge_item_md(&k);
        assert!(!md.contains("(("), "no dead block ref allowed");
        assert!(!md.contains("siyuan://"), "no dead link allowed");
        assert!(md.contains("Session ID: `ses_f717xxxx`"));
        assert!(md.contains("原始工作记录：AIKS Build Pipeline 修复"));
    }

    #[test]
    fn block_ref_neutralizes_quotes_in_label() {
        assert_eq!(
            block_ref("20260914-x", "a \"b\" c"),
            "((20260914-x \"a 'b' c\"))"
        );
    }

    #[test]
    fn category_names_match_ui() {
        assert_eq!(category_display_name("implementation"), "实现方案");
        assert_eq!(category_display_name("decision"), "决策记录");
        assert_eq!(category_display_name("unknown-cat"), "通用");
    }
}
