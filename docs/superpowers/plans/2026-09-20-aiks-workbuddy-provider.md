# AIKS WorkBuddy Provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add WorkBuddy as AIKS's fifth local session provider, with read-only SQLite discovery, JSONL parsing, incremental integration, and Desktop source visibility.

**Architecture:** Reuse the existing `SessionProvider` trait, `ProviderRegistry`, canonical `NormalizedSession` model, sync engine, and pipeline. Implement a new native Rust `WorkBuddyProvider`; do not embed or invoke `workbuddy-copilot`. Keep WorkBuddy-specific data shapes inside the provider and emit only AIKS canonical types downstream.

**Tech Stack:** Rust, async-trait, rusqlite, serde_json, chrono, existing AIKS StateDb/IncrementalScanner, React/Vite/TypeScript.

**Spec:** `docs/superpowers/specs/2026-09-20-aiks-workbuddy-provider-design.md`

## Global Constraints

- WorkBuddy upstream files are read-only; never modify `workbuddy.db`, WAL/SHM, settings, hooks, or transcripts.
- Do not read WorkBuddy `connectors/`, `.neodata_token`, memory profiles, MCP secrets, or arbitrary file-history content.
- Do not add a second sync/pipeline path; reuse canonical AIKS flow.
- One corrupt JSONL line/session must not block other sessions/providers.
- Parser version is `workbuddy-jsonl-v1`.
- Existing Claude/Codex/Gemini/OpenCode behavior must remain compatible.
- Fixtures must be synthetic and anonymous.

## Review Focus

- WorkBuddy installed with a healthy DB but zero sessions must be detected rather than reported as missing.
- Windows/UNC/non-POSIX cwd values must not be required to derive transcript paths correctly.
- A JSONL filename that looks like a session id but contains a different `sessionId` must not be incorrectly attached.
- Schema drift in optional DB columns must degrade safely while missing required columns reports provider error.
- Malformed/unknown JSONL events must not leak transcript content into logs or crash the global scan.

---

### Task 1: Add WorkBuddy canonical source and configuration contract

**Files:**
- Modify: `crates/aiks-core/src/model/mod.rs`
- Modify: `crates/aiks-core/src/config/mod.rs`
- Modify: `config.example.toml`
- Test: `crates/aiks-core/src/model/mod.rs`
- Test: `crates/aiks-core/src/config/mod.rs`

**Interfaces:**
- Produces: `SourceKind::WorkBuddy`, stable key `workbuddy`, display name `WorkBuddy`, `Config.providers.workbuddy: ProviderConfig`, `Config::workbuddy_path() -> Option<PathBuf>`.
- Consumed by: Tasks 2-5.

- [ ] **Step 1: Write failing tests for the source kind and config field**

Add `WorkBuddy` to the existing source roundtrip expectation and add config tests asserting TOML with `[providers.workbuddy]` deserializes and `workbuddy_path()` returns an override path.

- [ ] **Step 2: Run focused tests and verify RED**

Run:
```bash
cargo test -p aiks-core source_kind_roundtrip
cargo test -p aiks-core workbuddy
```
Expected: compile/test failure because `SourceKind::WorkBuddy`, the config field, and `workbuddy_path()` do not exist yet.

- [ ] **Step 3: Implement the minimal canonical/config changes**

Add:
```rust
SourceKind::WorkBuddy
```
with `as_str() == "workbuddy"`, `display_name() == "WorkBuddy"`, aliases `workbuddy`/`work_buddy`; add `workbuddy: ProviderConfig` to `ProvidersConfig`; add `Config::workbuddy_path()` equivalent to Claude/Gemini/OpenCode; document the section in `config.example.toml`.

- [ ] **Step 4: Re-run focused tests and full core tests**

Run:
```bash
cargo test -p aiks-core source_kind_roundtrip
cargo test -p aiks-core workbuddy
cargo test -p aiks-core
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/aiks-core/src/model/mod.rs crates/aiks-core/src/config/mod.rs config.example.toml
git commit -m "feat: add WorkBuddy source configuration"
```

---

### Task 2: Implement read-only WorkBuddy discovery and health checking

**Files:**
- Create: `crates/aiks-core/src/providers/workbuddy.rs`
- Modify: `crates/aiks-core/src/providers/mod.rs`
- Create: `crates/aiks-core/tests/workbuddy_provider.rs`

**Interfaces:**
- Consumes: `SourceKind::WorkBuddy`, `Config::workbuddy_path()` from Task 1.
- Produces: `WorkBuddyProvider::new(path_override: Option<PathBuf>)`, `SessionProvider` implementation, `parser_version() == "workbuddy-jsonl-v1"`.
- Consumed by: Tasks 3-5.

- [ ] **Step 1: Write failing provider discovery tests with an isolated temporary DB**

Tests create a temporary `.workbuddy/workbuddy.db` with synthetic `sessions` rows and assert:

```rust
assert_eq!(summary.source, SourceKind::WorkBuddy);
assert_eq!(summary.title.as_deref(), Some("Custom title"));
assert_eq!(summary.project_path.as_deref(), Some("C:\\src\\demo"));
```

Include an active row and a `deleted_at IS NOT NULL` row; only the active row may be discovered. Add a zero-session DB health test that expects `ProviderHealth::Ok`.

- [ ] **Step 2: Run the new integration test and verify RED**

Run:
```bash
cargo test -p aiks-core --test workbuddy_provider
```
Expected: compile failure because the WorkBuddy provider does not exist.

- [ ] **Step 3: Implement root resolution, read-only SQLite access, schema checks, discovery, and health**

Implementation rules:

```text
explicit override
WORKBUDDY_CONFIG_DIR
~/.workbuddy
Windows ProgramData candidates
Windows WorkBuddy-env candidates
```

Open SQLite with `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX` and a finite busy timeout. Validate required columns using `PRAGMA table_info(sessions)`. Build the SELECT dynamically enough that optional `model`, `permission_mode`, and `is_playground` columns may be absent without breaking discovery. Exclude soft-deleted rows and prefer `custom_title` over `title`.

- [ ] **Step 4: Verify provider tests and core regression**

Run:
```bash
cargo test -p aiks-core --test workbuddy_provider
cargo test -p aiks-core
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/aiks-core/src/providers/workbuddy.rs crates/aiks-core/src/providers/mod.rs crates/aiks-core/tests/workbuddy_provider.rs
git commit -m "feat: discover WorkBuddy sessions read-only"
```

---

### Task 3: Parse WorkBuddy transcripts into the canonical model

**Files:**
- Modify: `crates/aiks-core/src/providers/workbuddy.rs`
- Modify: `crates/aiks-core/tests/workbuddy_provider.rs`

**Interfaces:**
- Consumes: `SessionSummary` discovery from Task 2.
- Produces: `load_session()` returning `NormalizedSession` with canonical text/reasoning/tool/unknown blocks.
- Consumed by: Tasks 4-5 and the existing sync/pipeline.

- [ ] **Step 1: Add failing transcript parsing tests**

Cover synthetic JSONL events:

```json
{"id":"u1","timestamp":1782828714330,"type":"message","role":"user","content":[{"type":"input_text","text":"<system-reminder>x</system-reminder><user_query>真实问题</user_query>"}],"sessionId":"s1","cwd":"C:\\src\\demo"}
{"id":"a1","timestamp":1782828715330,"type":"message","role":"assistant","content":[{"type":"output_text","text":"回答"}],"sessionId":"s1"}
{"type":"reasoning","text":"思考"}
{"type":"function_call","id":"call-1","name":"read_file","arguments":{"path":"a.txt"}}
{"type":"function_call_result","id":"call-1","content":"ok"}
{"type":"file-history-snapshot","content":"must-not-load-file-history"}
{"type":"ai-title","aiTitle":"Ignored title"}
not-json
```

Assert tagged user text becomes exactly `真实问题`, assistant output is preserved, malformed line does not abort load, DB title stays authoritative, and unsupported event shapes become `Unknown` rather than guessed canonical values.

Add the review-focus case where `wrong-name.jsonl` contains `sessionId: s1`; transcript lookup must attach by verified session identity rather than cwd path encoding.

- [ ] **Step 2: Run the integration test and verify RED**

Run:
```bash
cargo test -p aiks-core --test workbuddy_provider transcript
```
Expected: failures because `load_session`/parser behavior is incomplete.

- [ ] **Step 3: Implement recursive transcript indexing and conservative parser**

Implement helpers with focused responsibilities:

```rust
fn build_transcript_index(&self) -> anyhow::Result<HashMap<String, PathBuf>>
fn transcript_session_id(path: &Path) -> Option<String>
fn extract_message_text(content: &serde_json::Value) -> Vec<String>
fn extract_user_query(text: &str) -> String
fn parse_event(value: serde_json::Value) -> Option<ParsedEvent>
```

Rules:
- recursively inspect only `projects/**/*.jsonl`;
- accept `sessionId` and `session_id` identity keys;
- do not read WorkBuddy `file-history/` paths referenced by events;
- malformed lines are skipped with non-content warning metadata;
- `ai-title` creates no message;
- unknown/unproven schemas are preserved as `ContentBlock::Unknown`.

- [ ] **Step 4: Re-run transcript tests and full core tests**

Run:
```bash
cargo test -p aiks-core --test workbuddy_provider
cargo test -p aiks-core
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/aiks-core/src/providers/workbuddy.rs crates/aiks-core/tests/workbuddy_provider.rs
git commit -m "feat: parse WorkBuddy transcripts"
```

---

### Task 4: Wire WorkBuddy into the registry and incremental pipeline contract

**Files:**
- Modify: `crates/aiks-core/src/providers/mod.rs`
- Modify: `crates/aiks-core/tests/workbuddy_provider.rs`
- Modify only if required by existing exhaustive source matching: `crates/aiks-core/src/sync/engine.rs`, `crates/aiks-core/src/runtime/mod.rs`, `crates/aiks-core/src/storage/repo.rs`, or other compiler-reported exhaustive matches.

**Interfaces:**
- Consumes: `WorkBuddyProvider` from Tasks 2-3.
- Produces: normal `ProviderRegistry` construction and parser-version flow into existing sync state.

- [ ] **Step 1: Write failing registry/incremental tests**

Test that enabling WorkBuddy in config makes `build_registry()` return a provider for `SourceKind::WorkBuddy` and that `parser_version()` is `workbuddy-jsonl-v1`.

Using existing `IncrementalScanner`, record a transcript with `workbuddy-jsonl-v0` and assert checking it with `workbuddy-jsonl-v1` returns `FileChangeStatus::Modified`.

- [ ] **Step 2: Run focused tests and verify RED**

Run:
```bash
cargo test -p aiks-core --test workbuddy_provider registry
cargo test -p aiks-core --test workbuddy_provider parser_version
```
Expected: registry test fails until WorkBuddy construction is wired.

- [ ] **Step 3: Wire WorkBuddy into `build_registry` and resolve exhaustive matches minimally**

Do not create WorkBuddy-specific pipeline branches. Any compiler-required `SourceKind` match updates must preserve existing semantics and use WorkBuddy's stable display/key values.

- [ ] **Step 4: Run core and CLI verification**

Run:
```bash
cargo test -p aiks-core
cargo check -p aiks-cli
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/aiks-core/src/providers/mod.rs crates/aiks-core/tests/workbuddy_provider.rs crates/aiks-core/src
git commit -m "feat: wire WorkBuddy into AIKS registry"
```

---

### Task 5: Surface WorkBuddy in Desktop sources without conflating health and count

**Files:**
- Modify: `apps/aiks-desktop/src/pages/SourcesPage.tsx`
- Modify: `apps/aiks-desktop/src/api/types.ts` and/or Tauri/core status payload only if provider health is not currently exposed.
- Modify: `apps/aiks-desktop/src/api/tauri.ts` and corresponding Rust command/status code only if required to expose provider health.
- Create or modify: `apps/aiks-desktop/src/api/workbuddy-source.test.ts` (or existing closest source/status test file).

**Interfaces:**
- Consumes: source display name `WorkBuddy`, source key `workbuddy`, provider health from core if available.
- Produces: Sources page card that can show detected/healthy with `0` sessions and sync via `workbuddy`.

- [ ] **Step 1: Write failing frontend/status tests**

Assert WorkBuddy exists in the rendered/source mapping and maps to sync key `workbuddy`. Add a contract test showing health/detection can be true while count is zero; do not use `count > 0` as the only detected signal.

- [ ] **Step 2: Run frontend tests and verify RED**

Run the repository's frontend test command from `apps/aiks-desktop` for the new/modified test.
Expected: FAIL because WorkBuddy is not listed and/or detection still depends on `count > 0`.

- [ ] **Step 3: Implement minimal Desktop/status changes**

Add WorkBuddy label/icon/description and stable sync-key mapping. If current `FullStatus` lacks provider health, extend the existing status DTO once in Core/Tauri and consume it in the page; do not add a WorkBuddy-only endpoint.

- [ ] **Step 4: Verify frontend and affected Rust tests**

Run:
```bash
cd apps/aiks-desktop
npm ci
npm test -- --run
npm run build
cd ../..
cargo test -p aiks-core
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/aiks-desktop crates/aiks-core apps/aiks-desktop/src-tauri
git commit -m "feat: show WorkBuddy data source"
```

---

### Task 6: Documentation, third-party attribution, and whole-workspace verification

**Files:**
- Modify: `README.md` if provider list is documented there.
- Modify: `AGENTS.md` provider/source text if needed to keep current-state documentation accurate.
- Modify: `THIRD_PARTY_NOTICES.md` only for actual copied/adapted MIT code; if implementation is independent and only format research was consulted, record research provenance in docs instead of falsely claiming copied code.
- Create: `docs/reference-analysis/workbuddy.md` summarizing verified storage assumptions and upstream references.

**Interfaces:**
- Consumes: completed implementation from Tasks 1-5.
- Produces: maintainable project documentation and final verification evidence.

- [ ] **Step 1: Add/update documentation describing the implemented provider and provenance**

Document:
- verified DB and JSONL responsibilities;
- read-only boundary;
- parser version;
- unsupported sensitive directories;
- links to `SuperOPC-AI-Incubator/workbuddy-copilot` research used to understand WorkBuddy storage;
- no claim that Python code was copied unless it actually was.

- [ ] **Step 2: Run final verification**

Run:
```bash
cargo test -p aiks-core
cargo check -p aiks-cli
cargo test --workspace
cd apps/aiks-desktop
npm ci
npm test -- --run
npm run build
```
Expected: all commands PASS. If CI has environment-specific Tauri system-dependency failures, report them separately and do not call the feature complete until product-code tests are green.

- [ ] **Step 3: Inspect branch diff for scope/privacy regressions**

Confirm no real `.workbuddy` data, tokens, private paths, unrelated refactors, or copied unlicensed Python code entered the branch.

- [ ] **Step 4: Commit documentation**

```bash
git add README.md AGENTS.md THIRD_PARTY_NOTICES.md docs/reference-analysis/workbuddy.md
git commit -m "docs: document WorkBuddy provider integration"
```
