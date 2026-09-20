# WorkBuddy Provider Reference Analysis

Checked: 2026-09-20

Upstream reference: https://github.com/SuperOPC-AI-Incubator/workbuddy-copilot

This document records the storage assumptions used by the native AIKS WorkBuddy provider. The implementation is independent Rust code; no WorkBuddy source code was copied into AIKS.

## Scope

AIKS treats WorkBuddy as a read-only local Session Provider.

The provider reads only:

- `workbuddy.db` for session metadata.
- `projects/**/*.jsonl` for transcript events.

It does not invoke or embed `workbuddy-copilot`, and it does not create a WorkBuddy-specific downstream pipeline. Parsed data is converted to the existing AIKS `NormalizedSession` / `NormalizedMessage` canonical model.

Parser version:

```text
workbuddy-jsonl-v1
```

## Storage model

A typical WorkBuddy root contains:

```text
.workbuddy/
  workbuddy.db
  workbuddy.db-wal
  workbuddy.db-shm
  projects/
    <project encoding>/
      <session>.jsonl
      <session>/
  sessions/
  tasks/
  memory/
  connectors/
  file-history/
  settings.json
  models.json
  mcp.json
  blobs/
  traces/
  logs/
```

For AIKS integration, `workbuddy.db` is the metadata authority and the JSONL transcript is the conversational-content authority.

The `sessions` table fields used by the provider include the required identity/path/title/timestamp fields:

```text
id
cwd
title
custom_title
created_at
updated_at
deleted_at
```

Optional fields are consumed only when present, including:

```text
status
mode
last_activity_at
permission_mode
is_playground
model
```

Soft-deleted rows (`deleted_at IS NOT NULL`) are excluded. Display title precedence is:

```text
custom_title > title
```

## Root discovery

Resolution order is:

1. Explicit `providers.workbuddy.path`.
2. `WORKBUDDY_CONFIG_DIR`.
3. `~/.workbuddy`.
4. Windows `%ProgramData%\WorkBuddy\users\<...>\.workbuddy` candidates.
5. Windows `%SystemDrive%\WorkBuddy-env\<...>\.workbuddy` candidates.

SQLite is opened read-only with a finite busy timeout. The provider never writes the database, WAL, SHM, settings, transcript files, or any other upstream state.

## Transcript identity

AIKS does not derive transcript identity from the JSONL filename or from a WorkBuddy cwd encoding.

It recursively inspects only `projects/**/*.jsonl` and reads each event's internal session identity:

```text
sessionId
session_id
```

The resulting session-id-to-transcript index is used to attach transcripts to DB sessions. This intentionally supports cases where the filename does not match the session ID.

Malformed JSONL lines are skipped rather than aborting the provider scan.

## Verified event handling

The parser recognizes the following event families conservatively:

- `message`
- `reasoning`
- `function_call`
- `function_call_result`
- `file-history-snapshot`
- `ai-title`

Message text forms verified for normalization include:

```text
text
input_text
output_text
string content
```

For user messages, if a `<user_query>...</user_query>` region is present, AIKS extracts the body of that region. Otherwise it keeps the trimmed original text.

`ai-title` does not override the DB title.

Known, well-supported event shapes are mapped to canonical Text / Thinking / ToolCall / ToolResult blocks. Unsupported or insufficiently proven shapes are retained as `ContentBlock::Unknown` instead of being guessed into a stronger type.

A `file-history-snapshot` event is not followed into WorkBuddy's `file-history/` directory.

## Privacy boundary

The WorkBuddy provider must not read or publish:

- `connectors/`
- connection tokens or `.neodata_token`
- memory profiles
- MCP secrets/configured credentials
- arbitrary `file-history/` contents
- unrelated WorkBuddy files outside the DB metadata and transcript scope

Fixtures and tests use synthetic data only. Logs must not emit transcript content.

## Health semantics

A WorkBuddy installation with a valid compatible `workbuddy.db` and zero sessions is healthy.

Health and session count are therefore separate concepts:

```text
healthy provider + 0 sessions = detected/connected
```

Desktop source detection consumes provider health rather than inferring installation from `count > 0`.

## Incremental integration

WorkBuddy uses the existing AIKS provider registry, source state, parser-version invalidation, synchronization, durable pipeline, Knowledge extraction, search, and SiYuan paths.

There is no WorkBuddy-specific sync engine.

When a transcript was recorded with an older parser version, checking it with `workbuddy-jsonl-v1` returns a modified state and forces a normal re-parse through the existing incremental scanner.

## Provenance

The upstream WorkBuddy repository was inspected to understand local storage responsibilities and event shapes. The AIKS provider was implemented independently against those observed data formats.

Because no WorkBuddy source code was copied or adapted into AIKS, no WorkBuddy-derived-code entry is added to `THIRD_PARTY_NOTICES.md`.
