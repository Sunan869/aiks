# Reference Analysis: AICoder Session Viewer

## Repository

- Name: aicoder-session-viewer
- URL: https://github.com/seastart/aicoder-session-viewer
- Commit SHA: b750098594c3bb969d5039a9f2e44a283c38af9b
- Checked Date: 2026-09-11
- License: MIT

## Why This Repository Matters

AICoder Session Viewer is a Rust/Tauri desktop app providing Providers for Claude Code, Codex, Gemini CLI, and OpenCode in Rust. It is the **PRIMARY source** for all four AIKS Providers. The parser logic, data model, and path discovery patterns are directly adapted from this repository.

## Relevant Files

| File/Directory | Purpose |
|---|---|
| `src-tauri/src/providers/claude.rs` | Claude Code JSONL parser and discovery |
| `src-tauri/src/providers/codex.rs` | Codex JSONL + SQLite parser and discovery |
| `src-tauri/src/providers/gemini.rs` | Gemini CLI JSON parser and discovery |
| `src-tauri/src/providers/opencode.rs` | OpenCode SQLite parser (schema as of this commit) |
| `src-tauri/src/models.rs` | ToolKind, SessionSummary, Message, ContentBlock, Role |
| `src-tauri/src/providers/mod.rs` | ProviderRegistry, SessionProvider trait |

## Session/Data Location

```text
Claude Code:   ~/.claude/projects/{project-hash}/{uuid}.jsonl
Codex:         ~/.codex/sessions/{Y}/{M}/{D}/rollout-*.jsonl
               ~/.codex/state_5.sqlite (threads table for fast listing)
Gemini CLI:    ~/.gemini/tmp/{project}/chats/session-*.json
OpenCode:      ~/.local/share/opencode/opencode.db
```

## Data Model / Schema

See individual reference analysis files for each provider.

Key AICoder models adapted in AIKS:
- `ContentBlock::Text|ToolUse|ToolResult|Thinking|Image`
- `Role::User|Assistant|System|Tool`
- `TokenUsage` with cache fields

## Parsing Flow

```text
File system / SQLite
  ↓ scan (list_sessions / discover_sessions)
  ↓ parse file/db (get_session / load_session)
  ↓ ContentBlock normalization
  ↓ SessionSummary / NormalizedSession
```

## Reusable Code

Directly adapted into AIKS:
- `src/providers/claude.rs` → Claude JSONL parser, mtime summary cache, agent_id injection
- `src/providers/codex.rs` → Codex JSONL parser, dual-version (user_message / item_completed), SQLite fast path
- `src/providers/gemini.rs` → Gemini JSON parser, .project_root discovery
- `src/providers/opencode.rs` → OpenCode SQLite parser (updated for live schema)

## Reference-Only Code

- Tauri IPC commands (`commands.rs`) — not applicable to AIKS CLI
- Tauri app configuration — not applicable
- Frontend (Vue/TS) — not applicable
- `providers/search.rs` — AIKS does not implement in-memory search

## Do Not Reuse

- Frontend rendering
- Session search UI
- Tauri build pipeline
- Antigravity provider (undocumented tool, not in scope for V1)

## Compatibility Risks

- OpenCode schema has diverged: live DB uses `{callID, tool, state}` for tool parts, not `{toolName, toolCallId, args}` as AICoder's opencode.rs assumes. AIKS was updated to match actual live schema.
- Codex: dual user_message format (≤0.146 vs ≥0.147) handled in both codex.rs implementations.
- Gemini: uses `.project_root` file to map encoded dir names to real paths.

## AIKS Decisions

1. Adapted all four providers from AICoder, maintaining same algorithmic logic.
2. AIKS wraps providers with `async_trait` per design doc spec.
3. OpenCode provider updated for actual live schema (confirmed 2026-09-11).
4. ToolCall and ToolResult now use AIKS canonical types, not AICoder's `ContentBlock`.

## Attribution Required

- Original files: `src-tauri/src/providers/{claude,codex,gemini,opencode}.rs`, `src-tauri/src/models.rs`
- AIKS derived files: `src/providers/{claude,codex,gemini,opencode}.rs`
- License notice required: MIT
