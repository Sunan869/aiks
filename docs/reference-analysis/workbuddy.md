# WorkBuddy Provider Reference Analysis

Checked: 2026-09-21

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
workbuddy-jsonl-v2
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

For both discovery and loading, the normalized update timestamp is the newer of `updated_at` and `last_activity_at`. When the optional activity field is absent or null, `updated_at` is used. Discovery sorts by the same rule. This v2 refinement supersedes the original activity-first rule: an older activity timestamp must not hide a newer metadata update.

## Root discovery

Resolution order is:

1. Explicit `providers.workbuddy.path`.
2. `WORKBUDDY_CONFIG_DIR`.
3. `~/.workbuddy`.
4. Windows `%ProgramData%\WorkBuddy\users\<...>\.workbuddy` candidates.
5. Windows `%SystemDrive%\WorkBuddy-env\<...>\.workbuddy` candidates.

An explicit path must identify the data root containing `workbuddy.db` and `projects/`, not the WorkBuddy installation directory or the AIKS output directory. Use an actual absolute path rather than a literal `~` in an explicit configuration value. If automatic Windows discovery does not locate the installation, use the explicit path override; candidate discovery still needs real-device acceptance.

SQLite is opened read-only with a finite busy timeout. The provider issues no SQL writes and does not modify settings or transcript files.

## Transcript identity

AIKS does not derive transcript identity from the JSONL filename or from a WorkBuddy cwd encoding.

It recursively inspects only `projects/**/*.jsonl` and reads each event's internal session identity:

```text
sessionId
session_id
```

The resulting session-id-to-transcript index is used to attach transcripts to DB sessions. This intentionally supports cases where the filename does not match the session ID. A null or non-string `sessionId` does not mask a valid string `session_id`.

Loading a transcript without any matching session events returns an error rather than a misleading empty success. Title-only events still count as matching session events even though they do not become messages.

Blank lines are ignored. Malformed nonblank JSONL lines are skipped without discarding valid events. On a successful load with malformed lines, `metadata.parse_warnings.malformed_lines` records the count; the warning log contains the count, not the source lines.

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

Known event shapes are mapped to canonical Text / Thinking / ToolCall / ToolResult blocks. Unknown event types and unsupported message blocks are retained as `ContentBlock::Unknown`. In v2, reasoning without a supported string `text` or `content` is also retained as the complete unknown event, rather than being reduced to empty thinking text. Other unobserved tool-event layouts still require their own fixtures before compatibility can be claimed.

A `file-history-snapshot` event is not followed into WorkBuddy's `file-history/` directory.

## Privacy boundary

The WorkBuddy provider must not read or publish:

- `connectors/`
- connection tokens or `.neodata_token`
- memory profiles
- MCP secrets/configured credentials
- arbitrary `file-history/` contents
- unrelated WorkBuddy files outside the DB metadata and transcript scope

The scanner rejects a symlinked `projects` root and does not follow child links. Before transcript reads, canonical paths are checked to remain inside the configured `projects` directory and to identify JSONL files. Loading a caller-supplied summary uses the same check; a forged `source_path` cannot simply bypass discovery's read boundary. These checks are not a claim of race-proof isolation against concurrent hostile filesystem changes.

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

When a transcript was recorded with an older parser version, checking it with `workbuddy-jsonl-v2` returns a modified state and requests a normal re-parse through the existing incremental scanner. Tests include a `v1` to `v2` transition with unchanged file content.

## V2 regression evidence

`crates/aiks-core/tests/workbuddy_regressions.rs` adds nine synthetic regression cases covering metadata timestamps, discovery ordering, structured reasoning preservation, session-ID fallback, mismatched transcripts, malformed-line diagnostics, and out-of-scope or symlinked paths.

Before the production fix, all nine cases failed in CI run 35566267326 at commit `6c4c10dcafc5b9cf0c19584db894efc7fa084a62`; the pre-existing WorkBuddy provider tests passed. The symlink cases run on Unix. Windows compilation does not by itself validate Windows junction or real-installation behavior. Use the latest feature-branch CI result for post-fix status.

## Local acceptance still required

Automated fixtures do not establish compatibility with every installed WorkBuddy version. Before release, exercise the feature build with a local installation:

1. Select the real data root and confirm the Data Sources page reports WorkBuddy health correctly, including a valid zero-session database.
2. Sync a synthetic conversation with user text, an assistant response, and a tool call/result; inspect the imported session and search for a unique phrase.
3. Repeat sync without changes and confirm that no duplicate session or message is created.
4. Append a message, then separately rename the session; check refreshed content, title, timestamps, and ordering through the full sync path.
5. Confirm the intended downstream Knowledge/SiYuan view receives the imported session without introducing a WorkBuddy-only pipeline.

These are acceptance steps, not a record of completed real-device verification. Do not upload private transcripts or credentials for this check.

## Provenance

The upstream WorkBuddy repository was inspected to understand local storage responsibilities and event shapes. The AIKS provider was implemented independently against those observed data formats.

Because no WorkBuddy source code was copied or adapted into AIKS, no WorkBuddy-derived-code entry is added to `THIRD_PARTY_NOTICES.md`.
