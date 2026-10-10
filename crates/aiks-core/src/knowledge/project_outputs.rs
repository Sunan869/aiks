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
        safe_inline(&item.knowledge_id),
        safe_inline(&item.session_external_id),
        safe_inline(&item.source)
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
        // Reports must not present knowledge flagged as incorrect or outdated as verified work.
        if matches!(
            item.feedback_status.as_deref(),
            Some("incorrect" | "outdated")
        ) {
            continue;
        }
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
    // Bound untrusted project titles independently of the requested context budget.
    let title: String = safe_inline(&snapshot.project.title)
        .chars()
        .take(120)
        .collect();
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
        let quality_notice = match item.feedback_status.as_deref() {
            Some("duplicate") => "质量提醒：用户标记为可能重复，需核对来源。\n",
            Some("needs_detail") => "质量提醒：用户标记为需补充，不能当作完整结论。\n",
            _ => "",
        };
        let block = format!(
            "## {}\n类别：{}\n{}{}\n来源：{}\n更新：{}\n\n",
            safe_inline(&item.title),
            safe_inline(&item.category),
            quality_notice,
            sanitizer.sanitize(&item.summary),
            cite(item),
            safe_inline(&item.updated_at)
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
    fn reviewed_reports_exclude_disputed_and_outdated_knowledge() {
        let mut data = snapshot();
        data.entries[1].updated_at = "2026-10-09".into();
        let report = render_project_review(&data, "2026-10-09", "2026-10-09").unwrap();
        assert!(report.contains("bounded queue"));
        assert!(!report.contains("Deprecated recommendation"));
        data.entries[0].feedback_status = Some("incorrect".into());
        let report = render_project_review(&data, "2026-10-09", "2026-10-09").unwrap();
        assert!(!report.contains("bounded queue"));
        assert!(report.contains("没有对应的可核实记录"));
    }

    #[test]
    fn citations_do_not_allow_multiline_metadata_to_inject_markdown() {
        let mut data = snapshot();
        data.entries[0].session_external_id = "s1\n## Forged heading".into();
        data.entries[0].source = "codex\r\n- forged source".into();
        data.entries[0].knowledge_id = "k1\n## Forged knowledge".into();
        let report = render_project_review(&data, "2026-10-09", "2026-10-09").unwrap();
        let context = render_agent_context(&data, 400);
        for output in [&report, &context] {
            assert!(!output.contains("\n## Forged"));
            assert!(!output.contains("\n- forged"));
            assert!(output.contains("Session s1"));
        }
    }

    #[test]
    fn agent_context_sanitizes_timestamps_and_respects_character_budget() {
        let mut data = snapshot();
        data.entries[0].updated_at = "2026-10-09\n## Forged timestamp".into();
        let context = render_agent_context(&data, 256);
        assert!(!context.contains("\n## Forged timestamp"));
        assert!(context.chars().count() <= 256 * 3);
    }

    #[test]
    fn very_long_project_title_cannot_exceed_minimum_agent_budget() {
        let mut data = snapshot();
        data.project.title = "A".repeat(100_000);
        let context = render_agent_context(&data, 256);
        assert!(context.chars().count() <= 256 * 3);
        assert!(!context.contains(&"A".repeat(121)));
    }

    #[test]
    fn agent_context_flags_incomplete_and_duplicate_knowledge() {
        let mut data = snapshot();
        data.entries[0].feedback_status = Some("needs_detail".into());
        data.entries[1].feedback_status = Some("duplicate".into());
        let context = render_agent_context(&data, 2000);
        assert!(context.contains("需补充，不能当作完整结论"));
        assert!(context.contains("可能重复，需核对来源"));
        assert!(context.contains("Deprecated recommendation"));
    }

    #[test]
    fn agent_context_rejects_outdated_items_without_writing_any_file() {
        let context = render_agent_context(&snapshot(), 400);
        assert!(context.contains("bounded queue"));
        assert!(!context.contains("Deprecated recommendation"));
        assert!(context.contains("k1"));
    }
}
