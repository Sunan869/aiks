use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::Utc;
use reqwest::{Client, Url};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind};

pub const SHARE_PARSER_VERSION: &str = "share-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareImportResult {
    pub source: SourceKind,
    pub external_session_id: String,
    pub title: Option<String>,
    pub message_count: usize,
    pub source_url: String,
    pub cache_path: PathBuf,
}

pub struct ShareImportService {
    http: Client,
    cache_root: PathBuf,
}

impl ShareImportService {
    pub fn new(cache_root: PathBuf) -> anyhow::Result<Self> {
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/153 Safari/537.36")
            .build()?;
        Ok(Self { http, cache_root })
    }

    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    pub async fn fetch_and_cache(&self, share_url: &str) -> anyhow::Result<(NormalizedSession, ShareImportResult)> {
        let requested = Url::parse(share_url.trim()).context("invalid share URL")?;
        let initial_source = detect_share_source(&requested)
            .ok_or_else(|| anyhow::anyhow!("Unsupported share URL. Supported: ChatGPT, Claude, Gemini"))?;

        let response = self
            .http
            .get(requested.clone())
            .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
            .send()
            .await
            .context("fetch shared conversation")?;

        let status = response.status();
        let final_url = response.url().clone();
        if !status.is_success() {
            anyhow::bail!("Share page returned HTTP {status}");
        }

        let source = detect_share_source(&final_url).unwrap_or(initial_source);
        anyhow::ensure!(
            source == initial_source || initial_source == SourceKind::GeminiShare,
            "Share URL redirected to an unexpected provider"
        );

        let html = response.text().await.context("read share page")?;
        let parsed = parse_share_html(source, &html)?;
        anyhow::ensure!(
            !parsed.messages.is_empty(),
            provider_empty_page_error(source)
        );

        let canonical_url = final_url.to_string();
        let external_session_id = share_external_id(source, &final_url);
        let now = Utc::now();
        let mut metadata = HashMap::new();
        metadata.insert("share_url".to_string(), serde_json::json!(canonical_url));
        metadata.insert("imported_at".to_string(), serde_json::json!(now.to_rfc3339()));
        metadata.insert(
            "share_provider".to_string(),
            serde_json::json!(source.display_name()),
        );

        let mut session = NormalizedSession {
            source,
            external_session_id: external_session_id.clone(),
            title: parsed.title,
            project_name: Some(format!("{} Web", source.display_name())),
            project_path: None,
            source_path: None,
            started_at: None,
            updated_at: Some(now),
            model: None,
            messages: parsed.messages,
            usage: None,
            metadata,
        };

        let cache_path = self.cache_path(source, &external_session_id);
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create share cache dir: {}", parent.display()))?;
        }
        session.source_path = Some(cache_path.clone());

        let serialized = serde_json::to_vec_pretty(&session)?;
        let temp_path = cache_path.with_extension("json.tmp");
        fs::write(&temp_path, serialized)
            .with_context(|| format!("write share cache: {}", temp_path.display()))?;
        if cache_path.exists() {
            fs::remove_file(&cache_path).ok();
        }
        fs::rename(&temp_path, &cache_path)
            .with_context(|| format!("commit share cache: {}", cache_path.display()))?;

        let result = ShareImportResult {
            source,
            external_session_id,
            title: session.title.clone(),
            message_count: session.messages.len(),
            source_url: canonical_url,
            cache_path,
        };
        Ok((session, result))
    }

    fn cache_path(&self, source: SourceKind, external_session_id: &str) -> PathBuf {
        self.cache_root
            .join(source.as_str())
            .join(format!("{external_session_id}.json"))
    }
}

#[derive(Debug)]
struct ParsedShare {
    title: Option<String>,
    messages: Vec<NormalizedMessage>,
}

fn parse_share_html(source: SourceKind, html: &str) -> anyhow::Result<ParsedShare> {
    let document = Html::parse_document(html);
    let mut messages = match source {
        SourceKind::ChatgptShare => extract_chatgpt_dom(&document),
        SourceKind::ClaudeShare => extract_claude_dom(&document),
        SourceKind::GeminiShare => extract_gemini_dom(&document),
        _ => Vec::new(),
    };

    if messages.is_empty() {
        let body = document
            .root_element()
            .text()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        messages = match source {
            SourceKind::ChatgptShare => extract_labeled_transcript(
                &body,
                &["You said", "You said:"],
                &["ChatGPT said", "ChatGPT said:"],
            ),
            SourceKind::ClaudeShare => extract_labeled_transcript(
                &body,
                &["You said", "You said:", "Human:"],
                &["Claude responded", "Claude responded:", "Claude:"],
            ),
            SourceKind::GeminiShare => extract_labeled_transcript(
                &body,
                &["You said", "You said:"],
                &["Gemini said", "Gemini said:"],
            ),
            _ => Vec::new(),
        };
    }

    let title = extract_title(source, &document);
    Ok(ParsedShare { title, messages })
}

fn extract_chatgpt_dom(document: &Html) -> Vec<NormalizedMessage> {
    let Ok(selector) = Selector::parse("[data-message-author-role]") else {
        return Vec::new();
    };
    document
        .select(&selector)
        .filter_map(|node| {
            let role = match node.value().attr("data-message-author-role")? {
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                _ => return None,
            };
            let text = clean_node_text(node.text());
            (!text.is_empty()).then_some((role, text))
        })
        .enumerate()
        .map(|(index, (role, text))| normalized_message(index, role, text))
        .collect()
}

fn extract_claude_dom(document: &Html) -> Vec<NormalizedMessage> {
    let Ok(selector) = Selector::parse("[data-testid=\"user-message\"], .font-claude-response") else {
        return Vec::new();
    };
    document
        .select(&selector)
        .filter_map(|node| {
            let role = if node.value().attr("data-testid") == Some("user-message") {
                MessageRole::User
            } else {
                MessageRole::Assistant
            };
            let text = clean_node_text(node.text());
            (!text.is_empty()).then_some((role, text))
        })
        .enumerate()
        .map(|(index, (role, text))| normalized_message(index, role, text))
        .collect()
}

fn extract_gemini_dom(document: &Html) -> Vec<NormalizedMessage> {
    for selector_text in [
        "[data-test-id=\"user-query\"], [data-test-id=\"model-response\"]",
        "[data-testid=\"user-query\"], [data-testid=\"model-response\"]",
    ] {
        let Ok(selector) = Selector::parse(selector_text) else {
            continue;
        };
        let messages: Vec<_> = document
            .select(&selector)
            .filter_map(|node| {
                let attr = node
                    .value()
                    .attr("data-test-id")
                    .or_else(|| node.value().attr("data-testid"))
                    .unwrap_or_default();
                let role = if attr.contains("user") {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                };
                let text = clean_node_text(node.text());
                (!text.is_empty()).then_some((role, text))
            })
            .enumerate()
            .map(|(index, (role, text))| normalized_message(index, role, text))
            .collect();
        if !messages.is_empty() {
            return messages;
        }
    }
    Vec::new()
}

fn extract_labeled_transcript(
    body: &str,
    user_labels: &[&str],
    assistant_labels: &[&str],
) -> Vec<NormalizedMessage> {
    let mut turns: Vec<(MessageRole, String)> = Vec::new();
    let mut current_role: Option<MessageRole> = None;
    let mut current = Vec::new();

    let flush = |role: &mut Option<MessageRole>, lines: &mut Vec<String>, turns: &mut Vec<(MessageRole, String)>| {
        if let Some(value) = role.take() {
            let text = lines.join("\n").trim().to_string();
            if !text.is_empty() {
                turns.push((value, text));
            }
        }
        lines.clear();
    };

    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if user_labels.iter().any(|label| line == *label) {
            flush(&mut current_role, &mut current, &mut turns);
            current_role = Some(MessageRole::User);
            continue;
        }
        if assistant_labels.iter().any(|label| line == *label) {
            flush(&mut current_role, &mut current, &mut turns);
            current_role = Some(MessageRole::Assistant);
            continue;
        }
        if current_role.is_some() {
            current.push(line.to_string());
        }
    }
    flush(&mut current_role, &mut current, &mut turns);

    turns
        .into_iter()
        .enumerate()
        .map(|(index, (role, text))| normalized_message(index, role, text))
        .collect()
}

fn normalized_message(index: usize, role: MessageRole, text: String) -> NormalizedMessage {
    NormalizedMessage {
        external_id: format!("share-turn-{index}"),
        parent_id: (index > 0).then(|| format!("share-turn-{}", index - 1)),
        role,
        created_at: None,
        model: None,
        blocks: vec![ContentBlock::Text { text }],
        usage: None,
        metadata: HashMap::new(),
    }
}

fn clean_node_text<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    let mut out = Vec::new();
    for part in parts {
        let value = part.trim();
        if !value.is_empty() && out.last().is_none_or(|last: &String| last != value) {
            out.push(value.to_string());
        }
    }
    out.join("\n")
}

fn extract_title(source: SourceKind, document: &Html) -> Option<String> {
    let preferred = match source {
        SourceKind::ClaudeShare => vec!["[data-testid=\"page-header\"]", "h1"],
        SourceKind::ChatgptShare | SourceKind::GeminiShare => vec!["h1", "title"],
        _ => vec!["title"],
    };
    for selector_text in preferred {
        let Ok(selector) = Selector::parse(selector_text) else {
            continue;
        };
        if let Some(node) = document.select(&selector).next() {
            let text = clean_node_text(node.text());
            let text = text
                .trim()
                .trim_start_matches("ChatGPT - ")
                .trim_end_matches(" - Google Gemini")
                .trim();
            if !text.is_empty()
                && !matches!(text, "ChatGPT" | "Claude" | "Google Gemini" | "Gemini")
            {
                return Some(text.chars().take(200).collect());
            }
        }
    }
    None
}

pub fn detect_share_source(url: &Url) -> Option<SourceKind> {
    let host = url.host_str()?.trim_start_matches("www.").to_ascii_lowercase();
    let path = url.path();
    if host == "chatgpt.com" && path.starts_with("/share/") {
        Some(SourceKind::ChatgptShare)
    } else if host == "claude.ai" && path.starts_with("/share/") {
        Some(SourceKind::ClaudeShare)
    } else if (host == "g.co" && path.starts_with("/gemini/share/"))
        || (host == "gemini.google.com" && (path.starts_with("/share/") || path.starts_with("/app/")))
    {
        Some(SourceKind::GeminiShare)
    } else {
        None
    }
}

fn share_external_id(source: SourceKind, url: &Url) -> String {
    let segments: Vec<_> = url
        .path_segments()
        .map(|values| values.filter(|value| !value.is_empty()).collect())
        .unwrap_or_default();
    let candidate = segments.last().copied().unwrap_or_default();
    if candidate.len() >= 4
        && candidate
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return candidate.to_string();
    }
    let digest = Sha256::digest(format!("{}:{}", source.as_str(), url).as_bytes());
    hex::encode(&digest[..16])
}

fn provider_empty_page_error(source: SourceKind) -> &'static str {
    match source {
        SourceKind::ClaudeShare => {
            "No Claude conversation content was found. Claude share pages may require a logged-in browser session; this URL cannot currently be imported by the direct fetcher."
        }
        SourceKind::ChatgptShare => {
            "No ChatGPT conversation content was found. The shared link may have been revoked or the page format may have changed."
        }
        SourceKind::GeminiShare => {
            "No Gemini conversation content was found. The shared link may have been revoked or the page format may have changed."
        }
        _ => "No conversation content was found.",
    }
}

pub fn normalized_session_hash(session: &NormalizedSession) -> anyhow::Result<String> {
    let bytes = serde_json::to_vec(session)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_share_urls() {
        assert_eq!(
            detect_share_source(&Url::parse("https://chatgpt.com/share/abc-def").unwrap()),
            Some(SourceKind::ChatgptShare)
        );
        assert_eq!(
            detect_share_source(&Url::parse("https://claude.ai/share/abc-def").unwrap()),
            Some(SourceKind::ClaudeShare)
        );
        assert_eq!(
            detect_share_source(&Url::parse("https://g.co/gemini/share/abc123").unwrap()),
            Some(SourceKind::GeminiShare)
        );
    }

    #[test]
    fn parses_chatgpt_role_dom() {
        let html = r#"<html><body>
          <h1>Shared chat</h1>
          <div data-message-author-role="user"><p>Hello</p></div>
          <div data-message-author-role="assistant"><p>Hi there</p></div>
        </body></html>"#;
        let parsed = parse_share_html(SourceKind::ChatgptShare, html).unwrap();
        assert_eq!(parsed.messages.len(), 2);
        assert_eq!(parsed.messages[0].role, MessageRole::User);
        assert_eq!(parsed.messages[1].role, MessageRole::Assistant);
    }

    #[test]
    fn parses_claude_stable_selectors() {
        let html = r#"<html><body>
          <div data-testid="page-header">Architecture review</div>
          <div data-testid="user-message">Question</div>
          <div class="font-claude-response">Answer</div>
        </body></html>"#;
        let parsed = parse_share_html(SourceKind::ClaudeShare, html).unwrap();
        assert_eq!(parsed.title.as_deref(), Some("Architecture review"));
        assert_eq!(parsed.messages.len(), 2);
    }

    #[test]
    fn labeled_fallback_preserves_turn_order() {
        let text = "Conversation with Gemini\nYou said\nHello\nGemini said\nHi";
        let turns = extract_labeled_transcript(text, &["You said"], &["Gemini said"]);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].role, MessageRole::User);
        assert_eq!(turns[1].role, MessageRole::Assistant);
    }
}
