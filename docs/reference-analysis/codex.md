# Reference Analysis: Codex

## Repository

- Name: codex
- URL: https://github.com/openai/codex
- Commit SHA: eaa8b6d91701d6cabe464141facc677e5915fbfc
- Checked Date: 2026-09-11
- License: Apache 2.0

## Why This Repository Matters

openai/codex is the source of truth for the Codex session file format. AICoder Session Viewer's codex.rs provider was adapted for AIKS.

## Session/Data Location

```text
~/.codex/sessions/{YYYY}/{MM}/{DD}/rollout-{id}.jsonl
~/.codex/state_5.sqlite  (threads table: fast session listing with title/cwd/tokens)
```

## Data Model / Schema

Each JSONL line: `{"timestamp":"...", "type":"...", "payload":{...}}`

Key event types:
- `session_meta`: `payload.cwd` = working directory
- `event_msg`: `payload.type` = "user_message" (≤0.146) | "item_completed" | "error" | "token_count"
- `response_item`: `payload.type` = "message" | "function_call" | "function_call_output" | "custom_tool_call" | "custom_tool_call_output" | "web_search_call"

User message format evolution:
- **≤0.146**: `event_msg.user_message.message` (string)
- **≥0.147**: `event_msg.item_completed.item.type == "UserMessage"`, content array

State SQLite `threads` table:
```sql
id, title, cwd, created_at, updated_at, tokens_used, rollout_path, archived
```

## AIKS Decisions

1. Try SQLite fast path first, fall back to JSONL scan.
2. Lock user_message source to first format seen (UserMsgSource enum) to avoid duplicates.
3. Parser version: `codex-v1`
4. Custom tool call input is kept as raw string (JS code).

## Attribution Required

- Parsing approach from AICoder Session Viewer (MIT)
- Original file: `references/aicoder-session-viewer/src-tauri/src/providers/codex.rs`
- AIKS derived file: `crates/aiks-core/src/providers/codex.rs`
