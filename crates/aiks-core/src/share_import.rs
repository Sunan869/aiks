use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use chrono::{DateTime, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::model::{ContentBlock, MessageRole, NormalizedMessage, NormalizedSession, SourceKind};

pub const SHARE_PARSER_VERSION: &str = "share-v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareAssetInput {
    pub kind: String,
    pub url: String,
    pub name: Option<String>,
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareMessageInput {
    pub external_id: Option<String>,
    pub role: String,
    pub text: String,
    pub created_at: Option<String>,
    #[serde(default)]
    pub assets: Vec<ShareAssetInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareConversationInput {
    pub source: String,
    pub source_url: String,
    pub external_session_id: String,
    pub title: Option<String>,
    pub model: Option<String>,
    pub updated_at: Option<String>,
    pub messages: Vec<ShareMessageInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareImportResult {
    pub source: SourceKind,
    pub external_session_id: String,
    pub title: Option<String>,
    pub message_count: usize,
    pub source_url: String,
    pub cache_path: PathBuf,
    pub content_hash: String,
}

pub fn share_cache_root() -> PathBuf {
    crate::config::data_root().join("imports").join("share")
}

pub fn detect_share_source(raw_url: &str) -> anyhow::Result<SourceKind> {
    let url = Url::parse(raw_url.trim()).context("invalid share URL")?;
    detect_share_source_url(&url).ok_or_else(|| {
        anyhow::anyhow!(
            "Unsupported share URL. Supported: ChatGPT, Claude, Gemini, DeepSeek, Doubao, Kimi, Yuanbao, Qwen public share links"
        )
    })
}

pub fn canonical_share_url(raw_url: &str) -> anyhow::Result<String> {
    let url = Url::parse(raw_url.trim()).context("invalid share URL")?;
    let source = detect_share_source_url(&url).ok_or_else(|| {
        anyhow::anyhow!(
            "Unsupported share URL. Supported: ChatGPT, Claude, Gemini, DeepSeek, Doubao, Kimi, Yuanbao, Qwen public share links"
        )
    })?;
    let share_id = share_external_id_from_url(source, &url)
        .ok_or_else(|| anyhow::anyhow!("Share URL is missing a share id"))?;

    let canonical = match source {
        SourceKind::ChatgptShare => format!("https://chatgpt.com/share/{share_id}"),
        SourceKind::ClaudeShare => format!("https://claude.ai/share/{share_id}"),
        SourceKind::GeminiShare => format!("https://gemini.google.com/share/{share_id}"),
        SourceKind::DeepseekShare => format!("https://chat.deepseek.com/share/{share_id}"),
        SourceKind::DoubaoShare => {
            let first = url
                .path_segments()
                .and_then(|mut parts| parts.find(|part| !part.is_empty()))
                .unwrap_or("thread");
            if first == "s" {
                format!("https://www.doubao.com/s/{share_id}")
            } else {
                format!("https://www.doubao.com/thread/{share_id}")
            }
        }
        SourceKind::KimiShare => format!("https://www.kimi.com/share/{share_id}"),
        SourceKind::YuanbaoShare => format!("https://yb.tencent.com/s/{share_id}"),
        SourceKind::QwenShare => format!("https://www.qianwen.com/share/chat/{share_id}"),
        _ => anyhow::bail!("not a Share URL source"),
    };
    Ok(canonical)
}

pub fn share_external_id(raw_url: &str) -> anyhow::Result<String> {
    let url = Url::parse(raw_url.trim()).context("invalid share URL")?;
    let source =
        detect_share_source_url(&url).ok_or_else(|| anyhow::anyhow!("Unsupported share URL"))?;
    share_external_id_from_url(source, &url)
        .ok_or_else(|| anyhow::anyhow!("Share URL is missing a share id"))
}

pub fn persist_share_conversation(
    mut input: ShareConversationInput,
) -> anyhow::Result<ShareImportResult> {
    let source = SourceKind::from_str(input.source.trim())
        .ok_or_else(|| anyhow::anyhow!("Unknown Share source: {}", input.source))?;
    anyhow::ensure!(
        matches!(
            source,
            SourceKind::ChatgptShare
                | SourceKind::ClaudeShare
                | SourceKind::GeminiShare
                | SourceKind::DeepseekShare
                | SourceKind::DoubaoShare
                | SourceKind::KimiShare
                | SourceKind::YuanbaoShare
                | SourceKind::QwenShare
        ),
        "Only Share URL sources can be imported"
    );
    anyhow::ensure!(
        !input.messages.is_empty(),
        "Shared conversation has no messages"
    );
    anyhow::ensure!(
        input.messages.len() <= 20_000,
        "Shared conversation has too many messages"
    );

    let canonical_url = canonical_share_url(&input.source_url)?;
    let url_source = detect_share_source(&canonical_url)?;
    anyhow::ensure!(
        url_source == source,
        "Share URL provider does not match parsed source"
    );

    let url_external_id = share_external_id(&canonical_url)?;
    if input.external_session_id.trim().is_empty() {
        input.external_session_id = url_external_id.clone();
    }
    anyhow::ensure!(
        input.external_session_id == url_external_id,
        "Share id does not match the supplied URL"
    );

    let mut messages = Vec::with_capacity(input.messages.len());
    let mut latest_message_time = None;
    for (index, item) in input.messages.into_iter().enumerate() {
        let created_at = item.created_at.as_deref().and_then(parse_datetime);
        if let Some(value) = created_at {
            latest_message_time = Some(
                latest_message_time.map_or(value, |current: DateTime<Utc>| current.max(value)),
            );
        }

        let role = MessageRole::from_str(&item.role);
        let mut blocks = Vec::new();
        if !item.text.trim().is_empty() {
            blocks.push(ContentBlock::Text {
                text: item.text.trim().to_string(),
            });
        }
        for asset in item.assets {
            if asset.url.trim().is_empty() {
                continue;
            }
            if asset.kind.eq_ignore_ascii_case("image") {
                blocks.push(ContentBlock::Image {
                    source: asset.url,
                    media_type: asset.media_type,
                });
            } else {
                blocks.push(ContentBlock::FileReference {
                    path: asset.url,
                    name: asset.name,
                });
            }
        }
        if blocks.is_empty() {
            continue;
        }

        messages.push(NormalizedMessage {
            external_id: item
                .external_id
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| format!("share-turn-{index}")),
            parent_id: (index > 0).then(|| format!("share-turn-{}", index - 1)),
            role,
            created_at,
            model: input.model.clone(),
            blocks,
            usage: None,
            metadata: HashMap::new(),
        });
    }
    anyhow::ensure!(
        !messages.is_empty(),
        "Shared conversation has no usable messages"
    );

    let source_updated_at = input
        .updated_at
        .as_deref()
        .and_then(parse_datetime)
        .or(latest_message_time)
        .unwrap_or_else(Utc::now);

    let mut metadata = HashMap::new();
    metadata.insert("share_url".to_string(), serde_json::json!(canonical_url));
    metadata.insert(
        "imported_at".to_string(),
        serde_json::json!(Utc::now().to_rfc3339()),
    );
    metadata.insert(
        "share_provider".to_string(),
        serde_json::json!(source.display_name()),
    );

    let external_session_id = input.external_session_id;
    let title = input
        .title
        .map(|value| value.trim().chars().take(300).collect::<String>())
        .filter(|value| !value.is_empty());

    let cache_path = share_cache_root()
        .join(source.as_str())
        .join(format!("{external_session_id}.json"));

    let mut session = NormalizedSession {
        source,
        external_session_id: external_session_id.clone(),
        title: title.clone(),
        project_name: Some(format!("{} Web", source.display_name())),
        project_path: None,
        source_path: Some(cache_path.clone()),
        started_at: messages
            .iter()
            .filter_map(|message| message.created_at)
            .min(),
        updated_at: Some(source_updated_at),
        model: input.model,
        messages,
        usage: None,
        metadata,
    };

    let content_hash = normalized_session_hash(&session)?;
    session.metadata.insert(
        "share_content_hash".to_string(),
        serde_json::json!(content_hash.clone()),
    );

    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create Share URL cache directory: {}", parent.display()))?;
    }
    let temp_path = cache_path.with_extension("json.tmp");
    fs::write(&temp_path, serde_json::to_vec_pretty(&session)?)
        .with_context(|| format!("write Share URL cache: {}", temp_path.display()))?;
    if cache_path.exists() {
        fs::remove_file(&cache_path).ok();
    }
    fs::rename(&temp_path, &cache_path)
        .with_context(|| format!("commit Share URL cache: {}", cache_path.display()))?;

    Ok(ShareImportResult {
        source,
        external_session_id,
        title,
        message_count: session.messages.len(),
        source_url: canonical_url,
        cache_path,
        content_hash,
    })
}

fn detect_share_source_url(url: &Url) -> Option<SourceKind> {
    if url.scheme() != "https" {
        return None;
    }
    let host = url
        .host_str()?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    let parts = url
        .path_segments()?
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    match host.as_str() {
        "chatgpt.com" | "chat.openai.com"
            if parts.first().copied() == Some("share") && parts.len() >= 2 =>
        {
            Some(SourceKind::ChatgptShare)
        }
        "claude.ai" if parts.first().copied() == Some("share") && parts.len() == 2 => {
            Some(SourceKind::ClaudeShare)
        }
        "gemini.google.com" if parts.first().copied() == Some("share") && parts.len() == 2 => {
            Some(SourceKind::GeminiShare)
        }
        "share.gemini.google" if parts.len() == 1 => Some(SourceKind::GeminiShare),
        "g.co" if parts.len() == 3 && parts[0] == "gemini" && parts[1] == "share" => {
            Some(SourceKind::GeminiShare)
        }
        "chat.deepseek.com"
            if parts.first().copied() == Some("share") && parts.len() == 2 =>
        {
            Some(SourceKind::DeepseekShare)
        }
        "doubao.com"
            if matches!(parts.first().copied(), Some("thread") | Some("s"))
                && parts.len() == 2 =>
        {
            Some(SourceKind::DoubaoShare)
        }
        "kimi.com" | "kimi.moonshot.cn"
            if parts.first().copied() == Some("share") && parts.len() == 2 =>
        {
            Some(SourceKind::KimiShare)
        }
        "yb.tencent.com" if parts.first().copied() == Some("s") && parts.len() == 2 => {
            Some(SourceKind::YuanbaoShare)
        }
        "qianwen.com"
            if parts.first().copied() == Some("share")
                && parts.get(1).copied() == Some("chat")
                && parts.len() == 3 =>
        {
            Some(SourceKind::QwenShare)
        }
        "activity.qianwen.com" if url.query_pairs().any(|(key, _)| key == "shareId") => {
            Some(SourceKind::QwenShare)
        }
        _ => None,
    }
}

fn share_external_id_from_url(source: SourceKind, url: &Url) -> Option<String> {
    let parts = url
        .path_segments()?
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    let value = match source {
        SourceKind::ChatgptShare => {
            if parts.get(1).copied() == Some("e") {
                parts.get(2).copied()
            } else {
                parts.get(1).copied()
            }
        }
        SourceKind::ClaudeShare => parts.get(1).copied(),
        SourceKind::GeminiShare => match url.host_str()?.trim_start_matches("www.") {
            "g.co" => parts.get(2).copied(),
            "share.gemini.google" => parts.first().copied(),
            _ => parts.get(1).copied(),
        },
        SourceKind::DeepseekShare
        | SourceKind::DoubaoShare
        | SourceKind::KimiShare
        | SourceKind::YuanbaoShare => parts.get(1).copied(),
        SourceKind::QwenShare => {
            if url
                .host_str()?
                .trim_start_matches("www.")
                .eq_ignore_ascii_case("activity.qianwen.com")
            {
                return url
                    .query_pairs()
                    .find_map(|(key, value)| (key == "shareId").then(|| value.into_owned()))
                    .filter(|value| !value.trim().is_empty());
            }
            parts.get(2).copied()
        }
        _ => None,
    }?;

    let value = value.trim();
    if value.len() < 4
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return None;
    }
    Some(value.to_string())
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

pub fn normalized_session_hash(session: &NormalizedSession) -> anyhow::Result<String> {
    let stable = serde_json::json!({
        "source": session.source,
        "external_session_id": session.external_session_id,
        "title": session.title,
        "model": session.model,
        "messages": session.messages,
    });
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&stable)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_and_canonicalizes_supported_share_urls() {
        assert_eq!(
            detect_share_source("https://chatgpt.com/share/abc-def").unwrap(),
            SourceKind::ChatgptShare
        );
        assert_eq!(
            canonical_share_url("https://chat.openai.com/share/abc-def").unwrap(),
            "https://chatgpt.com/share/abc-def"
        );
        assert_eq!(
            canonical_share_url("https://g.co/gemini/share/abc123").unwrap(),
            "https://gemini.google.com/share/abc123"
        );
        assert_eq!(
            canonical_share_url("https://share.gemini.google/abc123").unwrap(),
            "https://gemini.google.com/share/abc123"
        );
        assert_eq!(
            canonical_share_url("https://chat.deepseek.com/share/deep123").unwrap(),
            "https://chat.deepseek.com/share/deep123"
        );
        assert_eq!(
            canonical_share_url("https://www.doubao.com/thread/doubao123").unwrap(),
            "https://www.doubao.com/thread/doubao123"
        );
        assert_eq!(
            canonical_share_url("https://www.kimi.com/share/kimi1234").unwrap(),
            "https://www.kimi.com/share/kimi1234"
        );
        assert_eq!(
            canonical_share_url("https://yb.tencent.com/s/yuanbao123").unwrap(),
            "https://yb.tencent.com/s/yuanbao123"
        );
        assert_eq!(
            canonical_share_url("https://www.qianwen.com/share/chat/qwen1234").unwrap(),
            "https://www.qianwen.com/share/chat/qwen1234"
        );
        assert_eq!(
            canonical_share_url(
                "https://activity.qianwen.com/share?shareId=qwen5678&authorId=test"
            )
            .unwrap(),
            "https://www.qianwen.com/share/chat/qwen5678"
        );
        assert_eq!(
            canonical_share_url("https://www.doubao.com/s/short123").unwrap(),
            "https://www.doubao.com/s/short123"
        );
    }

    #[test]
    fn rejects_private_or_non_share_urls() {
        assert!(detect_share_source("http://chatgpt.com/share/abc-def").is_err());
        assert!(detect_share_source("https://chatgpt.com/c/abc-def").is_err());
        assert!(detect_share_source("https://claude.ai/chat/abc-def").is_err());
        assert!(detect_share_source("https://example.com/share/abc-def").is_err());
    }
}
