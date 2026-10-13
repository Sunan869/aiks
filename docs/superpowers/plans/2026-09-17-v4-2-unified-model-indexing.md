# V4.2 Unified Model and Knowledge Indexing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every canonical SiYuan knowledge document use one AIKS model/embedding capability layer and one idempotent FTS + chunk + vector indexing lifecycle.

**Architecture:** Add a small `ModelService` facade over the existing LLM and embedding clients, then add `KnowledgeIndexService` as the only document indexing entry point. SiYuan lifecycle events enqueue/execute this service; session extraction publishes to SiYuan and then calls the same service instead of owning a separate knowledge-vector path.

**Tech Stack:** Rust, rusqlite/SQLite FTS5, reqwest OpenAI-compatible APIs, Tauri events, SiYuan HTTP API.

**Spec:** `docs/superpowers/specs/2026-09-17-v4-2-backend-unification-design.md`

## Global Constraints
- SiYuan remains canonical for knowledge body/title.
- AIKS owns FTS/chunks/vectors/index state.
- No database lock may be held across HTTP/AI/SiYuan awaits.
- Existing `aiks.toml` files containing `[extractor]` must remain loadable during the migration window.
- Indexing must be idempotent by canonical content hash + embedding model identity.
- Editing a knowledge document must never leave deleted vectors without an automatic rebuild attempt.

---

### Task 1: Introduce the unified ModelService facade

**Files:**
- Create: `crates/aiks-core/src/ai/model_service.rs`
- Modify: `crates/aiks-core/src/ai/mod.rs`
- Modify: `crates/aiks-core/src/config/mod.rs`
- Test: `crates/aiks-core/src/ai/model_service.rs`

**Interfaces:**
- Produces: `pub struct ModelService`
- Produces: `ModelService::new(llm: AiModelConfig, embedding: EmbeddingConfig) -> anyhow::Result<Self>`
- Produces: `llm_config(&self) -> &AiModelConfig`
- Produces: `embedding_config(&self) -> &EmbeddingConfig`
- Produces: `embed_texts(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>>`
- Existing AI extraction remains compatible by consuming `AiModelConfig` from `ModelService` until the extractor itself is migrated.

- [ ] **Step 1: Write the failing facade test**

```rust
#[test]
fn model_service_exposes_one_llm_and_embedding_identity() {
    let llm = AiModelConfig { model: "qwen-test".into(), ..Default::default() };
    let embedding = EmbeddingConfig { model: "embed-test".into(), dimensions: Some(768), ..Default::default() };
    let service = ModelService::new(llm, embedding).unwrap();
    assert_eq!(service.llm_config().model, "qwen-test");
    assert_eq!(service.embedding_config().model, "embed-test");
    assert_eq!(service.embedding_config().dimensions, Some(768));
}
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test -p aiks-core model_service_exposes_one_llm_and_embedding_identity`
Expected: FAIL because `ModelService` does not exist.

- [ ] **Step 3: Implement the minimal facade**

`ModelService` owns an `AiModelConfig` and an `EmbeddingClient`. It exposes the configs and delegates embedding calls. Do not add provider discovery or reranking yet.

- [ ] **Step 4: Keep legacy extractor config load-compatible but unused by new code**

Keep `Config.extractor` for this release with a deprecation comment; new services must never read it. Existing `[extractor]` TOML therefore keeps parsing, while `config.ai` becomes the sole LLM execution configuration.

- [ ] **Step 5: Run core tests**

Run: `cargo test -p aiks-core ai::`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/aiks-core/src/ai crates/aiks-core/src/config/mod.rs
git commit -m "refactor(v4.2): unify model capability access"
```

---

### Task 2: Persist per-document index lifecycle state

**Files:**
- Create: `crates/aiks-core/migrations/009_v42_knowledge_index.sql`
- Modify: `crates/aiks-core/src/storage/db.rs`
- Modify: `crates/aiks-core/src/knowledge/model.rs`
- Test: `crates/aiks-core/src/storage/db.rs`

**Interfaces / Columns:**

Add to `knowledge_item`:

```text
index_status TEXT NOT NULL DEFAULT 'pending'
indexed_hash TEXT
indexed_at TEXT
embedding_model TEXT
embedding_dimensions INTEGER
index_chunk_count INTEGER NOT NULL DEFAULT 0
last_index_error TEXT
```

- [ ] **Step 1: Extend the migration test first**

Add an assertion that all seven columns exist after `StateDb::open()` and after reopening the same DB.

- [ ] **Step 2: Run migration tests and observe RED**

Run: `cargo test -p aiks-core storage::db::tests::open_creates_tables`
Expected: FAIL because V4.2 index columns are absent.

- [ ] **Step 3: Add guarded V10 migration**

Add `SCHEMA_V10_SQL` and use `ensure_column` for additive upgrades before executing indexes/default-normalization SQL. Keep migration idempotent.

- [ ] **Step 4: Expose index metadata in the knowledge read model**

Extend `KnowledgeRecord` with optional/default-safe fields matching the seven columns and update all row mappings.

- [ ] **Step 5: Run storage + knowledge tests**

Run: `cargo test -p aiks-core storage:: knowledge::`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/aiks-core/migrations/009_v42_knowledge_index.sql crates/aiks-core/src/storage/db.rs crates/aiks-core/src/knowledge
git commit -m "feat(v4.2): track knowledge index lifecycle"
```

---

### Task 3: Add KnowledgeIndexService as the only document indexing entry point

**Files:**
- Create: `crates/aiks-core/src/indexing/mod.rs`
- Create: `crates/aiks-core/src/indexing/service.rs`
- Modify: `crates/aiks-core/src/lib.rs`
- Modify: `crates/aiks-core/src/pipeline/embedding_stage.rs`
- Modify: `crates/aiks-core/src/pipeline/knowledge_repo.rs`
- Test: `crates/aiks-core/src/indexing/service.rs`

**Interfaces:**

```rust
pub struct KnowledgeIndexInput {
    pub knowledge_id: String,
    pub siyuan_doc_id: String,
    pub markdown: String,
}

pub struct KnowledgeIndexResult {
    pub knowledge_id: String,
    pub indexed_hash: String,
    pub chunk_count: usize,
    pub embedded_count: usize,
    pub skipped: bool,
}

pub struct KnowledgeIndexService {
    db: Arc<StateDb>,
    models: Arc<ModelService>,
}

impl KnowledgeIndexService {
    pub async fn index_document(&self, input: KnowledgeIndexInput) -> anyhow::Result<KnowledgeIndexResult>;
    pub fn mark_deleted(&self, siyuan_doc_id: &str) -> anyhow::Result<Option<String>>;
}
```

- [ ] **Step 1: Write RED tests for idempotence and stale replacement**

Tests must prove:
1. first index creates FTS/chunks and sets `ready`;
2. same content + same model returns `skipped=true`;
3. changed content deletes old chunk/vector derivatives before replacing them;
4. failure stores `failed` + `last_index_error`;
5. `mark_deleted` removes FTS/chunks/vectors and makes the item non-searchable.

Use a fake embedding backend boundary rather than a real network request. If needed, define an internal trait:

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn model_name(&self) -> &str;
    fn dimensions(&self) -> Option<usize>;
    async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>>;
}
```

`ModelService` supplies the production implementation; tests supply deterministic vectors.

- [ ] **Step 2: Run indexing tests and observe RED**

Run: `cargo test -p aiks-core indexing::`
Expected: FAIL because the module/service does not exist.

- [ ] **Step 3: Implement hash/idempotence and FTS/read-model update**

Compute SHA-256 over canonical markdown. Set `indexing`, update cached content/hash, rebuild `knowledge_fts`, split into chunks, then call embedding outside the DB lock.

- [ ] **Step 4: Persist embeddings transactionally after the HTTP call**

Store vectors with current model identity/dimensions, update chunk count and finish with `ready`. Any embedding error records `failed` without restoring stale vectors.

- [ ] **Step 5: Make old session-oriented embedding code delegate or remain compatibility-only**

`EmbeddingStage` must not contain a second chunking/vector persistence implementation for canonical knowledge. Move reusable chunking logic into indexing and have the legacy pipeline invoke `KnowledgeIndexService` after publication where possible.

- [ ] **Step 6: Run focused and full core tests**

Run: `cargo test -p aiks-core indexing:: pipeline:: knowledge::`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/aiks-core/src/indexing crates/aiks-core/src/pipeline crates/aiks-core/src/lib.rs
git commit -m "feat(v4.2): add canonical knowledge index service"
```

---

### Task 4: Wire SiYuan document lifecycle events to indexing

**Files:**
- Modify: `apps/aiks-desktop/src-tauri/src/workbench/events.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/app_state.rs` if a shared service handle is required
- Modify: `apps/aiks-desktop/src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.js`
- Test: `apps/aiks-desktop/src-tauri/src/workbench/events.rs`
- Test: `apps/aiks-desktop/src/api/workbench.test.ts` or a new bridge-source contract test

**Behavior:**

```text
documentCreated -> resolve canonical doc -> create/bind knowledge identity -> index
documentChanged -> mark stale -> fetch markdown -> index
documentDeleted -> delete derived indexes / mark deleted
```

Only `/20 Knowledge` documents enter the Knowledge index. `/10 AI Sessions` are handled by the session-index plan.

- [ ] **Step 1: Add failing event tests**

Extend inbound validation/dispatch tests so all three lifecycle events are accepted and routed to explicit handlers instead of falling through to generic `workbench-event` forwarding.

- [ ] **Step 2: Add a bridge contract test**

The bridge source must emit `documentCreated` and `documentDeleted` in addition to debounced `documentChanged`. Use SiYuan plugin lifecycle/event APIs when available; DOM input remains only the edit signal, not create/delete detection.

- [ ] **Step 3: Run RED tests**

Run Rust workbench tests and `npm test -- workbench` in `apps/aiks-desktop`.
Expected: FAIL until explicit lifecycle wiring exists.

- [ ] **Step 4: Implement event handlers**

For create/change, fetch canonical markdown from `SiYuanSink`, release network resources, then call `KnowledgeIndexService`. For delete, call `mark_deleted` immediately. Emit `knowledge-index-status` events with `pending/indexing/ready/failed` for diagnostics.

- [ ] **Step 5: Run tests**

Run:
```bash
cargo test -p aiks-desktop workbench
cd apps/aiks-desktop && npm test -- workbench
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/aiks-desktop/src-tauri/src/workbench apps/aiks-desktop/src-tauri/resources/siyuan/data/plugins/aiks-bridge apps/aiks-desktop/src/api
git commit -m "feat(v4.2): index SiYuan knowledge lifecycle events"
```

---

### Task 5: Ensure manual and extracted knowledge both enter the same index service

**Files:**
- Modify: `apps/aiks-desktop/src-tauri/src/knowledge_commands.rs`
- Modify: `crates/aiks-core/src/knowledge/publisher.rs`
- Modify: `crates/aiks-core/src/pipeline/ai_stage.rs`
- Test: corresponding existing module tests

- [ ] **Step 1: Add failing tests**

Prove that:
- manual creation publishes to SiYuan and finishes with `index_status=ready` when embedding is enabled;
- extracted knowledge publication calls the same `KnowledgeIndexService` path;
- with embedding disabled, FTS/chunks still become ready and vector count is zero without marking the index failed.

- [ ] **Step 2: Run RED tests**

Run: `cargo test -p aiks-core knowledge:: pipeline::`
Expected: FAIL on missing unified indexing calls.

- [ ] **Step 3: Implement one post-publication indexing hook**

Both manual creation and AI extraction pass `knowledge_id + siyuan_doc_id + canonical markdown` to `KnowledgeIndexService`; do not duplicate chunk/vector code.

- [ ] **Step 4: Run full Rust verification**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/aiks-desktop/src-tauri/src/knowledge_commands.rs crates/aiks-core/src/knowledge crates/aiks-core/src/pipeline
git commit -m "feat(v4.2): unify manual and extracted knowledge indexing"
```
