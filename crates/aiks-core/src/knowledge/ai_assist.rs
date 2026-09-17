use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ai::ModelService;

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
    if request.title.trim().is_empty() && request.operation != AiAssistOperation::Title {
        anyhow::bail!("AI Assist requires a document title");
    }
    Ok(())
}

fn build_prompt(request: &AiAssistRequest) -> anyhow::Result<(String, String)> {
    let instruction = match request.operation {
        AiAssistOperation::Summary => {
            "生成忠于原文、简洁且可独立理解的摘要。只填写 summary。"
        }
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
    };

    let system = format!(
        "你是 AIKS 知识助手。{instruction}\n\
         你只能返回一个 JSON 对象，禁止 Markdown 代码块和额外说明。\n\
         JSON schema: {{\"title\":string|null,\"summary\":string|null,\"tags\":[string],\"category\":string|null,\"text\":string|null}}。\n\
         未要求的字段必须返回 null 或空数组。不得编造输入中不存在的事实。"
    );

    let context = serde_json::json!({
        "title": request.title,
        "content": request.content,
        "existing_summary": request.existing_summary,
        "existing_tags": request.existing_tags,
        "existing_category": request.existing_category,
    });
    let user = format!("请处理下面的知识文档上下文：\n{}", serde_json::to_string_pretty(&context)?);
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
        | AiAssistOperation::Rewrite => suggestion.text.is_some(),
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
        assert_eq!(serde_json::to_string(&AiAssistOperation::KeyConclusions).unwrap(), "\"key_conclusions\"");
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
