# Reference Analysis: OpenCode

## Repository

- Name: opencode
- URL: https://github.com/anomalyco/opencode
- Commit SHA: 193de13a88d62a6409c6d385831180f1def527dc
- Checked Date: 2026-09-11
- License: MIT (see packages/opencode/package.json)

## Why This Repository Matters

OpenCode is the AI coding tool the AIKS primary user uses. Its SQLite schema is the authoritative source for `OpenCodeProvider`. The schema is more complex than initially documented — contains session, message, part, project tables with JSON fields.

## Relevant Files

| File/Directory | Purpose |
|---|---|
| `packages/opencode/migration/20260511173437_session-metadata/migration.sql` | Latest migration: adds `metadata` column to session |
| `packages/opencode/src/` | TypeScript source (schema reference) |
| `packages/console/core/migrations/` | Console migrations (not relevant for session data) |

## Session/Data Location

```text
# Linux / macOS
~/.local/share/opencode/opencode.db

# Windows (OpenCode uses Unix-style paths even on Windows)  
%USERPROFILE%\.local\share\opencode\opencode.db
```

## Data Model / Schema

Verified from live database inspection on 2026-09-11:

```sql
-- Core session
CREATE TABLE session (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    directory TEXT,
    title TEXT NOT NULL,
    version TEXT NOT NULL,
    time_created INTEGER NOT NULL,   -- Unix milliseconds
    time_updated INTEGER,            -- Unix milliseconds
    model TEXT,
    time_archived INTEGER,
    ...
);

-- Per-session message  
CREATE TABLE message (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    time_created INTEGER NOT NULL,   -- Unix milliseconds
    time_updated INTEGER NOT NULL,
    data TEXT NOT NULL               -- JSON: {role, time, modelID, providerID, ...}
);

-- Message content parts
CREATE TABLE part (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    time_created INTEGER NOT NULL,
    time_updated INTEGER NOT NULL,
    data TEXT NOT NULL               -- JSON: {type, ...}
);

-- Project
CREATE TABLE project (
    id TEXT PRIMARY KEY,
    worktree TEXT NOT NULL,          -- Working directory
    name TEXT,
    time_created INTEGER NOT NULL,
    time_updated INTEGER NOT NULL,
    ...
);
```

### Part data.type values (from live DB)

| type | Description | Key fields |
|------|-------------|------------|
| `text` | Text content | `{text: string}` |
| `tool` | Tool call + result | `{callID, tool, state: {status, input, output, raw}}` |
| `reasoning` | Extended thinking | `{text: string}` |
| `file` | File attachment | `{mime, url}` |
| `step-start` | Step delimiter (skip) | - |
| `step-finish` | Step delimiter (skip) | - |
| `compaction` | Context compaction (skip) | - |
| `patch` | File patch (skip) | - |

### Message data fields

- `role`: "user" | "assistant" | "system"
- `modelID`: model identifier string
- `providerID`: provider identifier
- `time.created`: Unix milliseconds
- `parentID`: parent message ID

## Parsing Flow

```text
SQLite (read-only, WAL-aware)
  ↓ JOIN session + project
  ↓ Load messages by session_id ORDER BY time_created
  ↓ Load parts by session_id (avoid N+1)
  ↓ Group parts by message_id
  ↓ Parse part.data JSON → ContentBlock
  ↓ tool parts → ToolCall + ToolResult (if completed)
  ↓ NormalizedMessage[]
  ↓ NormalizedSession
```

## Reusable Code

- OpenCode schema discovery from sqlite_master
- Part data parsing logic (adapted in `crates/aiks-core/src/providers/opencode.rs`)
- WAL-aware read-only connection pattern

## Reference-Only Code

- TypeScript source: used only to understand schema evolution
- Console/stats packages: not relevant to session data

## Do Not Reuse

- Any OpenCode API code
- Console/web UI code  
- Account/authentication logic

## Compatibility Risks

- Schema evolves via drizzle migrations — future versions may change part types or add new message types
- `model` column on session was added in a migration (already present in current live DB)
- `session_message` table exists alongside `message` but is used for different purposes (model-switched events); AIKS reads `message`, not `session_message`
- WAL must be handled: open with `SQLITE_OPEN_READ_ONLY` and set busy_timeout

## AIKS Decisions

1. Use `message` + `part` tables (not `session_message` which stores events, not messages)
2. JOIN `session` with `project` to get `worktree` as project path
3. Filter `time_archived IS NULL` to skip archived sessions
4. Use `millis_to_datetime()` for all time fields (stored as Unix ms)
5. `tool` parts produce TWO ContentBlocks: ToolCall + ToolResult (if `state.status == "completed"`)
6. Windows DB path: `~/.local/share/opencode/opencode.db` (same as Linux)

## Attribution Required

- Parsing approach adapted from: AICoder Session Viewer
- Original file: `references/aicoder-session-viewer/src-tauri/src/providers/opencode.rs`
- AIKS derived file: `crates/aiks-core/src/providers/opencode.rs`
- License notice required: MIT
