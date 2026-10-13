# AIKS WorkBuddy Provider Design

## Goal

Integrate WorkBuddy as AIKS's fifth local session provider while preserving the existing `SessionProvider -> NormalizedSession -> Pipeline -> Knowledge/Search` architecture.

## Scope

This change adds WorkBuddy only. Cursor, Cline, Copilot, web chat history, WorkBuddy memory/connectors/hooks, and bidirectional WorkBuddy writes are out of scope.

## Architecture

WorkBuddy remains a read-only upstream source. `workbuddy.db` is the authority for session metadata; `projects/**/*.jsonl` is the authority for transcript content. The provider discovers sessions cheaply from SQLite, resolves transcripts by verified session id, parses supported JSONL events into AIKS canonical types, and leaves Pipeline/Knowledge/Embedding/Search unchanged.

```text
WorkBuddy
  |- workbuddy.db                 session metadata
  `- projects/**/*.jsonl         transcript events
             |
             v
      WorkBuddyProvider
             |
             v
       SessionProvider
             |
             v
      NormalizedSession
             |
             v
      existing AIKS Pipeline
             |
             v
   Knowledge / Index / Search
```

## Provider discovery

`providers.workbuddy.path` points to the `.workbuddy` root, not directly to the DB file. Resolution order:

1. Explicit `providers.workbuddy.path`.
2. `WORKBUDDY_CONFIG_DIR`.
3. `~/.workbuddy`.
4. Windows `%ProgramData%/WorkBuddy/users/*/.workbuddy` candidates.
5. Windows `%SystemDrive%/WorkBuddy-env/*/.workbuddy` candidates.

A valid root must contain a readable `workbuddy.db`. `projects/` may be empty for a newly installed client, but when present it is scanned recursively for transcript files.

Do not derive a transcript path only from WorkBuddy's cwd encoding. Build a transcript index from `projects/**/*.jsonl` and verify session identity from JSONL `sessionId`/`session_id` when available. A filename equal to `<session-id>.jsonl` can be a candidate, but identity from content wins.

## SQLite access

Open `workbuddy.db` read-only and WAL-aware. AIKS must never write WorkBuddy's database, journal, WAL, schema, settings, hooks, or transcript files.

Authoritative session query fields:

- `id`
- `cwd`
- `title`
- `custom_title`
- `status`
- `created_at`
- `updated_at`
- `deleted_at`
- `mode`
- `last_activity_at`
- optional `permission_mode`, `is_playground`, `model` when present

Soft-deleted sessions (`deleted_at IS NOT NULL`) are excluded. `custom_title` takes precedence over `title`.

Schema compatibility must be defensive: required columns are checked; optional columns are read only when available. A schema mismatch is a provider error, not a crash of the global scan.

## Canonical mapping

Add `SourceKind::WorkBuddy` with stable string `workbuddy` and display name `WorkBuddy`.

Session mapping:

- `external_session_id` <- DB `sessions.id`
- `title` <- `custom_title` else `title`
- `project_path` <- `cwd`
- `project_name` <- final cwd path component when available
- `started_at` <- `created_at` milliseconds
- `updated_at` <- `last_activity_at` else `updated_at`
- `model` <- optional DB `model`
- `source_path` <- resolved JSONL path
- provider-private fields such as `mode`, `status`, `permission_mode`, `is_playground` remain in `metadata`

### `message`

Supported content forms:

- plain string
- array blocks with `type` in `text`, `input_text`, `output_text`

These become `ContentBlock::Text` blocks. Roles map through AIKS's canonical roles.

For user messages, if text contains `<user_query>...</user_query>`, keep only the tag body after trimming. If the tag is absent, keep the original text. System reminders surrounding a tagged user query therefore do not pollute the canonical user prompt.

### `reasoning`

When a reliable text field is present, emit `ContentBlock::Thinking`. If the shape is not recognized, preserve the event as `ContentBlock::Unknown`; do not guess undocumented fields.

### `function_call`

When a reliable tool name/input shape is present, emit `ContentBlock::ToolCall`. Otherwise preserve as `Unknown`.

### `function_call_result`

When a reliable result content and call id are present, emit `ContentBlock::ToolResult`; otherwise preserve as `Unknown`.

### `file-history-snapshot`

Do not load referenced file-history content and do not feed it to knowledge extraction. Preserve only safe event metadata/`Unknown` information from the JSONL event itself.

### `ai-title`

Do not create a canonical message. The DB title remains authoritative.

## Corruption and isolation

- Blank lines are ignored.
- Malformed JSON lines are skipped with warning and do not abort the session.
- One corrupt transcript may fail that session load but must not abort other WorkBuddy sessions.
- A WorkBuddy provider failure must not block Claude/Codex/Gemini/OpenCode discovery.

## Incremental behavior

Reuse existing `source_session`, `source_file_state`, `pipeline_job`, and `IncrementalScanner` semantics. WorkBuddy does not introduce a separate synchronization subsystem.

Parser version starts at `workbuddy-jsonl-v1`. A future parser-version bump must force reparse through existing parser-version change detection.

DB timestamps plus transcript file size/mtime/parser version provide the normal change signals. Unchanged sessions must not be reprocessed; new or changed sessions enter the existing durable pipeline.

## Health semantics

Provider state and session count are separate concepts.

- `Ok`: root and DB are detected, DB opens read-only, required session schema is compatible.
- `NotFound`: no usable WorkBuddy root/DB was found.
- `Error`: permission problem, corrupt DB, incompatible required schema, or real read failure.

A healthy WorkBuddy installation with zero sessions is still detected and should be shown as `0` sessions, not as "not installed".

## Desktop

The Sources page adds WorkBuddy with key `workbuddy`. It must eventually use provider health/detection rather than `count > 0` as the sole connection signal. This change should be made without altering sync behavior for the existing four providers.

## Privacy and security

Never read or publish WorkBuddy `connectors/`, `.neodata_token`, memory profile files, MCP secrets, or arbitrary `file-history/` content. Fixtures must be synthetic and anonymous. Logs must not dump full transcript contents or secrets.

## Tests and acceptance

Core tests cover:

1. `SourceKind::WorkBuddy` roundtrip.
2. Config default/override resolution.
3. Read-only DB discovery and soft-delete filtering.
4. `custom_title` precedence.
5. Transcript lookup by session identity.
6. `text`/`input_text`/`output_text` extraction.
7. `<user_query>` extraction and fallback.
8. Malformed JSON-line isolation.
9. Conservative reasoning/tool mapping with unknown fallback.
10. Parser-version integration with incremental state.
11. Existing providers remain available.

Desktop tests/build cover WorkBuddy display and sync key mapping.

Final verification target:

```bash
cargo test -p aiks-core
cargo check -p aiks-cli
cargo test --workspace
cd apps/aiks-desktop
npm ci
npm run build
```

No completion claim is made until actual CI/build output is observed.
