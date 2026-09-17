# V4.2 AI Assist and Bridge Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add explicit AI Assist for manually authored knowledge and reduce the AIKS↔SiYuan bridge to cross-system coordination only.

**Architecture:** AI Assist is an AIKS backend capability that receives canonical document text and returns structured suggestions without mutating the body. `aiks-siyuan` requests assist operations and applies accepted suggestions locally. Pure SiYuan presentation actions remain inside the Workbench and are removed from the cross-system bridge contract.

**Tech Stack:** Rust/OpenAI-compatible chat API, Tauri, TypeScript/SiYuan, Vitest/Node tests.

**Spec:** `docs/superpowers/specs/2026-09-17-v4-2-backend-unification-design.md`

## Global Constraints
- AI Assist never silently overwrites user-authored body content.
- All AI Assist operations use the same `ModelService` LLM capability as session extraction.
- Suggestions must be structured JSON and validated before crossing the Tauri boundary.
- Bridge remains nonce/version validated.
- Preserve the user’s current `aiks/v4.2` UI redesign.

---

### Task 1: Add structured AI Assist backend

**Files:**
- Create: `crates/aiks-core/src/knowledge/ai_assist.rs`
- Modify: `crates/aiks-core/src/knowledge/mod.rs`
- Modify: `crates/aiks-core/src/ai/model_service.rs`
- Test: `crates/aiks-core/src/knowledge/ai_assist.rs`

**Types:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiAssistOperation {
    Summary,
    Tags,
    Category,
    Title,
    KeyConclusions,
    Structure,
    Rewrite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAssistRequest {
    pub operation: AiAssistOperation,
    pub title: String,
    pub content: String,
    pub existing_summary: Option<String>,
    pub existing_tags: Vec<String>,
    pub existing_category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAssistSuggestion {
    pub operation: AiAssistOperation,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub text: Option<String>,
}
```

- [ ] **Step 1: Write RED tests for operation parsing and output validation**
- [ ] **Step 2: Add deterministic prompt builders per operation**
- [ ] **Step 3: Add one `ModelService::complete_json` helper using the existing OpenAI-compatible LLM client**
- [ ] **Step 4: Reject malformed/empty suggestions rather than returning unvalidated model text**
- [ ] **Step 5: Run `cargo test -p aiks-core knowledge::ai_assist`**
- [ ] **Step 6: Commit**

Commit: `feat(v4.2): add manual knowledge AI assist service`

---

### Task 2: Expose AI Assist through Tauri without mutating canonical content

**Files:**
- Modify: `apps/aiks-desktop/src-tauri/src/knowledge_commands.rs` or create `ai_assist_commands.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/lib.rs`
- Modify: `apps/aiks-desktop/src/api/types.ts`
- Modify: `apps/aiks-desktop/src/api/tauri.ts`
- Test: API contract tests

**Command:**

```text
assist_knowledge_v42(
  siyuan_doc_id,
  operation,
  title,
  content,
  existing_summary,
  existing_tags,
  existing_category
)
```

The command returns a suggestion only. It does not write SiYuan or SQLite.

- [ ] **Step 1: Write RED API contract tests**
- [ ] **Step 2: Implement command and type serialization**
- [ ] **Step 3: Verify no DB/SiYuan write occurs in the command**
- [ ] **Step 4: Run Rust + desktop API tests**
- [ ] **Step 5: Commit**

Commit: `feat(v4.2): expose knowledge AI assist API`

---

### Task 3: Add the cross-system AI Assist request/response contract

**Repositories:** `Sunan869/aiks` and `Sunan869/aiks-siyuan`

**Files in `aiks`:**
- Modify: `apps/aiks-desktop/src-tauri/src/workbench/protocol.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/workbench/events.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/workbench/commands.rs` if backend→Workbench responses use the existing action path
- Modify: corresponding tests

**Files in `aiks-siyuan`:**
- Create or modify: `app/src/aiks/hostApi.ts`
- Add focused tests in `app/src/aiks/*.test.ts`

**Contract:**

Workbench→AIKS event:

```json
{
  "event": "requestAiAssist",
  "payload": {
    "requestId": "...",
    "docId": "...",
    "operation": "summary"
  }
}
```

AIKS→Workbench action:

```json
{
  "action": "aiAssistResult",
  "payload": {
    "requestId": "...",
    "ok": true,
    "suggestion": { }
  }
}
```

The Workbench reads its own canonical editor content before/when forming the request or AIKS fetches canonical markdown by `docId`; choose one implementation and keep the body out of generic window messages when possible. Preferred: AIKS fetches markdown from SiYuan by `docId`.

- [ ] **Step 1: Write protocol RED tests for allowed event/action + request ID validation**
- [ ] **Step 2: Implement backend request handling: fetch canonical markdown, call `AiAssistService`, send structured result**
- [ ] **Step 3: Implement `aiks-siyuan` host API helpers for requesting assist and receiving result**
- [ ] **Step 4: Run both repositories’ focused tests/typecheck**
- [ ] **Step 5: Commit separately in both repositories**

Suggested commits:
- `aiks`: `feat(v4.2): bridge knowledge AI assist requests`
- `aiks-siyuan`: `feat(aiks): integrate knowledge AI assist bridge`

---

### Task 4: Remove pure SiYuan UI operations from the cross-system bridge

**Files in `aiks`:**
- Modify: `apps/aiks-desktop/src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.js`
- Modify: `apps/aiks-desktop/src-tauri/src/workbench/protocol.rs`
- Modify: `apps/aiks-desktop/src/api/workbench.ts`
- Modify tests

**Files in `aiks-siyuan`:**
- Modify: `app/src/aiks/hostApi.ts` and current UI call sites only where required

Cross-system bridge retains:

```text
bridgeReady
openDocument
openBlock
openSession/setWorkspaceMode
readonly control
documentCreated
documentChanged
documentDeleted
requestAiAssist
requestOpenSession
requestOpenKnowledge
```

Pure Workbench presentation is local and removed from AIKS bridge responsibility:

```text
showSearch
showGraph
showOutline
showBacklinks
showDatabase
```

- [ ] **Step 1: Write contract tests asserting removed actions are no longer in the active bridge action set**
- [ ] **Step 2: Update AIKS callers so none rely on removed actions**
- [ ] **Step 3: Keep equivalent native behavior directly in `aiks-siyuan` where your redesigned UI exposes it**
- [ ] **Step 4: Run all frontend/bridge tests in both repositories**
- [ ] **Step 5: Commit**

---

### Task 5: Final runtime packaging and lock update

**Repositories:** both

- [ ] **Step 1: Merge reviewed `aiks/v4.2-backend-bridge` into `aiks/v4.2` without overwriting the existing UI redesign**
- [ ] **Step 2: Run `cd app && pnpm test && pnpm run typecheck && pnpm run build:desktop`**
- [ ] **Step 3: Let the existing runtime workflow publish the immutable Windows runtime release for the final `aiks/v4.2` commit**
- [ ] **Step 4: Update `scripts/siyuan.version` in `aiks` to the new tag, fork commit and SHA256**
- [ ] **Step 5: Run `scripts/dev.ps1` identity validation path plus repository CI**
- [ ] **Step 6: Manual smoke: create/edit/delete knowledge, product search, session result navigation, AI Assist preview/apply, readonly AI session documents**
