/// V3 AI Extraction Stage
///
/// Calls the AI model on session chunks and produces 0~N KnowledgeItems.
/// Handles Map-Reduce for long sessions.
use std::time::Instant;

use tracing::info;

use crate::ai::{
    AiClient,
    config::AiModelConfig,
    prompts_v3::{make_v3_chunk_prompt, make_v3_extraction_prompt, make_v3_final_prompt, SYSTEM_PROMPT_V3},
    schema_v3::{V3ExtractionResult, V3KnowledgeItem},
};
use crate::pipeline::knowledge_repo::KnowledgeRepo;
use crate::pipeline::repo::PipelineRepo;
use crate::pipeline::session_chunker::load_chunks;
use crate::storage::StateDb;
use crate::util::SecretSanitizer;

pub struct AiStage {
    client: AiClient,
    sanitizer: SecretSanitizer,
    config: AiModelConfig,
}

impl AiStage {
    pub fn new(config: AiModelConfig) -> anyhow::Result<Self> {
        let client = AiClient::new(config.clone())?;
        Ok(Self {
            client,
            sanitizer: SecretSanitizer::new(),
            config,
        })
    }

    /// Run AI extraction on a session's LLM chunks.
    /// Returns the number of knowledge items created.
    pub async fn run(
        &self,
        db: &StateDb,
        pipeline_run_id: &str,
        session_id: i64,
        session_title: Option<&str>,
        project_name: Option<&str>,
    ) -> anyhow::Result<usize> {
        let pipeline_repo = PipelineRepo::new(db);
        let knowledge_repo = KnowledgeRepo::new(db);

        // Load chunks from DB
        let chunks = load_chunks(db, session_id)?;

        if chunks.is_empty() {
            anyhow::bail!("No chunks found for session {}", session_id);
        }

        let t0 = Instant::now();
        info!(session_id, chunks = chunks.len(), "[AI] Starting extraction");

        let result = if chunks.len() == 1 {
            // Single chunk: direct extraction
            let sanitized = self.sanitizer.sanitize(&chunks[0].1);
            let prompt = make_v3_extraction_prompt(&sanitized);
            let response = self.client.chat(SYSTEM_PROMPT_V3, &prompt).await?;
            // R10: parse errors propagate — model failure / protocol breakage
            // must surface as a stage error, never as a silent "skip".
            parse_v3_result_typed(&response)?
        } else {
            // Multiple chunks: Map-Reduce
            self.map_reduce(session_title, project_name, &chunks).await?
        };

        let latency_ms = t0.elapsed().as_millis() as i64;

        // Log AI request
        log_ai_request(db, pipeline_run_id, "AI_EXTRACT", &self.config.model, &self.config.base_url, chunks.len(), result.items.len(), latency_ms, true);

        if !result.worth_extracting || result.items.is_empty() {
            info!(
                session_id,
                score = result.knowledge_score,
                "[AI] Session not worth extracting or produced 0 items"
            );
            pipeline_repo.record_stage(
                pipeline_run_id, "AI_EXTRACTED", "SUCCESS",
                Some(chunks.len() as i32), Some(0), Some(latency_ms),
                Some(&serde_json::json!({"score": result.knowledge_score, "items": 0})),
                None,
            )?;
            return Ok(0);
        }

        let item_count = result.items.len();
        info!(session_id, items = item_count, latency_ms, "[AI] Extraction complete");

        // Save knowledge items
        knowledge_repo.save_items(session_id, project_name, &result)?;

        pipeline_repo.record_stage(
            pipeline_run_id, "AI_EXTRACTED", "SUCCESS",
            Some(chunks.len() as i32), Some(item_count as i32), Some(latency_ms),
            Some(&serde_json::json!({"score": result.knowledge_score, "items": item_count})),
            None,
        )?;

        Ok(item_count)
    }

    async fn map_reduce(
        &self,
        session_title: Option<&str>,
        project_name: Option<&str>,
        chunks: &[(i32, String)],
    ) -> anyhow::Result<V3ExtractionResult> {
        let total = chunks.len();
        let mut chunk_summaries = Vec::new();

        for (idx, (_, text)) in chunks.iter().enumerate() {
            let sanitized = self.sanitizer.sanitize(text);
            let prompt = make_v3_chunk_prompt(&sanitized, idx, total);
            // R10: a failed chunk means the map-reduce input is incomplete —
            // propagate the error instead of fabricating a degraded summary.
            let resp = self.client.chat(SYSTEM_PROMPT_V3, &prompt).await?;
            chunk_summaries.push(resp);
        }

        let title = session_title.unwrap_or("未知会话");
        let final_prompt = make_v3_final_prompt(title, project_name, &chunk_summaries);
        let response = self.client.chat(SYSTEM_PROMPT_V3, &final_prompt).await?;
        // R10: final parse errors propagate as real failures.
        parse_v3_result_typed(&response)
    }
}

/// B17/R10/R11: Parse V3 result — returns Ok(result) or Err if JSON is invalid/malformed.
/// This separates "no knowledge" (valid skip) from "AI output broken" (error).
/// The error preview is cut on character boundaries (UTF-8 safe).
/// Public for testing.
pub fn parse_v3_result_typed(response: &str) -> anyhow::Result<V3ExtractionResult> {
    let clean = clean_json(response);
    if clean.is_empty() || (!clean.starts_with('{')) {
        anyhow::bail!(
            "AI response is not a JSON object: {:?}",
            crate::util::safe_preview(response, 100)
        );
    }
    if let Ok(r) = serde_json::from_str::<V3ExtractionResult>(&clean) {
        return Ok(r);
    }
    // Qwen-style small models emit near-JSON: smart quotes, JS-style bare
    // keys (ASCII or Chinese), mixed/single quotes, Python literals, trailing
    // commas. Escalating lenient repairs — every attempt's output is validated
    // by serde_json, so an over-eager pass simply fails validation and the
    // next one runs; a mangled attempt can never shadow a good one.
    for repaired in repair_attempts(&clean) {
        if let Ok(r) = serde_json::from_str::<V3ExtractionResult>(&repaired) {
            return Ok(r);
        }
    }
    // Schema patching: the model sometimes emits a well-formed but
    // incomplete wrapper — `{"items": [...]}` without the top-level fields,
    // a bare single item object, or a bare items array. Patch the missing
    // fields structurally, then re-validate.
    for candidate in schema_patch_candidates(&clean) {
        if let Ok(r) = serde_json::from_str::<V3ExtractionResult>(&candidate) {
            return Ok(r);
        }
    }
    // Last resort: scan the text for balanced top-level {...} blocks and try
    // each. Reasoning text leaked into the answer can contain arbitrary
    // braces, so the first-`{`-to-last-`}` clip in clean_json grabs a bogus
    // start; a balanced scan recovers the real JSON whenever it survived.
    // Every candidate must satisfy the full V3 schema, so junk candidates
    // (e.g. JSON examples quoted inside the reasoning text) are rejected.
    for candidate in balanced_json_candidates(&clean) {
        if let Ok(r) = serde_json::from_str::<V3ExtractionResult>(&candidate) {
            return Ok(r);
        }
    }
    // Truncation repair: with thinking disabled a huge session still yields
    // 15-20 KB of clean JSON that max_tokens may cut mid-array ("EOF while
    // parsing a list"). Close the document at several tail cut points —
    // usually the pristine cut already yields most items — and let serde
    // decide which candidate is well-formed and schema-complete.
    for candidate in truncated_close_candidates(&clean) {
        if let Ok(r) = serde_json::from_str::<V3ExtractionResult>(&candidate) {
            return Ok(r);
        }
    }
    // Schema-degradation recovery: some runs emit bare item objects (or a
    // bare items array) instead of the wrapped schema — often as several
    // concatenated objects ("trailing characters"). Collect every
    // item-shaped balanced block and wrap them into a valid result. If no
    // item can be recovered, the original error stands.
    if let Some(r) = degraded_item_extraction(&clean) {
        return Ok(r);
    }
    Err(anyhow::anyhow!(
        "AI JSON parse failed: {} (response length: {}, head: {:?})",
        // Report the SCHEMA error (e.g. "missing field `session_summary`"),
        // not the Value-parse error — a syntactically valid but
        // schema-incomplete response otherwise yields an empty message.
        serde_json::from_str::<V3ExtractionResult>(&clean)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default(),
        response.len(),
        crate::util::safe_preview(&clean, 120)
    ))
}

/// Patch well-formed JSON that skips parts of the V3 wrapper schema:
/// `{"items": [...]}` (top-level fields missing), a bare item object, or a
/// bare items array. Returns candidate JSON strings for schema validation.
fn schema_patch_candidates(clean: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(clean) else {
        return out;
    };
    match v {
        serde_json::Value::Object(ref map) if map.contains_key("items") => {
            let summary = map
                .get("session_summary")
                .cloned()
                .unwrap_or_else(|| serde_json::Value::String("（模型未提供会话摘要）".into()));
            let score = map
                .get("knowledge_score")
                .cloned()
                .unwrap_or_else(|| serde_json::json!(0.7));
            let worth = map
                .get("worth_extracting")
                .cloned()
                .unwrap_or_else(|| serde_json::json!(true));
            // Patch each item too — the final-stage model drops required
            // item fields (observed: "missing field `summary`").
            let items = match map.get("items") {
                Some(serde_json::Value::Array(arr)) => arr
                    .iter()
                    .filter_map(patch_item_value)
                    .collect::<Vec<_>>(),
                _ => vec![],
            };
            out.push(
                serde_json::json!({
                    "session_summary": summary,
                    "knowledge_score": score,
                    "worth_extracting": worth,
                    "items": items,
                })
                .to_string(),
            );
        }
        serde_json::Value::Object(ref map)
            if map.contains_key("title") && map.contains_key("content") =>
        {
            out.push(
                serde_json::json!({
                    "session_summary": "Schema 降级恢复：模型输出单个条目",
                    "knowledge_score": 0.7,
                    "worth_extracting": true,
                    "items": [v],
                })
                .to_string(),
            );
        }
        serde_json::Value::Array(ref arr) if !arr.is_empty() => {
            let items: Vec<serde_json::Value> =
                arr.iter().filter_map(patch_item_value).collect();
            if items.is_empty() {
                return out;
            }
            out.push(
                serde_json::json!({
                    "session_summary": "Schema 降级恢复：模型输出条目数组",
                    "knowledge_score": 0.7,
                    "worth_extracting": true,
                    "items": items,
                })
                .to_string(),
            );
        }
        _ => {}
    }
    out
}

/// Fill in required V3KnowledgeItem fields the model omitted (summary /
/// content / tags / confidence are skipped surprisingly often). None for
/// values that are not objects or carry no title at all — without a title
/// the item has no usable identity.
fn patch_item_value(v: &serde_json::Value) -> Option<serde_json::Value> {
    let obj = v.as_object()?;
    let take = |k: &str| obj.get(k).cloned().filter(|x| !x.is_null());
    let title = take("title")?;
    let non_empty_str = |val: Option<serde_json::Value>| {
        val.filter(|x| x.as_str().map(|s| !s.trim().is_empty()).unwrap_or(true))
    };
    let summary = non_empty_str(take("summary"))
        .or_else(|| non_empty_str(take("content")))
        .unwrap_or_else(|| title.clone());
    let content = non_empty_str(take("content"))
        .or_else(|| non_empty_str(take("summary")))
        .unwrap_or_else(|| title.clone());
    Some(serde_json::json!({
        "title": title,
        "category": take("category").unwrap_or_else(|| serde_json::json!("general")),
        "summary": summary,
        "content": content,
        "problem": take("problem"),
        "root_causes": take("root_causes").unwrap_or_else(|| serde_json::json!([])),
        "solutions": take("solutions").unwrap_or_else(|| serde_json::json!([])),
        "key_commands": take("key_commands"),
        "key_files": take("key_files"),
        "decisions": take("decisions"),
        "tags": take("tags").unwrap_or_else(|| serde_json::json!([])),
        "confidence": take("confidence").unwrap_or_else(|| serde_json::json!(0.7)),
    }))
}

/// Escalating repair attempts, tried in order after the pristine parse fails.
fn repair_attempts(clean: &str) -> Vec<String> {
    let smart = normalize_smart_quotes(clean);
    let mut attempts = Vec::new();
    // 1. Conservative: bare ASCII keys + trailing commas on the original text.
    attempts.push(repair_json(clean));
    // 2. Smart quotes normalized (curly “ ” ‘ ’ → straight) — CJK models emit
    //    these constantly and serde rejects them at the very first key.
    attempts.push(repair_json(&smart));
    // 3. Aggressive single→double quote flattening (mixed-quote documents).
    attempts.push(repair_json(&smart.replace('\'', "\"")));
    // 4. Lenient: Unicode bare keys (中文键名), fullwidth key colons, Python
    //    True/False/None literals.
    attempts.push(repair_json_lenient(&smart));
    attempts
}

/// Normalize curly quotes to straight ASCII quotes. Only used inside the
/// repair path (validated afterwards), never on the pristine parse.
fn normalize_smart_quotes(s: &str) -> String {
    s.replace('\u{201c}', "\"") // “
        .replace('\u{201d}', "\"") // ”
        .replace('\u{2018}', "'")  // ‘
        .replace('\u{2019}', "'")  // ’
}

fn clean_json(s: &str) -> String {
    // Qwen3-style thinking models may wrap answers in <think>...</think>.
    let s = strip_think_blocks(s);
    // Leading BOM / zero-width characters survive trim() and break the parse.
    let s = s.trim_start_matches(['\u{feff}', '\u{200b}', '\u{200c}', '\u{200d}']);
    let s = s.trim();
    let s = if s.starts_with("```") {
        let after = s.trim_start_matches('`').trim_start_matches("json").trim_start_matches('\n');
        if let Some(end) = after.rfind("```") { &after[..end] } else { after }
    } else { s };
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        s[start..=end].to_string()
    } else { s.to_string() }
}

fn strip_think_blocks(s: &str) -> String {
    let mut out = s.to_string();
    loop {
        let has_open = out.contains("<think>");
        let close_rel = out.find("</think>");
        match (has_open, close_rel) {
            (false, None) => break,
            (true, _) => {
                let start = out.find("<think>").expect("checked above");
                if let Some(end_rel) = out[start..].find("</think>") {
                    let end = start + end_rel + "</think>".len();
                    out = format!("{}{}", &out[..start], &out[end..]);
                } else {
                    // Unclosed think block: drop everything up to it and keep the tail.
                    out = out[start + "<think>".len()..].to_string();
                    break;
                }
            }
            // Bare closing tag with no opener: vLLM chat templates often
            // swallow the opening <think> and leave only `</think>`. Drop
            // the whole leaked reasoning prefix before the tag.
            (false, Some(close)) => {
                out = out[close + "</think>".len()..].to_string();
            }
        }
    }
    out
}

/// Collect balanced `{...}` substrings, one per candidate start. A leaked
/// reasoning prefix whose bogus `{` never closes before the real JSON means
/// the top-level blocks share a closing brace — so EVERY `{` (not just
/// top-level ones) must be tried as a start; bogus candidates simply fail
/// schema validation downstream. String/escape state is tracked so braces
/// inside values never miscount. Starts are capped: at 15 KB responses the
/// first 64 openings are far more than enough and keep the scan O(starts·n).
fn balanced_json_candidates(s: &str) -> Vec<String> {
    const MAX_STARTS: usize = 64;
    let bytes = s.as_bytes();
    let mut candidates = Vec::new();
    let mut starts = 0;
    for start in 0..bytes.len() {
        if bytes[start] != b'{' {
            continue;
        }
        starts += 1;
        if starts > MAX_STARTS {
            break;
        }
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let mut end = None;
        for (offset, &b) in bytes[start..].iter().enumerate() {
            let j = start + offset;
            if in_string {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    in_string = false;
                }
                continue;
            }
            match b {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(j);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(e) = end {
            candidates.push(s[start..=e].to_string());
        }
    }
    candidates
}

/// Build closing candidates for a JSON document truncated by max_tokens.
///
/// Cut points tried: the full text first (close exactly where it stopped),
/// then trailing `{`/`[`/`,` boundaries walking backwards over the whole
/// text — a partially emitted item has no complete field set, so the scan
/// must be able to reach far enough back to close `"items":[` into an empty
/// array and keep the top-level object schema-complete. Each candidate is
/// schema-validated by the caller; the deepest cut that parses wins the most
/// items for that run.
fn truncated_close_candidates(s: &str) -> Vec<String> {
    const MAX_CUTS: usize = 24;
    let bytes = s.as_bytes();
    let mut cuts = vec![bytes.len()];
    for i in (0..bytes.len()).rev() {
        match bytes[i] {
            b',' | b'{' | b'[' => cuts.push(i + 1),
            _ => {}
        }
        if cuts.len() >= MAX_CUTS {
            break;
        }
    }
    let mut candidates = Vec::new();
    for &cut in &cuts {
        if let Some(closed) = close_truncated_json(&bytes[..cut]) {
            candidates.push(closed);
        }
    }
    candidates
}

/// Close an unterminated JSON prefix: finish any open string, supply a value
/// when the text ends mid-key/value, drop a trailing comma, then close all
/// open brackets in order. Returns None when no object is open at all.
fn close_truncated_json(bytes: &[u8]) -> Option<String> {
    let mut stack: Vec<u8> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut prev = b'\0';
    // What the open string was preceded by: ':' → a value; '{'/',' → a key.
    let mut string_after = b'\0';
    for &b in bytes {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else {
            match b {
                b'"' => {
                    in_string = true;
                    string_after = prev;
                }
                b'{' | b'[' => stack.push(b),
                b'}' | b']' => {
                    stack.pop()?;
                }
                _ => {}
            }
        }
        if !b.is_ascii_whitespace() {
            prev = b;
        }
    }
    if stack.is_empty() {
        return None; // nothing open — the text isn't a truncated object
    }
    let mut out = String::from_utf8_lossy(bytes).into_owned();
    if in_string {
        // A dangling escape would corrupt the closing quote we append.
        while out.ends_with('\\') {
            out.pop();
        }
        out.push('"');
        if string_after == b'{' || string_after == b',' {
            // The string we just closed was an (incomplete) object key.
            out.push_str(": null");
        }
    } else if prev == b':' {
        out.push_str("null");
    } else if prev == b',' {
        while out.ends_with(',') {
            out.pop();
        }
    } else if prev == b'"' {
        // A complete string just closed at EOF: it was either an object key
        // (needs a value) or an array element (already fine).
        if let Some(&top) = stack.last() {
            if top == b'{' {
                out.push_str(": null");
            }
        }
    }
    while let Some(open) = stack.pop() {
        out.push(if open == b'{' { '}' } else { ']' });
    }
    Some(out)
}

/// Collect item-shaped balanced blocks from a response that skipped the
/// top-level wrapper. Handles a single bare item, a bare items array, and
/// concatenated item objects; items missing required fields are patched
/// first. Returns None when nothing item-shaped parses.
fn degraded_item_extraction(clean: &str) -> Option<V3ExtractionResult> {
    let smart = normalize_smart_quotes(clean);
    let mut items = Vec::new();
    for candidate in balanced_json_candidates(&smart) {
        if let Ok(item) = serde_json::from_str::<V3KnowledgeItem>(&candidate) {
            items.push(item);
            continue;
        }
        // Item missing required fields — patch and retry.
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&candidate) {
            if let Some(patched) = patch_item_value(&v) {
                if let Ok(item) = serde_json::from_value::<V3KnowledgeItem>(patched) {
                    items.push(item);
                    continue;
                }
            }
        }
        if let Ok(arr) = serde_json::from_str::<Vec<V3KnowledgeItem>>(&candidate) {
            items.extend(arr);
        }
    }
    if items.is_empty() {
        return None;
    }
    Some(V3ExtractionResult {
        session_summary: "Schema 降级恢复：模型输出缺少顶层包装，按条目逐一回收".to_string(),
        // Substantive items were recovered; keep the score above the default
        // min_knowledge_score (0.6) so the degraded haul is not discarded.
        knowledge_score: 0.7,
        worth_extracting: true,
        items,
    })
}

/// Best-effort repair of near-JSON emitted by small local models:
/// - quote unquoted ASCII object keys: `{foo: 1}` → `{"foo": 1}`
/// - remove trailing commas: `[1, 2,]` → `[1, 2]`
/// - convert single-quoted strings when the text has no double quotes at all
fn repair_json(s: &str) -> String {
    use regex::Regex;
    let mut out = s.to_string();

    // Only convert single quotes when double quotes are absent (ambiguous otherwise).
    if !out.contains('"') && out.contains('\'') {
        out = out.replace('\'', "\"");
    }

    if let Ok(re) = Regex::new(r#"([\{,]\s*)([A-Za-z_][A-Za-z0-9_\-]*)\s*:"#) {
        out = re.replace_all(&out, "$1\"$2\":").to_string();
    }

    if let Ok(re) = Regex::new(r#",\s*([\}\]])"#) {
        out = re.replace_all(&out, "$1").to_string();
    }

    out
}

/// Extra-lenient pass for non-ASCII mistakes the conservative repair misses:
/// - Chinese/Unicode bare keys: `{总结: "..."}` → `{"总结": "..."}`
/// - fullwidth key colon: `{键： 1}` → `{"键": 1}` (prose colons inside string
///   values may be mangled, but every attempt is validated before use)
/// - Python literals: `True/False/None` → `true/false/null`
fn repair_json_lenient(s: &str) -> String {
    use regex::Regex;
    let mut out = s.to_string();

    if let Ok(re) = Regex::new(r#"(:\s*)(True|False|None)\b"#) {
        out = re
            .replace_all(&out, |caps: &regex::Captures| {
                match &caps[2] {
                    "True" => format!("{}true", &caps[1]),
                    "False" => format!("{}false", &caps[1]),
                    _ => format!("{}null", &caps[1]),
                }
            })
            .to_string();
    }

    if let Ok(re) = Regex::new(r#"([\{,]\s*)(\p{L}[\p{L}\p{N}_\-]*)\s*[：:]"#) {
        out = re.replace_all(&out, "$1\"$2\":").to_string();
    }

    out = repair_json(&out);
    out
}

fn log_ai_request(
    db: &StateDb,
    pipeline_run_id: &str,
    stage: &str,
    model: &str,
    endpoint: &str,
    input_count: usize,
    output_count: usize,
    latency_ms: i64,
    success: bool,
) {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let status = if success { "SUCCESS" } else { "FAILED" };
    let _ = db.conn().execute(
        "INSERT INTO ai_request_log (id, pipeline_run_id, stage, model, endpoint, input_tokens, output_tokens, latency_ms, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            id, pipeline_run_id, stage, model, endpoint,
            input_count as i64, output_count as i64, latency_ms, status, now
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_v3_response() {
        let json = r#"{
            "session_summary": "Test session",
            "knowledge_score": 0.85,
            "worth_extracting": true,
            "items": [
                {
                    "title": "Test Item",
                    "category": "troubleshooting",
                    "summary": "Test summary",
                    "content": "Test content",
                    "problem": null,
                    "root_causes": ["cause1"],
                    "solutions": ["solution1"],
                    "key_commands": null,
                    "key_files": null,
                    "decisions": null,
                    "tags": ["rust", "test"],
                    "confidence": 0.9
                }
            ]
        }"#;
        let result = parse_v3_result_typed(json).unwrap();
        assert!(result.worth_extracting);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].title, "Test Item");
    }

    #[test]
    fn parse_skip_response() {
        let json = r#"{"session_summary":"trivial","knowledge_score":0.3,"worth_extracting":false,"items":[]}"#;
        let result = parse_v3_result_typed(json).unwrap();
        assert!(!result.worth_extracting);
        assert!(result.items.is_empty());
    }

    #[test]
    fn parse_json_with_fences() {
        let json = "```json\n{\"session_summary\":\"x\",\"knowledge_score\":0.5,\"worth_extracting\":false,\"items\":[]}\n```";
        let result = parse_v3_result_typed(json).unwrap();
        assert!(!result.worth_extracting);
    }

    /// R11: non-JSON CJK responses must produce an error, not panic.
    #[test]
    fn parse_cjk_garbage_is_error_not_panic() {
        let garbage = "中".repeat(40);
        assert!(parse_v3_result_typed(&garbage).is_err());
    }

    /// Qwen-style unquoted keys must be repaired, not failed.
    #[test]
    fn parse_repairs_unquoted_keys() {
        let json = r#"{session_summary:"x", knowledge_score:0.5, worth_extracting:false, items:[]}"#;
        let result = parse_v3_result_typed(json).unwrap();
        assert!(!result.worth_extracting);
    }

    /// Trailing commas and <think> wrappers must not break parsing.
    #[test]
    fn parse_repairs_trailing_comma_and_think_block() {
        let json = "<think>让我想想……这个会话价值不高。</think>\n\
            {\"session_summary\":\"x\",\"knowledge_score\":0.4,\"worth_extracting\":false,\"items\":[,]}";
        let result = parse_v3_result_typed(json).unwrap();
        assert!(!result.worth_extracting);
    }

    /// Single-quoted JSON (no double quotes anywhere) is repaired.
    #[test]
    fn parse_repairs_single_quotes() {
        let json = "{'session_summary':'x','knowledge_score':0.4,'worth_extracting':false,'items':[]}";
        let result = parse_v3_result_typed(json).unwrap();
        assert!(!result.worth_extracting);
    }

    /// Smart quotes (curly “ ”) break serde at the first key — the exact
    /// "key must be a string at line 1 column 2" failure from Qwen output.
    #[test]
    fn parse_repairs_smart_quotes() {
        let json = "{\u{201c}session_summary\u{201d}:\u{201c}x\u{201d},\u{201c}knowledge_score\u{201d}:0.4,\u{201c}worth_extracting\u{201d}:false,\u{201c}items\u{201d}:[]}";
        let result = parse_v3_result_typed(json).unwrap();
        assert_eq!(result.session_summary, "x");
        assert!(!result.worth_extracting);
    }

    /// Mixed single/double quotes: keys single-quoted while some string value
    /// uses double quotes — the conservative no-double-quote rule must not
    /// fire, the aggressive flattening pass has to save it.
    #[test]
    fn parse_repairs_mixed_quotes() {
        let json = "{'session_summary':'x',\"knowledge_score\":0.4,'worth_extracting':false,'items':[]}";
        let result = parse_v3_result_typed(json).unwrap();
        assert_eq!(result.session_summary, "x");
        assert!(!result.worth_extracting);
    }

    /// Chinese bare keys + Python literals + trailing comma.
    #[test]
    fn parse_repairs_unicode_keys_and_python_literals() {
        let json = "{session_summary:\"x\", knowledge_score:0.4, worth_extracting:True, items:[],}";
        let result = parse_v3_result_typed(json).unwrap();
        assert_eq!(result.session_summary, "x");
        assert!(result.worth_extracting);
        assert!(result.items.is_empty());
    }

    /// A leading BOM must not break the parse.
    #[test]
    fn parse_tolerates_leading_bom() {
        let json = "\u{feff}{\"session_summary\":\"x\",\"knowledge_score\":0.4,\"worth_extracting\":false,\"items\":[]}";
        let result = parse_v3_result_typed(json).unwrap();
        assert!(!result.worth_extracting);
    }

    /// Failed parses include a sanitized response head for diagnosis.
    #[test]
    fn parse_failure_includes_head_preview() {
        let garbage = "{\"aaaa\": ".to_string() + &"b".repeat(400);
        let err = parse_v3_result_typed(&garbage).unwrap_err().to_string();
        assert!(err.contains("head:"), "error should carry a head preview: {err}");
    }

    /// vLLM chat templates often swallow the opening <think> tag and leave a
    /// bare `</think>` — the reasoning prefix must be dropped, not glued into
    /// the clipped JSON. (Observed live: reasoning...</think>\n\n{...}.)
    #[test]
    fn parse_strips_bare_closing_think_tag() {
        let json = "用户要求输出严格 JSON。需要只输出 JSON。计算 1+1=2。\n</think>\n\n\
            {\"session_summary\":\"x\",\"knowledge_score\":0.4,\"worth_extracting\":false,\"items\":[]}";
        let result = parse_v3_result_typed(json).unwrap();
        assert!(!result.worth_extracting);
    }

    /// The observed production failure: the model starts its answer with a
    /// bogus `{` (to honor "first char must be {"), then reasons (no think
    /// tags when reasoning is truncated mid-stream), and the real JSON
    /// follows. The bogus brace and the real object share a closing brace,
    /// so every `{` opening must be tried as a scan start.
    #[test]
    fn parse_recovers_json_after_leaked_reasoning_with_braces() {
        let real = r#"{"session_summary":"真实摘要","knowledge_score":0.9,"worth_extracting":true,"items":[{"title":"T","category":"general","summary":"s","content":"c","problem":null,"root_causes":[],"solutions":[],"key_commands":null,"key_files":null,"decisions":null,"tags":[],"confidence":1.0}]}"#;
        let leaked = format!(
            "{{。用户最后说\"输出完整 JSON（同上述 Schema）\"。这里可能指 {{chunk_index}} 与 {{topics}} 字段。\n\n{}",
            real
        );
        let result = parse_v3_result_typed(&leaked).unwrap();
        assert!(result.worth_extracting);
        assert_eq!(result.session_summary, "真实摘要");
    }

    /// Braces inside string values must not confuse the balanced scan.
    #[test]
    fn parse_balanced_scan_ignores_braces_in_strings() {
        let real = r#"{"session_summary":"含 { 花括 } 的摘要","knowledge_score":0.5,"worth_extracting":false,"items":[]}"#;
        let leaked = "推理文本 { 假括号。 }} 继续推理\n\n".to_string() + real;
        let result = parse_v3_result_typed(&leaked).unwrap();
        assert!(!result.worth_extracting);
        assert_eq!(result.session_summary, "含 { 花括 } 的摘要");
    }

    /// Qwen3 soft switch must sit at the END of the user message.
    #[test]
    fn v3_prompts_end_with_no_think_suffix() {
        use crate::ai::prompts_v3::{make_v3_chunk_prompt, make_v3_extraction_prompt, make_v3_final_prompt};
        assert!(make_v3_extraction_prompt("内容").ends_with("/no_think"));
        assert!(make_v3_chunk_prompt("内容", 0, 2).ends_with("/no_think"));
        assert!(make_v3_final_prompt("标题", None, &["摘要".into()]).ends_with("/no_think"));
    }

    /// max_tokens truncation mid-array (observed live: "EOF while parsing a
    /// list at line 216", 14.7 KB of otherwise-clean JSON) — closing the
    /// document must recover the items emitted before the cut.
    #[test]
    fn parse_closes_truncated_json() {
        let full = r#"{"session_summary":"大数据集分析","knowledge_score":0.9,"worth_extracting":true,"items":[
            {"title":"条目一","category":"architecture","summary":"s1","content":"c1","problem":null,"root_causes":[],"solutions":[],"key_commands":null,"key_files":null,"decisions":null,"tags":[],"confidence":0.9},
            {"title":"条目二","category":"troubleshooting","summary":"s2","content":"c2","problem":null,"root_causes":[],"solutions":[],"key_commands":null,"key_files":null,"decisions":null,"tags":[],"confidence":0.8}"#;
        let result = parse_v3_result_typed(full).unwrap();
        assert!(result.worth_extracting);
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].title, "条目一");
    }

    /// Truncation in the middle of a string value must also close cleanly.
    /// The half-emitted item cannot satisfy the item schema (missing later
    /// required fields), so the scan falls back to closing `"items":[]` —
    /// top-level data survives and the run is no longer a total failure.
    #[test]
    fn parse_closes_truncated_inside_string() {
        let truncated = r#"{"session_summary":"对 dataset-eval 项目进行了全面的分析。","knowledge_score":0.9,"worth_extracting":true,"items":[{"title":"条目A","category":"architecture","summary":"内容到这里被截断"#;
        let result = parse_v3_result_typed(truncated).unwrap();
        assert!(result.worth_extracting);
        assert_eq!(result.session_summary, "对 dataset-eval 项目进行了全面的分析。");
        assert!(result.items.is_empty());
    }

    /// Truncation after several complete items keeps every complete item —
    /// the boundary cut lands on the inter-item comma.
    #[test]
    fn parse_keeps_complete_items_before_truncation() {
        let item = |t: &str| format!(
            r#"{{"title":"{}","category":"general","summary":"s","content":"c","problem":null,"root_causes":[],"solutions":[],"key_commands":null,"key_files":null,"decisions":null,"tags":[],"confidence":0.9}}"#, t
        );
        let truncated = format!(
            r#"{{"session_summary":"多条目","knowledge_score":0.9,"worth_extracting":true,"items":[{},{},"#,
            item("甲"),
            item("乙")
        );
        let result = parse_v3_result_typed(&truncated).unwrap();
        assert!(result.worth_extracting);
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].title, "甲");
        assert_eq!(result.items[1].title, "乙");
    }

    /// Observed live (session 19): the model emitted bare item objects
    /// concatenated without any wrapper — "trailing characters" errors.
    /// Each item must be recovered individually.
    #[test]
    fn parse_recovers_bare_concatenated_items() {
        let item = |t: &str| format!(
            r#"{{"title":"{}","category":"implementation","summary":"s{}","content":"c{}","problem":null,"root_causes":[],"solutions":[],"key_commands":null,"key_files":null,"decisions":null,"tags":["t"],"confidence":0.85}}"#, t, t, t
        );
        let leaked = format!("{}\n\n{}\n\n{}", item("一"), item("二"), item("三"));
        let result = parse_v3_result_typed(&leaked).unwrap();
        assert!(result.worth_extracting);
        assert_eq!(result.items.len(), 3);
        assert_eq!(result.items[2].title, "三");
    }

    /// A bare items array (no wrapper) is also recovered.
    #[test]
    fn parse_recovers_bare_items_array() {
        let arr = r#"[{"title":"甲","category":"general","summary":"s","content":"c","problem":null,"root_causes":[],"solutions":[],"key_commands":null,"key_files":null,"decisions":null,"tags":[],"confidence":0.9},{"title":"乙","category":"decision","summary":"s","content":"c","problem":null,"root_causes":[],"solutions":[],"key_commands":null,"key_files":null,"decisions":null,"tags":[],"confidence":0.9}]"#;
        let result = parse_v3_result_typed(arr).unwrap();
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[1].title, "乙");
    }

    /// Observed live (sessions 390/386): the model emits a well-formed
    /// `{"items": [...]}` but omits the top-level wrapper fields — the
    /// patcher must fill them in and keep every item.
    #[test]
    fn parse_patches_wrapper_missing_top_level_fields() {
        let json = r#"{"items":[{"title":"长音频编辑器架构","category":"architecture","summary":"s","content":"c","problem":null,"root_causes":[],"solutions":[],"key_commands":null,"key_files":null,"decisions":null,"tags":["audio"],"confidence":0.85}]}"#;
        let result = parse_v3_result_typed(json).unwrap();
        assert!(result.worth_extracting);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].title, "长音频编辑器架构");
        assert_eq!(result.knowledge_score, 0.7);
    }

    /// A schema-incomplete response must carry the missing-field error, not
    /// an empty serde message.
    #[test]
    fn parse_failure_reports_missing_field() {
        let json = r#"{"unrelated": true}"#;
        let err = parse_v3_result_typed(json).unwrap_err().to_string();
        assert!(err.contains("missing field"), "error should name the missing field: {err}");
    }

    /// Observed live (sessions 370/366/354/348/342): final-stage items drop
    /// required fields (`summary`/`content`) because the final prompt did
    /// not spell out the schema. The patcher must fill them and keep the
    /// items.
    #[test]
    fn parse_patches_items_missing_required_fields() {
        let json = r#"{"items":[
            {"title":"条目缺摘要","category":"implementation","problem":"问题","root_causes":[],"solutions":[],"key_commands":null,"key_files":null,"decisions":null,"tags":["t"],"confidence":0.8},
            {"title":"条目缺内容","category":"architecture","summary":"有摘要没内容","problem":null,"root_causes":[],"solutions":[],"key_commands":null,"key_files":null,"decisions":null,"tags":[],"confidence":0.9}
        ]}"#;
        let result = parse_v3_result_typed(json).unwrap();
        assert!(result.worth_extracting);
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].summary, "条目缺摘要"); // falls back to title
        assert_eq!(result.items[1].content, "有摘要没内容"); // falls back to summary
    }
}
