use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ai::ModelService;
use crate::util::SecretSanitizer;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiAssistOperation {
    Summary,
    Tags,
    Category,
    Title,
    KeyConclusions,
    Structure,
    Rewrite,
    Compare,
    MergeDraft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAssistRequest {
    pub operation: AiAssistOperation,
    pub title: String,
    pub content: String,
    pub existing_summary: Option<String>,
    #[serde(default)]
    pub existing_tags: Vec<String>,
    pub existing_category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiAssistSuggestion {
    pub operation: AiAssistOperation,
    pub title: Option<String>,
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelSuggestion {
    title: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    category: Option<String>,
    text: Option<String>,
}

pub struct AiAssistService {
    models: Arc<ModelService>,
}

impl AiAssistService {
    pub fn new(models: Arc<ModelService>) -> Self {
        Self { models }
    }

    pub async fn suggest(&self, request: AiAssistRequest) -> anyhow::Result<AiAssistSuggestion> {
        validate_request(&request)?;
        let (system, user) = build_prompt(&request)?;
        let model_output: ModelSuggestion = self.models.complete_json(&system, &user).await?;
        let suggestion = normalize_suggestion(request.operation.clone(), model_output);
        validate_suggestion(&suggestion)?;
        Ok(suggestion)
    }
}

fn validate_request(request: &AiAssistRequest) -> anyhow::Result<()> {
    if request.content.trim().is_empty() {
        anyhow::bail!("AI Assist requires canonical document content");
    }
    if request.content.len() > 1_000_000 {
        anyhow::bail!("AI Assist input exceeds the 1 MiB safety budget");
    }
    if request.title.trim().is_empty() && request.operation != AiAssistOperation::Title {
        anyhow::bail!("AI Assist requires a document title");
    }
    Ok(())
}

fn build_prompt(request: &AiAssistRequest) -> anyhow::Result<(String, String)> {
    let instruction = match request.operation {
        AiAssistOperation::Summary => "生成忠于原文、简洁且可独立理解的摘要。只填写 summary。",
        AiAssistOperation::Tags => {
            "生成 3-8 个高信息密度标签，避免同义重复和过宽泛标签。只填写 tags。"
        }
        AiAssistOperation::Category => {
            "选择一个稳定、简短、适合知识库筛选的分类名。只填写 category。"
        }
        AiAssistOperation::Title => {
            "生成准确具体的标题，不夸张、不添加原文不存在的结论。只填写 title。"
        }
        AiAssistOperation::KeyConclusions => {
            "提取关键结论、决定、约束和可执行结果，使用清晰 Markdown。只填写 text。"
        }
        AiAssistOperation::Structure => {
            "在不改变事实和结论的前提下，给出更清晰的 Markdown 结构化版本。只填写 text。"
        }
        AiAssistOperation::Rewrite => {
            "在保持事实、代码、数字、路径和技术含义不变的前提下润色全文。只填写 text。"
        }
        AiAssistOperation::Compare => {
            "比较输入中明确标识的多份知识来源，列出一致点、不同点、适用条件和各自证据。不得把矛盾内容强行合并，使用清晰 Markdown，只填写 text。"
        }
        AiAssistOperation::MergeDraft => {
            "基于输入中多份明确标识的知识来源生成新的 Markdown 整理草稿，保留来源证据、差异和冲突说明，不得臆造验证结果或删除原始信息。只填写 text。"
        }
    };

    let system = format!(
        "你是 AIKS 知识助手。{instruction}\n\
         你只能返回一个 JSON 对象，禁止 Markdown 代码块和额外说明。\n\
         JSON schema: {{\"title\":string|null,\"summary\":string|null,\"tags\":[string],\"category\":string|null,\"text\":string|null}}。\n\
         未要求的字段必须返回 null 或空数组。不得编造输入中不存在的事实。"
    );

    // AI Assist may target a user-configured remote model. Sanitize every
    // user-controlled field at the external-send boundary, while keeping the
    // original knowledge and the reviewed local draft unchanged.
    let sanitizer = SecretSanitizer::new();
    let context = serde_json::json!({
        "title": sanitizer.sanitize(&request.title),
        "content": sanitizer.sanitize(&request.content),
        "existing_summary": request.existing_summary.as_deref().map(|value| sanitizer.sanitize(value)),
        "existing_tags": request.existing_tags.iter().map(|value| sanitizer.sanitize(value)).collect::<Vec<_>>(),
        "existing_category": request.existing_category.as_deref().map(|value| sanitizer.sanitize(value)),
    });
    let user = format!(
        "请处理下面的知识文档上下文：\n{}",
        serde_json::to_string_pretty(&context)?
    );
    Ok((system, user))
}

fn normalize_suggestion(
    operation: AiAssistOperation,
    output: ModelSuggestion,
) -> AiAssistSuggestion {
    AiAssistSuggestion {
        operation,
        title: trim_option(output.title),
        summary: trim_option(output.summary),
        tags: normalize_tags(output.tags),
        category: trim_option(output.category),
        text: trim_option(output.text),
    }
}

fn trim_option(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.into_iter()
        .filter_map(|tag| {
            let value = tag.trim();
            if value.is_empty() {
                return None;
            }
            let key = value.to_lowercase();
            seen.insert(key).then(|| value.to_string())
        })
        .take(12)
        .collect()
}

fn validate_suggestion(suggestion: &AiAssistSuggestion) -> anyhow::Result<()> {
    let valid = match suggestion.operation {
        AiAssistOperation::Summary => suggestion.summary.is_some(),
        AiAssistOperation::Tags => !suggestion.tags.is_empty(),
        AiAssistOperation::Category => suggestion.category.is_some(),
        AiAssistOperation::Title => suggestion.title.is_some(),
        AiAssistOperation::KeyConclusions
        | AiAssistOperation::Structure
        | AiAssistOperation::Rewrite
        | AiAssistOperation::Compare
        | AiAssistOperation::MergeDraft => suggestion.text.is_some(),
    };
    if !valid {
        anyhow::bail!(
            "AI Assist returned an empty suggestion for {:?}",
            suggestion.operation
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_names_use_stable_snake_case_transport() {
        assert_eq!(
            serde_json::to_string(&AiAssistOperation::KeyConclusions).unwrap(),
            "\"key_conclusions\""
        );
        assert_eq!(
            serde_json::from_str::<AiAssistOperation>("\"rewrite\"").unwrap(),
            AiAssistOperation::Rewrite
        );
    }

    #[test]
    fn empty_operation_output_is_rejected() {
        let suggestion = AiAssistSuggestion {
            operation: AiAssistOperation::Summary,
            title: None,
            summary: None,
            tags: vec![],
            category: None,
            text: None,
        };
        assert!(validate_suggestion(&suggestion).is_err());
    }

    #[test]
    fn tags_are_trimmed_deduplicated_and_bounded() {
        let tags = normalize_tags(vec![
            " Rust ".into(),
            "rust".into(),
            "SQLite".into(),
            "".into(),
        ]);
        assert_eq!(tags, vec!["Rust", "SQLite"]);
    }

    #[test]
    fn multi_source_operations_are_draft_only_and_require_text() {
        for operation in [AiAssistOperation::Compare, AiAssistOperation::MergeDraft] {
            let request = AiAssistRequest {
                operation: operation.clone(),
                title: "跨会话知识".into(),
                content: "来源 A 与来源 B 不同".into(),
                existing_summary: None,
                existing_tags: Vec::new(),
                existing_category: None,
            };
            let (system, _user) = build_prompt(&request).unwrap();
            assert!(system.contains("只填写 text"));
            let output = AiAssistSuggestion {
                operation,
                title: None,
                summary: None,
                tags: vec![],
                category: None,
                text: Some("保留差异与出处".into()),
            };
            validate_suggestion(&output).unwrap();
        }
    }

    #[test]
    fn oversized_organization_context_is_rejected_before_model_request() {
        let request = AiAssistRequest {
            operation: AiAssistOperation::MergeDraft,
            title: "Oversized".into(),
            content: "a".repeat(1_000_001),
            existing_summary: None,
            existing_tags: vec![],
            existing_category: None,
        };
        assert!(validate_request(&request).is_err());
        let mut allowed = request;
        allowed.content = "a".repeat(1_000_000);
        validate_request(&allowed).unwrap();
    }

    #[test]
    fn ai_assist_sanitizes_all_fields_before_sending_to_model() {
        let request = AiAssistRequest {
            operation: AiAssistOperation::MergeDraft,
            title: "API_KEY=titleSecret123".into(),
            content: "Authorization: Bearer contentSecret123456789".into(),
            existing_summary: Some("Bearer summarySecret123456789".into()),
            existing_tags: vec!["token=tagSecret123".into()],
            existing_category: Some("password=categorySecret123".into()),
        };
        let original = request.content.clone();
        let (_system, user) = build_prompt(&request).unwrap();
        for secret in [
            "titleSecret123",
            "contentSecret123456789",
            "summarySecret123456789",
            "tagSecret123",
            "categorySecret123",
        ] {
            assert!(
                !user.contains(secret),
                "secret leaked into AI Assist prompt"
            );
        }
        assert!(user.contains("[REDACTED]"));
        assert_eq!(
            request.content, original,
            "sanitizing must not mutate source knowledge"
        );
    }

    #[test]
    fn prompts_are_deterministic_and_keep_context_as_json() {
        let request = AiAssistRequest {
            operation: AiAssistOperation::Summary,
            title: "V4.2".into(),
            content: "正文".into(),
            existing_summary: None,
            existing_tags: vec!["AIKS".into()],
            existing_category: Some("architecture".into()),
        };
        let (system, user) = build_prompt(&request).unwrap();
        assert!(system.contains("只填写 summary"));
        assert!(user.contains("\"content\": \"正文\""));
        assert!(user.contains("\"AIKS\""));
    }
}
