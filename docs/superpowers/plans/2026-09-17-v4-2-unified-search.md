# V4.2 Unified Search and Session Indexing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the product-level SiYuan search shortcut with AIKS Unified Search over both Knowledge and AI Sessions using lexical + semantic recall and deterministic fusion.

**Architecture:** Reuse the ModelService/embedding capability from the indexing plan. Add session chunks/embeddings as a first-class searchable corpus, split lexical and semantic recall behind a search service, fuse candidates with RRF, and expose one Tauri API consumed by the top-level AIKS search UI. Keep SiYuan native search only for editor-native workflows.

**Tech Stack:** Rust, SQLite FTS5, existing embedding storage, Tauri, React/Vitest, SiYuan bridge/host API.

**Spec:** `docs/superpowers/specs/2026-09-17-v4-2-backend-unification-design.md`

## Global Constraints
- Knowledge and AI Sessions must be distinguishable in results.
- Product search must not call `showWorkbenchSearch()`.
- Search still returns lexical results when embedding is disabled/unavailable, with an explicit degradation flag.
- Chinese queries must not depend solely on whitespace splitting.
- V4.2 does not require Qdrant/Milvus.

---

### Task 1: Add persistent session chunks and embeddings

**Files:**
- Create: `crates/aiks-core/migrations/010_v42_session_search.sql`
- Create: `crates/aiks-core/src/indexing/session.rs`
- Modify: `crates/aiks-core/src/storage/db.rs`
- Modify: `crates/aiks-core/src/sync/*` at the point normalized sessions are persisted
- Test: `crates/aiks-core/src/indexing/session.rs`

**Interfaces:**

```rust
pub struct SessionIndexInput {
    pub session_id: i64,
    pub external_id: String,
    pub source: String,
    pub title: Option<String>,
    pub normalized_text: String,
}

pub struct SessionIndexService { /* db + embedding provider */ }

impl SessionIndexService {
    pub async fn index_session(&self, input: SessionIndexInput) -> anyhow::Result<usize>;
    pub fn remove_session(&self, session_id: i64) -> anyhow::Result<()>;
}
```

Tables/indexes must support lexical session search plus vector records tied to session chunks. Reuse the same embedding model identity as knowledge.

- [ ] **Step 1: Write RED tests for first index, idempotence and update replacement**
- [ ] **Step 2: Run `cargo test -p aiks-core indexing::session` and observe failure**
- [ ] **Step 3: Add additive migration and repository methods**
- [ ] **Step 4: Implement session chunking/embedding without holding the DB lock across HTTP**
- [ ] **Step 5: Wire successful source-session sync to enqueue/update the session index**
- [ ] **Step 6: Run focused tests and commit**

Commit message: `feat(v4.2): index AI sessions for semantic search`

---

### Task 2: Replace the current search implementation with explicit lexical and semantic recall modules

**Files:**
- Create: `crates/aiks-core/src/search/mod.rs`
- Create: `crates/aiks-core/src/search/query.rs`
- Create: `crates/aiks-core/src/search/lexical.rs`
- Create: `crates/aiks-core/src/search/semantic.rs`
- Create: `crates/aiks-core/src/search/fusion.rs`
- Create: `crates/aiks-core/src/search/service.rs`
- Modify: `crates/aiks-core/src/lib.rs`
- Compatibility modify: `crates/aiks-core/src/pipeline/search.rs`

**Public types:**

```rust
pub enum SearchCorpus { Knowledge, Session }

pub struct UnifiedSearchFilter {
    pub corpora: Vec<SearchCorpus>,
    pub project: Option<String>,
    pub source: Option<String>,
}

pub struct UnifiedSearchHit {
    pub corpus: SearchCorpus,
    pub entity_id: String,
    pub chunk_id: Option<String>,
    pub title: String,
    pub snippet: String,
    pub score: f32,
    pub match_types: Vec<String>,
    pub siyuan_doc_id: Option<String>,
}

pub struct UnifiedSearchOutcome {
    pub hits: Vec<UnifiedSearchHit>,
    pub degraded: bool,
    pub warnings: Vec<String>,
}
```

- [ ] **Step 1: Add query analyzer RED tests**

Examples must prove useful tokens are produced for:
- `如何解决kubernetes节点磁盘空间不足`
- `42804 timestamptz`
- `/var/lib/kubelet/pods`
- `qwen3.8:27b`

The analyzer may use CJK character/bigram terms plus preserved ASCII technical tokens; no external tokenizer dependency is required for V4.2.

- [ ] **Step 2: Add lexical recall tests for Knowledge and Session corpora**
- [ ] **Step 3: Add semantic recall tests with deterministic fake vectors**
- [ ] **Step 4: Implement Reciprocal Rank Fusion**

Use deterministic RRF:

```rust
score += 1.0 / (60.0 + rank as f32);
```

Track contributing match types rather than relying on the old hard-coded `0.35/0.65` score mixture.

- [ ] **Step 5: Keep degradation behavior explicit**

Embedding failure returns lexical results and a warning; lexical failure should degrade to safe literal fallback rather than return an empty semantic-only set.

- [ ] **Step 6: Keep `pipeline::search` as a thin compatibility wrapper**

Existing callers can delegate to `search::service` during migration; no second ranking implementation remains.

- [ ] **Step 7: Run search tests and commit**

Run: `cargo test -p aiks-core search:: pipeline::search`
Commit: `feat(v4.2): add unified hybrid search service`

---

### Task 3: Expose Unified Search through the engine and Tauri API

**Files:**
- Modify: `crates/aiks-core/src/engine/mod.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/knowledge_commands.rs` or create `search_commands.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/lib.rs`
- Modify: `apps/aiks-desktop/src/api/types.ts`
- Modify: `apps/aiks-desktop/src/api/tauri.ts`
- Modify: `apps/aiks-desktop/src/api/index.ts`
- Test: Rust command tests where available; `apps/aiks-desktop/src/api/*.test.ts`

**Engine interface:**

```rust
pub async fn unified_search(
    &self,
    query: &str,
    limit: usize,
    filter: UnifiedSearchFilter,
) -> anyhow::Result<UnifiedSearchOutcome>;
```

**Tauri command:**

```text
search_all_v42(query, limit, corpora, project, source)
```

- [ ] **Step 1: Write frontend API contract tests first**
- [ ] **Step 2: Implement engine delegation and Tauri serialization**
- [ ] **Step 3: Return `degraded` + `warnings` unchanged to the UI**
- [ ] **Step 4: Run Rust + frontend API tests**
- [ ] **Step 5: Commit**

Commit: `feat(v4.2): expose unified knowledge and session search`

---

### Task 4: Switch the AIKS product search UI away from SiYuan native search

**Files:**
- Modify: `apps/aiks-desktop/src/pages/KnowledgeWorkspacePage.tsx`
- Create: `apps/aiks-desktop/src/components/UnifiedSearchDialog.tsx`
- Create: `apps/aiks-desktop/src/components/UnifiedSearchDialog.test.tsx` if the current test setup supports component tests; otherwise test the search-state helper separately.
- Modify: `apps/aiks-desktop/src/api/workbench.ts` only to remove product-search dependence, not to remove native compatibility until the bridge cleanup plan.

**Behavior:**
- Search box opens AIKS results, not `showWorkbenchSearch()`.
- Results are grouped/labeled `知识` and `AI 对话`.
- Clicking a knowledge hit opens its `siyuan_doc_id` in knowledge mode.
- Clicking a session hit routes to Knowledge Workbench session mode/read-only.
- Degraded search shows a non-blocking warning while still rendering lexical results.

- [ ] **Step 1: Write a RED test asserting the top search no longer invokes `showWorkbenchSearch`**
- [ ] **Step 2: Write result routing tests for Knowledge and Session hits**
- [ ] **Step 3: Implement the dialog/state and API call**
- [ ] **Step 4: Run `npm test` and `npm run build` in `apps/aiks-desktop`**
- [ ] **Step 5: Commit**

Commit: `feat(v4.2): use unified search in knowledge workspace`

---

### Task 5: Remove product-search responsibility from the SiYuan bridge

**Repositories:** `Sunan869/aiks` and `Sunan869/aiks-siyuan`

**Files:**
- Modify in `aiks`: `apps/aiks-desktop/src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.js`
- Modify in `aiks`: bridge protocol/workbench tests
- Modify in `aiks-siyuan`: only host/API code that still assumes AIKS requests native global search, if any

- [ ] **Step 1: Add a contract test that product search has no `showSearch` dependency**
- [ ] **Step 2: Remove `showSearch` from active AIKS actions while preserving SiYuan's own native search UI internally**
- [ ] **Step 3: Run `aiks` frontend/Rust tests and `aiks-siyuan/app` `pnpm test && pnpm run typecheck`**
- [ ] **Step 4: Commit in each repository**

Suggested commits:
- `aiks`: `refactor(v4.2): remove native search bridge dependency`
- `aiks-siyuan`: `refactor(aiks): keep native search local to workbench`
