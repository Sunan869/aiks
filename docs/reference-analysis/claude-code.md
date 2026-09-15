# Reference Analysis: Claude Code

## Repository

- Name: claude-code
- URL: https://github.com/anthropics/claude-code
- Commit SHA: N/A (claude-code is not a typical open-source repo with source available)
- Checked Date: 2026-09-11
- License: Anthropic proprietary

## Why This Repository Matters

Claude Code is the source of truth for the JSONL session file format. AICoder Session Viewer's claude.rs provider was cross-referenced against actual Claude Code session files on disk.

## Session/Data Location

```text
~/.claude/projects/{project-encoded-path}/{session-uuid}.jsonl
~/.claude/projects/{project-encoded-path}/{session-uuid}/subagents/agent-{id}.jsonl
```

Project path encoding: `-` replaces `/` in the path, prefixed with `-`.
Example: `/home/alice/project` → `-home-alice-project`
Windows: `C:/Users/alice/project` → `-C-Users-alice-project`

## Data Model / Schema

Each JSONL line is one of:

```json
// User message
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"..."}]},"uuid":"...","sessionId":"...","cwd":"/path","timestamp":"2024-01-01T00:00:00.000Z","parentUuid":null}

// Assistant message
{"type":"assistant","message":{"role":"assistant","content":[...],"model":"claude-opus-4-5","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":20,"cache_creation_input_tokens":10}},"uuid":"...","sessionId":"...","cwd":"/path","timestamp":"...","parentUuid":"..."}

// Summary
{"type":"summary","summary":"...","leafUuid":"..."}

// Progress (subagent)
{"type":"progress","parentToolUseID":"...","data":{"agentId":"..."}}
```

Content block types:
- `text`: `{type, text}`
- `tool_use`: `{type, id, name, input}`
- `tool_result`: `{type, tool_use_id, content, is_error}`
- `thinking`: `{type, thinking}`
- `image`: `{type, source: {data, media_type}}`

## Parsing Flow

```text
~/.claude/projects/**/*.jsonl (main session files only)
  ↓ Two-pass parse:
    Pass 1: collect tool_use_id → agent_id from progress events
    Pass 2: parse user/assistant/system messages
  ↓ content blocks: Text | ToolCall | ToolResult | Thinking | Image | Unknown
  ↓ NormalizedMessage[]
  ↓ NormalizedSession
```

## Reusable Code

The AICoder Session Viewer claude.rs was used as primary source (MIT licensed).

## AIKS Decisions

1. Scan only main session JSONL files (exclude `subagents/` directory for now).
2. Dir name decoding handles Windows drive letters.
3. content hash uses `input_tokens + cache_read_input_tokens + cache_creation_input_tokens` as total input.
4. Parser version: `claude-v1`

## Attribution Required

- Parsing approach from AICoder Session Viewer (MIT)
- Original file: `references/aicoder-session-viewer/src-tauri/src/providers/claude.rs`
- AIKS derived file: `crates/aiks-core/src/providers/claude.rs`
