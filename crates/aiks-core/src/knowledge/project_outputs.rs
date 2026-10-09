//! Editable, evidence-linked local work reviews and read-only agent context packs.
//! These functions never write to AGENTS.md, CLAUDE.md or provider project files.
use crate::knowledge::project_memory::{ProjectMemoryEntry, ProjectMemorySnapshot};
use crate::util::SecretSanitizer;
use anyhow::{bail, Result};
use chrono::NaiveDate;

fn safe_inline(text: &str) -> String {
    let sanitizer = SecretSanitizer::new();
    sanitizer.sanitize(text).replace(['\r', '\n'], " ")
}

fn cite(item: &ProjectMemoryEntry) -> String {
    format!(
        "知识 {} · Session {} ({})",
        item.knowledge_id, item.session_external_id, item.source
    )
}

pub fn render_project_review(
    snapshot: &ProjectMemorySnapshot,
    from: &str,
    through: &str,
) -> Result<String> {
    let from_date = NaiveDate::parse_from_str(from, "%Y-%m-%d")?;
    let through_date = NaiveDate::parse_from_str(through, "%Y-%m-%d")?;
    if from_date > through_date {
        bail!("Report start date must not be after the end date");
    }
    let title = safe_inline(&snapshot.project.title);
    let mut decision = Vec::new();
    let mut solutions = Vec::new();
    let mut other = Vec::new();
    for item in &snapshot.entries {
        let Some(date) = item.updated_at.get(..10) else {
            continue;
        };
        if date < from || date > through {
            continue;
        }
        let line = format!(
            "- **{}**：{}（{}；更新 {}）",
            safe_inline(&item.title),
            safe_inline(&item.summary),
            cite(item),
            date
        );
        match item.category.as_str() {
            "decision" | "architecture" => decision.push(line),
            "solution" | "troubleshooting" => solutions.push(line),
            _ => other.push(line),
        }
    }
    let section = |header: &str, rows: &[String]| -> String {
        if rows.is_empty() {
            format!("## {header}\n\n没有对应的可核实记录。\n\n")
        } else {
            format!("## {header}\n\n{}\n\n", rows.join("\n"))
        }
    };
    let mut result = format!(
        "# {} · 工作回顾\n\n期间：{} 至 {}\n\n说明：仅依据已采集的工作 Session 与知识，不把计划、建议或猜测误写成已完成的事实。\n\n",
        title, from, through
    );
    result.push_str(&section("关键决策记录", &decision));
    result.push_str(&section("问题与解决方案记录", &solutions));
    result.push_str(&section("其他知识与风险线索", &other));
    if snapshot.truncated {
        result.push_str("注意：仅包含当前加载的知识记录，需扩大检索范围才能形成完整报告。\n");
    }
    Ok(result)
}

/// This is a conservative *character* budget, not an exact tokenizer count.
/// The caller reviews the text and explicitly chooses whether to copy it.
pub fn render_agent_context(snapshot: &ProjectMemorySnapshot, max_tokens: usize) -> String {
    let sanitizer = SecretSanitizer::new();
    let max_chars = max_tokens.clamp(256, 8192).saturating_mul(3);
    let title = safe_inline(&snapshot.project.title);
    let mut result = format!(
        "# {} · AIKS 项目背景候选\n\n以下内容供人工审核后复制给 Agent；不能自动写入 AGENTS.md 或 CLAUDE.md。\n只有来源明确的记录，不保证所有信息仍然生效。\n\n",
        title
    );
    let mut included = 0usize;
    let mut omitted = 0usize;
    for item in &snapshot.entries {
        if matches!(
            item.feedback_status.as_deref(),
            Some("incorrect" | "outdated")
        ) {
            omitted += 1;
            continue;
        }
        let block = format!(
            "## {}\n类别：{}\n{}\n来源：{}\n更新：{}\n\n",
            safe_inline(&item.title),
            safe_inline(&item.category),
            sanitizer.sanitize(&item.summary),
            cite(item),
            item.updated_at
        );
        if result.chars().count() + block.chars().count() > max_chars {
            omitted += 1;
            continue;
        }
        result.push_str(&block);
        included += 1;
    }
    let footer = format!(
        "\nAIKS 注：已收录 {included} 条，因质量标记或预算排除 {omitted} 条。请核对原 Session 证据后采用。\n"
    );
    if result.chars().count() + footer.chars().count() <= max_chars {
        result.push_str(&footer);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::project_memory::{
        ProjectMemoryEntry, ProjectMemorySnapshot, ProjectOverview,
    };

    fn snapshot() -> ProjectMemorySnapshot {
        ProjectMemorySnapshot {
            project: ProjectOverview {
                id: "test".into(),
                title: "Demo".into(),
                verified_path: true,
                session_count: 1,
                knowledge_count: 2,
                sources: vec!["codex".into()],
                last_updated_at: "2026-10-09".into(),
            },
            entries: vec![
                ProjectMemoryEntry {
                    knowledge_id: "k1".into(),
                    session_id: 1,
                    source: "codex".into(),
                    session_external_id: "s1".into(),
                    title: "Decision".into(),
                    category: "decision".into(),
                    summary: "Use a bounded queue".into(),
                    updated_at: "2026-10-09".into(),
                    feedback_status: None,
                },
                ProjectMemoryEntry {
                    knowledge_id: "k2".into(),
                    session_id: 1,
                    source: "codex".into(),
                    session_external_id: "s1".into(),
                    title: "Old fix".into(),
                    category: "solution".into(),
                    summary: "Deprecated recommendation".into(),
                    updated_at: "2026-10-08".into(),
                    feedback_status: Some("outdated".into()),
                },
            ],
            truncated: false,
        }
    }

    #[test]
    fn reports_are_date_scoped_and_cite_sources() {
        let report = render_project_review(&snapshot(), "2026-10-09", "2026-10-09").unwrap();
        assert!(report.contains("Session s1"));
        assert!(report.contains("bounded queue"));
        assert!(!report.contains("Deprecated recommendation"));
        assert!(render_project_review(&snapshot(), "2026-10-10", "2026-10-09").is_err());
    }

    #[test]
    fn agent_context_rejects_outdated_items_without_writing_any_file() {
        let context = render_agent_context(&snapshot(), 400);
        assert!(context.contains("bounded queue"));
        assert!(!context.contains("Deprecated recommendation"));
        assert!(context.contains("k1"));
    }
}
