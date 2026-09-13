# Reference Analysis: Gemini CLI

## Repository

- Name: gemini-cli
- URL: https://github.com/google-gemini/gemini-cli
- Commit SHA: ed2ac40df67a319bf348bd7e3d10494696b31b38
- Checked Date: 2026-09-11
- License: Apache 2.0

## Why This Repository Matters

google-gemini/gemini-cli is the source of truth for Gemini session file format. AICoder Session Viewer's gemini.rs provider was adapted for AIKS.

## Session/Data Location

```text
~/.gemini/tmp/{project-encoded}/chats/session-*.json
~/.gemini/tmp/{project-encoded}/.project_root  (contains real project path)
```

## Data Model / Schema

Each session file is a JSON object:
```json
{
  "sessionId": "...",
  "startTime": "2024-03-10T14:00:00Z",
  "messages": [...]
}
```

Message format:
```json
{
  "type": "user" | "gemini" | "info" | "error",
  "id": "...",
  "content": "[string for gemini] | [{text,...} for user]",
  "toolCalls": [{id, name, args, result: [{functionResponse: {response: {output|error}}}]}],
  "thoughts": [{subject, description}],
  "tokens": {input, output, cached, thoughts, tool, total},
  "timestamp": "...",
  "model": "..."
}
```

## AIKS Decisions

1. Read `.project_root` to get real project path.
2. Tool results are embedded inside toolCalls array (inline, not separate messages).
3. Thinking blocks from `thoughts` array.
4. Parser version: `gemini-json-v1`

## Attribution Required

- Parsing approach from AICoder Session Viewer (MIT)
- Original file: `references/aicoder-session-viewer/src-tauri/src/providers/gemini.rs`
- AIKS derived file: `src/providers/gemini.rs`
