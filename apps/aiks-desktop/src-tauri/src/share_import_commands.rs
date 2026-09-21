use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use aiks_core::{ShareConversationInput, ShareImportResult, SourceKind, SyncOptions};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::app_state::AppState;

const CAPTURE_PREFIX: &str = "__AIKS_SHARE_CAPTURE__";
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(35);
const CAPTURE_REVEAL_AFTER: Duration = Duration::from_secs(7);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareImportCommandResult {
    pub source: String,
    pub external_session_id: String,
    pub title: Option<String>,
    pub message_count: usize,
    pub session_id: i64,
    pub pipeline_queued: usize,
}

struct ChunkCapture {
    total: Option<usize>,
    chunks: Vec<Option<String>>,
    sender: Option<oneshot::Sender<Result<String, String>>>,
}

impl ChunkCapture {
    fn new(sender: oneshot::Sender<Result<String, String>>) -> Self {
        Self {
            total: None,
            chunks: Vec::new(),
            sender: Some(sender),
        }
    }

    fn fail(&mut self, error: String) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Err(error));
        }
    }

    fn push_chunk(&mut self, index: usize, total: usize, chunk: String) -> bool {
        if total == 0 || total > 10_000 || index >= total {
            self.fail("Invalid Share URL capture framing".to_string());
            return true;
        }
        if self.total.is_none() {
            self.total = Some(total);
            self.chunks.resize(total, None);
        }
        if self.total != Some(total) || self.chunks.len() != total {
            self.fail("Inconsistent Share URL capture framing".to_string());
            return true;
        }
        self.chunks[index] = Some(chunk);
        if self.chunks.iter().any(Option::is_none) {
            return false;
        }

        let encoded = self
            .chunks
            .iter()
            .filter_map(|value| value.as_deref())
            .collect::<String>();
        let decoded = BASE64_STANDARD
            .decode(encoded)
            .map_err(|error| format!("Decode Share URL capture: {error}"))
            .and_then(|bytes| {
                String::from_utf8(bytes)
                    .map_err(|error| format!("Share URL capture is not UTF-8: {error}"))
            });
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(decoded);
        }
        true
    }
}

#[tauri::command]
pub async fn fetch_chatgpt_share_html(url: String) -> Result<String, String> {
    let source = aiks_core::detect_share_source(&url).map_err(|error| error.to_string())?;
    if source != SourceKind::ChatgptShare {
        return Err("This fetch path only accepts ChatGPT public share links".to_string());
    }
    let canonical = aiks_core::canonical_share_url(&url).map_err(|error| error.to_string())?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(8))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(canonical)
        .header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/153 Safari/537.36",
        )
        .header(
            reqwest::header::ACCEPT,
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
        .header(reqwest::header::REFERER, "https://chatgpt.com/")
        .send()
        .await
        .map_err(|error| format!("Fetch ChatGPT share: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("ChatGPT share returned HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|size| size > 25 * 1024 * 1024)
    {
        return Err("ChatGPT share page is unexpectedly large".to_string());
    }
    response
        .text()
        .await
        .map_err(|error| format!("Read ChatGPT share page: {error}"))
}

#[tauri::command]
pub async fn persist_share_conversation(
    input: ShareConversationInput,
    state: State<'_, AppState>,
) -> Result<ShareImportCommandResult, String> {
    persist_and_sync(input, state.inner()).await
}

#[tauri::command]
pub async fn import_share_url_browser(
    url: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ShareImportCommandResult, String> {
    let source = aiks_core::detect_share_source(&url).map_err(|error| error.to_string())?;
    if !matches!(
        source,
        SourceKind::ClaudeShare
            | SourceKind::GeminiShare
            | SourceKind::DeepseekShare
            | SourceKind::DoubaoShare
            | SourceKind::KimiShare
            | SourceKind::YuanbaoShare
            | SourceKind::QwenShare
    ) {
        return Err("This Share URL is not supported by the browser importer".to_string());
    }
    let canonical = aiks_core::canonical_share_url(&url).map_err(|error| error.to_string())?;
    let json = capture_in_webview(&app, source, &canonical).await?;
    let input: ShareConversationInput = serde_json::from_str(&json)
        .map_err(|error| format!("Parse browser Share capture: {error}"))?;
    persist_and_sync(input, state.inner()).await
}

async fn persist_and_sync(
    input: ShareConversationInput,
    state: &AppState,
) -> Result<ShareImportCommandResult, String> {
    let stored: ShareImportResult =
        aiks_core::persist_share_conversation(input).map_err(|error| error.to_string())?;
    let engine = state.engine().ok_or("Engine not initialized")?;
    let source = stored.source.as_str().to_string();

    let stats = engine
        .sync_and_enqueue_extraction(SyncOptions {
            source_filter: Some(source.clone()),
            dry_run: false,
            overwrite: false,
        })
        .await
        .map_err(|error| format!("Share saved, but AIKS sync failed: {error}"))?;

    let session_id = engine
        .db()
        .conn()
        .query_row(
            "SELECT id FROM source_session WHERE source = ?1 AND external_session_id = ?2",
            rusqlite::params![source, stored.external_session_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| format!("Share saved, but session row was not found: {error}"))?;

    Ok(ShareImportCommandResult {
        source: stored.source.as_str().to_string(),
        external_session_id: stored.external_session_id,
        title: stored.title,
        message_count: stored.message_count,
        session_id,
        pipeline_queued: stats.extraction_candidates.len(),
    })
}

async fn capture_in_webview(
    app: &AppHandle,
    source: SourceKind,
    canonical_url: &str,
) -> Result<String, String> {
    let token = Uuid::new_v4().simple().to_string();
    let label = format!("share-import-{}", &token[..12]);
    let target_url: tauri::Url = canonical_url
        .parse()
        .map_err(|error| format!("Invalid canonical Share URL: {error}"))?;
    let script = browser_capture_script(source, &token);

    let (sender, receiver) = oneshot::channel();
    let capture = Arc::new(StdMutex::new(ChunkCapture::new(sender)));
    let callback_capture = Arc::clone(&capture);
    let callback_token = token.clone();
    let expected_source = source;

    WebviewWindowBuilder::new(app, &label, WebviewUrl::External(target_url))
        .title("AIKS Share Import")
        .inner_size(1000.0, 760.0)
        .visible(false)
        .focusable(true)
        .initialization_script(script)
        .on_navigation(move |url| navigation_allowed(expected_source, url))
        .on_document_title_changed(move |window, title| {
            let Some(payload) = title.strip_prefix(CAPTURE_PREFIX) else {
                return;
            };
            let fields = payload.splitn(6, '|').collect::<Vec<_>>();
            if fields.len() < 4 || fields[1] != callback_token {
                return;
            }

            let completed = if let Ok(mut state) = callback_capture.lock() {
                match fields[2] {
                    "ERROR" => {
                        let encoded = fields.get(3).copied().unwrap_or_default();
                        let message = BASE64_STANDARD
                            .decode(encoded)
                            .ok()
                            .and_then(|bytes| String::from_utf8(bytes).ok())
                            .unwrap_or_else(|| "Share page extraction failed".to_string());
                        state.fail(message);
                        true
                    }
                    "DATA" if fields.len() == 6 => {
                        let index = fields[3].parse::<usize>().ok();
                        let total = fields[4].parse::<usize>().ok();
                        let chunk = fields[5].to_string();
                        match (index, total) {
                            (Some(index), Some(total)) => state.push_chunk(index, total, chunk),
                            _ => {
                                state.fail("Invalid Share URL capture message".to_string());
                                true
                            }
                        }
                    }
                    _ => false,
                }
            } else {
                true
            };
            if completed {
                let _ = window.destroy();
            }
        })
        .build()
        .map_err(|error| format!("Create Share URL browser: {error}"))?;

    let reveal_app = app.clone();
    let reveal_label = label.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CAPTURE_REVEAL_AFTER).await;
        if let Some(window) = reveal_app.get_webview_window(&reveal_label) {
            let _ = window.show();
            let _ = window.set_focus();
        }
    });

    let outcome = tokio::time::timeout(CAPTURE_TIMEOUT, receiver).await;
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.destroy();
    }

    match outcome {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("Share URL capture channel closed unexpectedly".to_string()),
        Err(_) => Err(
            "Timed out reading the shared conversation. If a browser challenge appeared, complete it and try again."
                .to_string(),
        ),
    }
}

fn navigation_allowed(source: SourceKind, url: &tauri::Url) -> bool {
    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    match source {
        SourceKind::ClaudeShare => host == "claude.ai",
        SourceKind::GeminiShare => matches!(
            host.as_str(),
            "gemini.google.com"
                | "g.co"
                | "share.gemini.google"
                | "accounts.google.com"
                | "consent.google.com"
        ),
        SourceKind::DeepseekShare => host == "chat.deepseek.com",
        SourceKind::DoubaoShare => matches!(host.as_str(), "doubao.com" | "o.doubao.com"),
        SourceKind::KimiShare => matches!(host.as_str(), "kimi.com" | "kimi.moonshot.cn"),
        SourceKind::YuanbaoShare => {
            matches!(host.as_str(), "yb.tencent.com" | "yuanbao.tencent.com")
        }
        SourceKind::QwenShare => matches!(
            host.as_str(),
            "qianwen.com" | "chat2-api.qianwen.com" | "activity.qianwen.com"
        ),
        _ => false,
    }
}

fn browser_capture_script(source: SourceKind, token: &str) -> String {
    BROWSER_CAPTURE_SCRIPT
        .replace("__AIKS_TOKEN__", &serde_json::to_string(token).unwrap())
        .replace(
            "__AIKS_PROVIDER__",
            &serde_json::to_string(source.as_str()).unwrap(),
        )
}

// Claude selectors are adapted from pencil311/chat-share-reader (MIT).
// Gemini selectors are adapted from TheBluCoder/AI-chat-exporter (MIT).
// See THIRD_PARTY_NOTICES.md.
const BROWSER_CAPTURE_SCRIPT: &str = r#"
(() => {
  if (window.self !== window.top) return;
  const PREFIX = "__AIKS_SHARE_CAPTURE__";
  const TOKEN = __AIKS_TOKEN__;
  const PROVIDER = __AIKS_PROVIDER__;
  const CHUNK_SIZE = 5500;
  let sent = false;

  const utf8ToBase64 = (text) => {
    const bytes = new TextEncoder().encode(text);
    let binary = "";
    const step = 0x8000;
    for (let i = 0; i < bytes.length; i += step) {
      binary += String.fromCharCode(...bytes.subarray(i, i + step));
    }
    return btoa(binary);
  };
  const sendError = (error) => {
    if (sent) return;
    sent = true;
    const message = error instanceof Error ? error.message : String(error || "Share extraction failed");
    document.title = PREFIX + "|" + TOKEN + "|ERROR|" + utf8ToBase64(message);
  };
  const sendData = (value) => {
    if (sent) return;
    const json = JSON.stringify(value);
    if (!json || json.length < 2) return sendError("Shared conversation was empty");
    sent = true;
    const encoded = utf8ToBase64(json);
    const total = Math.max(1, Math.ceil(encoded.length / CHUNK_SIZE));
    for (let index = 0; index < total; index += 1) {
      const chunk = encoded.slice(index * CHUNK_SIZE, (index + 1) * CHUNK_SIZE);
      window.setTimeout(() => {
        document.title = PREFIX + "|" + TOKEN + "|DATA|" + index + "|" + total + "|" + chunk;
      }, index * 35);
    }
  };
  const readText = (element) => {
    if (!element) return "";
    return (typeof element.innerText === "string" ? element.innerText : element.textContent || "").trim();
  };
  const markdownText = (element) => {
    if (!element) return "";
    const clone = element.cloneNode(true);
    clone.querySelectorAll(
      'button,[data-testid="action-bar-copy"],[data-testid="action-bar-read-aloud"],[data-testid="page-header"]'
    ).forEach((node) => node.remove());
    clone.querySelectorAll("pre").forEach((pre) => {
      const code = pre.querySelector("code");
      if (!code) return;
      const className = code.getAttribute("class") || "";
      const match = className.match(/language-([\w+-]+)/);
      const language = match ? match[1] : "";
      const body = readText(code);
      const tick = String.fromCharCode(96);
      const fence = body.indexOf(tick + tick + tick) >= 0 ? tick.repeat(4) : tick.repeat(3);
      pre.replaceWith(document.createTextNode("\n" + fence + language + "\n" + body + "\n" + fence + "\n"));
    });
    clone.querySelectorAll("code").forEach((code) => {
      const body = readText(code);
      const tick = String.fromCharCode(96);
      code.replaceWith(document.createTextNode(body ? tick + body + tick : ""));
    });
    return readText(clone).replace(/\n{3,}/g, "\n\n").trim();
  };
  const shareId = () => {
    const parts = location.pathname.split("/").filter(Boolean);
    return parts[1] || parts[parts.length - 1] || "";
  };
  const assetFrom = (item) => {
    if (!item || typeof item !== "object") return null;
    const url = item.url || item.preview_url || item.download_url || item.file_url ||
      (item.source && item.source.url) || (item.asset && item.asset.url);
    if (!url || typeof url !== "string") return null;
    const mime = item.mime_type || item.content_type || null;
    const name = item.file_name || item.filename || item.name || null;
    const kind = String(item.type || item.file_type || mime || "").toLowerCase().includes("image") ? "image" : "file";
    return { kind, url, name, mediaType: typeof mime === "string" ? mime : null };
  };
  const claudeBlockText = (block) => {
    if (!block || typeof block !== "object") return "";
    const type = String(block.type || "text");
    if (type === "code") {
      const body = String(block.code || block.text || "");
      const language = String(block.language || "");
      const tick = String.fromCharCode(96);
      const fence = body.indexOf(tick + tick + tick) >= 0 ? tick.repeat(4) : tick.repeat(3);
      return body ? fence + language + "\n" + body + "\n" + fence : "";
    }
    if (type === "tool_result" || type === "tool_use") {
      return String(block.display_content || block.message || block.name || "");
    }
    return String(block.text || "");
  };

  const captureClaude = async () => {
    if (location.hostname !== "claude.ai" || !location.pathname.startsWith("/share/")) return;
    const id = shareId();
    if (!id) throw new Error("Claude share id is missing");
    try {
      const response = await fetch("/api/chat_snapshots/" + encodeURIComponent(id), {
        credentials: "include",
        headers: { Accept: "application/json" }
      });
      if (!response.ok) throw new Error("Claude snapshot API returned HTTP " + response.status);
      const snapshot = await response.json();
      const rawMessages = Array.isArray(snapshot.chat_messages) ? snapshot.chat_messages : [];
      const messages = [];
      rawMessages.forEach((message, index) => {
        if (!message || typeof message !== "object") return;
        const sender = String(message.sender || "").toLowerCase();
        const role = sender === "human" || sender === "user" ? "user" :
          sender === "assistant" ? "assistant" : sender === "system" ? "system" : "unknown";
        const parts = [];
        if (typeof message.text === "string" && message.text.trim()) parts.push(message.text.trim());
        if (Array.isArray(message.content)) {
          message.content.forEach((block) => {
            const text = claudeBlockText(block).trim();
            if (text) parts.push(text);
          });
        }
        const assets = [];
        [message.attachments, message.files].forEach((collection) => {
          if (!Array.isArray(collection)) return;
          collection.forEach((item) => {
            const asset = assetFrom(item);
            if (asset) assets.push(asset);
          });
        });
        if (!parts.length && !assets.length) return;
        messages.push({
          externalId: String(message.uuid || message.id || "share-turn-" + index),
          role,
          text: parts.join("\n\n"),
          createdAt: message.created_at || message.updated_at || null,
          assets
        });
      });
      if (!messages.length) throw new Error("Claude snapshot contained no messages");
      sendData({
        source: "claude_share",
        sourceUrl: "https://claude.ai/share/" + id,
        externalSessionId: id,
        title: snapshot.snapshot_name || snapshot.name || document.title || "Claude Shared Conversation",
        model: null,
        updatedAt: snapshot.updated_at || snapshot.created_at || null,
        messages
      });
      return;
    } catch (_) {}

    let attempts = 0;
    let stable = 0;
    let previous = -1;
    const timer = window.setInterval(() => {
      attempts += 1;
      const messages = [];
      const rows = Array.from(document.querySelectorAll('[class*="group/message-row"]'));
      rows.forEach((row) => {
        const user = row.querySelector('[data-testid="user-message"]');
        if (user) {
          const text = markdownText(user);
          if (text) messages.push({ externalId: "share-turn-" + messages.length, role: "user", text, createdAt: null, assets: [] });
        }
        let assistant = row.querySelector('[class*="standard-markdown"]');
        if (!assistant) {
          const candidates = Array.from(row.querySelectorAll('[class*="font-claude-response"]'));
          assistant = candidates.find((candidate) =>
            !candidates.some((other) => other !== candidate && other.contains(candidate))
          ) || null;
        }
        if (assistant) {
          const text = markdownText(assistant);
          if (text) messages.push({ externalId: "share-turn-" + messages.length, role: "assistant", text, createdAt: null, assets: [] });
        }
      });
      if (messages.length > 0 && messages.length === previous) stable += 1; else stable = 0;
      previous = messages.length;
      if (messages.length > 0 && stable >= 2) {
        window.clearInterval(timer);
        sendData({
          source: "claude_share",
          sourceUrl: "https://claude.ai/share/" + id,
          externalSessionId: id,
          title: (document.querySelector('[data-testid="page-header"]') || {}).textContent || document.title || "Claude Shared Conversation",
          model: null,
          updatedAt: null,
          messages
        });
      } else if (attempts >= 45) {
        window.clearInterval(timer);
        sendError("Claude share page loaded, but no conversation messages could be extracted");
      }
    }, 500);
  };

  const captureGemini = () => {
    if (location.hostname !== "gemini.google.com" || !location.pathname.startsWith("/share/")) return;
    const id = shareId();
    if (!id) return sendError("Gemini share id is missing");
    let attempts = 0;
    let stable = 0;
    let previous = -1;
    const timer = window.setInterval(() => {
      attempts += 1;
      const messages = [];
      const turns = Array.from(document.querySelectorAll("message-set"));
      turns.forEach((turn) => {
        const user = turn.querySelector("user-query, user-query-content, [data-test-id='user-query']");
        const model = turn.querySelector("model-response, message-content, [data-test-id='model-response']");
        const userText = markdownText(user);
        const modelText = markdownText(model);
        if (userText) messages.push({ externalId: "share-turn-" + messages.length, role: "user", text: userText, createdAt: null, assets: [] });
        if (modelText) messages.push({ externalId: "share-turn-" + messages.length, role: "assistant", text: modelText, createdAt: null, assets: [] });
      });
      if (!turns.length) {
        const nodes = Array.from(document.querySelectorAll(
          "user-query, [data-test-id='user-query'], model-response, [data-test-id='model-response']"
        ));
        nodes.sort((left, right) => left === right ? 0 :
          (left.compareDocumentPosition(right) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1));
        nodes.forEach((node) => {
          const tag = node.tagName.toLowerCase();
          const testId = node.getAttribute("data-test-id") || "";
          const role = tag.includes("user") || testId.includes("user") ? "user" : "assistant";
          const text = markdownText(node);
          if (text) messages.push({ externalId: "share-turn-" + messages.length, role, text, createdAt: null, assets: [] });
        });
      }
      if (messages.length > 0 && messages.length === previous) stable += 1; else stable = 0;
      previous = messages.length;
      if (messages.length > 0 && stable >= 3) {
        window.clearInterval(timer);
        let title = document.title || "Gemini Shared Conversation";
        title = title.replace(/\s*[-–—]\s*Google Gemini\s*$/i, "").trim();
        sendData({
          source: "gemini_share",
          sourceUrl: "https://gemini.google.com/share/" + id,
          externalSessionId: id,
          title,
          model: "Gemini",
          updatedAt: null,
          messages
        });
      } else if (attempts >= 50) {
        window.clearInterval(timer);
        sendError("Gemini share page loaded, but no conversation messages could be extracted");
      }
    }, 500);
  };

  const start = () => {
    if (PROVIDER === "claude_share") captureClaude().catch(sendError);
    if (PROVIDER === "gemini_share") captureGemini();
  };
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start, { once: true });
  } else {
    start();
  }
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_script_is_scoped_to_requested_provider_and_token() {
        let script = browser_capture_script(SourceKind::ClaudeShare, "abc123");
        assert!(script.contains(r#"const TOKEN = "abc123";"#));
        assert!(script.contains(r#"const PROVIDER = "claude_share";"#));
        assert!(!script.contains("__AIKS_TOKEN__"));
        assert!(!script.contains("__AIKS_PROVIDER__"));
    }

    #[test]
    fn browser_navigation_is_restricted() {
        assert!(navigation_allowed(
            SourceKind::ClaudeShare,
            &"https://claude.ai/share/abc".parse().unwrap()
        ));
        assert!(!navigation_allowed(
            SourceKind::ClaudeShare,
            &"https://example.com/".parse().unwrap()
        ));
        assert!(navigation_allowed(
            SourceKind::GeminiShare,
            &"https://consent.google.com/".parse().unwrap()
        ));
    }
}
