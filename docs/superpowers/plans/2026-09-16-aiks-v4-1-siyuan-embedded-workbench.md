# AIKS V4.1 SiYuan Embedded Workbench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make SiYuan the single source of truth for AIKS knowledge/session content while keeping AIKS Desktop as the unified shell and control plane, with an embedded reusable SiYuan workbench for block editing, backlinks, outline, database, graph, and provenance navigation.

**Architecture:** AIKS keeps Provider/Pipeline/AI/diagnostics/control metadata in SQLite, while user-facing content lives canonically in one `AI Knowledge` SiYuan notebook under `/10 AI Sessions` and `/20 Knowledge`. A persistent Tauri-managed SiYuan workbench is controlled through a versioned bridge protocol; V4 SQLite content columns remain as migration snapshots/cache for one release and are no longer the content write path after cutover.

**Tech Stack:** Rust, rusqlite, reqwest, Tauri 2, React 18, TypeScript, Vitest, Embedded SiYuan HTTP API and Plugin/Web UI runtime.

**Spec:** `docs/superpowers/specs/2026-09-16-aiks-v4-1-siyuan-embedded-workbench-design.md`

## Global Constraints

- Do not fork or modify SiYuan source code.
- Preserve existing V4 user knowledge and existing SiYuan doc IDs whenever a target document already exists.
- `/10 AI Sessions` is read-only from normal user flows; `/20 Knowledge` is fully editable.
- SiYuan is the canonical content store; SQLite content/title/tag fields remain compatibility snapshots/cache only in V4.1.
- Never hold the `StateDb` mutex across async HTTP/SiYuan operations.
- AI re-extraction must never overwrite a SiYuan knowledge document that changed since `generated_hash` was recorded.
- The embedded workbench accepts loopback SiYuan origins only.
- Keep the existing public-repository hygiene and secret-scan checks intact.
- Do not change `crates/aiks-core/src/ai/config.rs` as part of this feature.

---

### Task 1: Add V4.1 content binding and migration state schema

**Files:**
- Create: `crates/aiks-core/migrations/008_v41_siyuan_content_source.sql`
- Modify: `crates/aiks-core/src/storage/db.rs`
- Test: `crates/aiks-core/src/storage/db.rs`

**Interfaces:**
- Produces SQLite columns `knowledge_item.siyuan_doc_id`, `knowledge_item.generated_hash`, `knowledge_item.current_remote_hash`, `knowledge_item.migration_status`.
- Produces `source_session.siyuan_doc_id` when absent.
- Produces `content_migration` table used by Task 3.

- [ ] **Step 1: Add failing schema assertions**

Extend `open_creates_tables` so a fresh DB asserts these columns exist and `content_migration` exists:

```rust
let v41_columns: i64 = conn.query_row(
    "SELECT COUNT(*) FROM pragma_table_info('knowledge_item') WHERE name IN ('siyuan_doc_id','generated_hash','current_remote_hash','migration_status')",
    [],
    |row| row.get(0),
).unwrap();
assert_eq!(v41_columns, 4);

let migration_table: i64 = conn.query_row(
    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='content_migration'",
    [],
    |row| row.get(0),
).unwrap();
assert_eq!(migration_table, 1);
```

- [ ] **Step 2: Verify the test fails before migration code exists**

Run through CI/local workspace:

```bash
cargo test -p aiks-core storage::db::tests::open_creates_tables -- --exact
```

Expected: FAIL because V4.1 columns/table do not exist.

- [ ] **Step 3: Add idempotent migration SQL**

Create migration with additive-only schema:

```sql
CREATE TABLE IF NOT EXISTS content_migration (
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    target_doc_id TEXT,
    source_hash TEXT,
    target_hash TEXT,
    error_message TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(entity_type, entity_id)
);
```

Guard `ALTER TABLE` statements in Rust with `pragma_table_info` checks before executing the migration so existing V4 DBs upgrade safely.

- [ ] **Step 4: Run DB tests**

```bash
cargo test -p aiks-core storage::db::tests -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/aiks-core/migrations/008_v41_siyuan_content_source.sql crates/aiks-core/src/storage/db.rs
git commit -m "feat(v4.1): add SiYuan content binding schema"
```

### Task 2: Make SiYuanSink expose one-notebook content operations

**Files:**
- Modify: `crates/aiks-core/src/sink/siyuan.rs`
- Test: `crates/aiks-core/tests/v41_siyuan_contract.rs`

**Interfaces:**
- Produces `ensure_content_notebook() -> Result<String>`.
- Produces `create_knowledge_document(path, markdown) -> Result<String>`.
- Produces `move_documents_to_content_notebook(from_ids, to_path) -> Result<()>`.
- Produces `document_hash(doc_id) -> Result<String>`.
- Existing low-level API methods remain available.

- [ ] **Step 1: Add contract tests around path and attribute policy**

Test pure helpers without requiring a live kernel:

```rust
assert_eq!(SiYuanSink::knowledge_root(), "/20 Knowledge");
assert_eq!(SiYuanSink::session_root(), "/10 AI Sessions");
```

Add a helper test asserting V4.1 knowledge attrs include `custom-aiks-kind=knowledge` and `custom-aiks-generated-hash`.

- [ ] **Step 2: Run the contract test and confirm failure**

```bash
cargo test -p aiks-core --test v41_siyuan_contract -- --nocapture
```

Expected: FAIL because the V4.1 helpers are not defined.

- [ ] **Step 3: Implement single-notebook helpers**

`ensure_session_notebook()` becomes a compatibility wrapper returning the same notebook ID as `ensure_notebook()`. Keep old config fields readable but stop creating a second archive notebook in normal V4.1 paths.

Add canonical attribute constants:

```rust
pub const ATTR_AIKS_ID: &str = "custom-aiks-id";
pub const ATTR_MANAGED_BY: &str = "custom-aiks-managed-by";
pub const ATTR_SOURCE_TYPE: &str = "custom-aiks-source-type";
pub const ATTR_PROJECT: &str = "custom-aiks-project";
pub const ATTR_CATEGORY: &str = "custom-aiks-category";
pub const ATTR_GENERATED_HASH: &str = "custom-aiks-generated-hash";
```

- [ ] **Step 4: Run sink contract tests and existing sink tests**

```bash
cargo test -p aiks-core siyuan -- --nocapture
cargo test -p aiks-core --test v41_siyuan_contract -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/aiks-core/src/sink/siyuan.rs crates/aiks-core/tests/v41_siyuan_contract.rs
git commit -m "feat(v4.1): unify SiYuan content notebook operations"
```

### Task 3: Add resumable V4-to-V4.1 content migration service

**Files:**
- Create: `crates/aiks-core/src/knowledge/migration.rs`
- Modify: `crates/aiks-core/src/knowledge/mod.rs`
- Modify: `crates/aiks-core/src/lib.rs`
- Test: `crates/aiks-core/tests/v41_content_migration.rs`

**Interfaces:**

```rust
pub struct ContentMigrationService<'a> {
    db: &'a StateDb,
}

pub struct ContentMigrationStats {
    pub total: usize,
    pub migrated: usize,
    pub reused: usize,
    pub conflicts: usize,
    pub failed: usize,
}

impl<'a> ContentMigrationService<'a> {
    pub async fn migrate(&self, sink: &SiYuanSink) -> anyhow::Result<ContentMigrationStats>;
}
```

- [ ] **Step 1: Add failing tests for classification**

Cover:

```text
existing target_id + unchanged remote => reused
no target_id => create from legacy snapshot => migrated
local changed + remote changed from baseline => conflict
second run => no duplicate docs
```

Use a fake sink boundary or extracted migration planner so classification is deterministic without a live kernel.

- [ ] **Step 2: Verify failures**

```bash
cargo test -p aiks-core --test v41_content_migration -- --nocapture
```

Expected: FAIL before implementation.

- [ ] **Step 3: Implement migration state machine**

Use statuses exactly:

```text
pending
migrated
reused
conflict
failed
```

Do DB reads first, release DB lock, perform async SiYuan work, then persist the result. Existing `knowledge_sync_target.target_id` is reused as the first source of an existing remote doc ID.

- [ ] **Step 4: Run migration tests**

```bash
cargo test -p aiks-core --test v41_content_migration -- --nocapture
```

Expected: PASS, including idempotent rerun.

- [ ] **Step 5: Commit**

```bash
git add crates/aiks-core/src/knowledge/migration.rs crates/aiks-core/src/knowledge/mod.rs crates/aiks-core/src/lib.rs crates/aiks-core/tests/v41_content_migration.rs
git commit -m "feat(v4.1): add resumable SiYuan content migration"
```

### Task 4: Cut manual knowledge and AI publishing over to canonical SiYuan docs

**Files:**
- Modify: `crates/aiks-core/src/knowledge/workbench.rs`
- Modify: `crates/aiks-core/src/knowledge/publisher.rs`
- Modify: `crates/aiks-core/src/renderer/knowledge.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/knowledge_commands.rs`
- Test: `crates/aiks-core/tests/v4_native_knowledge.rs`
- Test: `crates/aiks-core/tests/v41_content_migration.rs`

**Interfaces:**
- Replace user-visible publish semantics with `create_manual_in_siyuan` and `open existing doc` semantics.
- `KnowledgeRecord` gains `siyuan_doc_id: Option<String>` and `generated_hash: Option<String>`.
- Renderer accepts `session_doc_id` and emits a real SiYuan block reference for conversation-derived knowledge.

- [ ] **Step 1: Update tests to require doc binding and edit protection**

Add assertions that after canonical binding:

```rust
assert_eq!(item.siyuan_doc_id.as_deref(), Some("doc-id"));
```

and that a changed remote hash produces a review/conflict result rather than an update.

- [ ] **Step 2: Verify tests fail**

```bash
cargo test -p aiks-core --test v4_native_knowledge -- --nocapture
cargo test -p aiks-core --test v41_content_migration -- --nocapture
```

- [ ] **Step 3: Implement canonical write path**

The Tauri command for manual creation performs:

```text
validate input -> create SiYuan doc under /20 Knowledge -> set attrs -> create/update SQLite control row with siyuan_doc_id
```

Do not create a SQLite-only manual document in V4.1 runtime paths.

AI publisher logic becomes the canonical update path; `publish_knowledge` remains only as compatibility naming until frontend removal.

- [ ] **Step 4: Run tests**

```bash
cargo test -p aiks-core --test v4_native_knowledge -- --nocapture
cargo test -p aiks-core --test v41_content_migration -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/aiks-core/src/knowledge apps/aiks-desktop/src-tauri/src/knowledge_commands.rs crates/aiks-core/src/renderer/knowledge.rs crates/aiks-core/tests
git commit -m "feat(v4.1): make SiYuan canonical for knowledge content"
```

### Task 5: Add versioned WorkbenchController and bridge commands

**Files:**
- Create: `apps/aiks-desktop/src-tauri/src/workbench/mod.rs`
- Create: `apps/aiks-desktop/src-tauri/src/workbench/protocol.rs`
- Create: `apps/aiks-desktop/src-tauri/src/workbench/controller.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/lib.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/lifecycle.rs`
- Test: `apps/aiks-desktop/src-tauri/src/workbench/protocol.rs`

**Interfaces:**

```rust
pub const BRIDGE_PROTOCOL_VERSION: u16 = 1;

pub enum WorkspaceMode { Knowledge, Session }

pub enum WorkbenchAction {
    ShowKnowledgeRoot,
    ShowSessionRoot,
    OpenDocument { doc_id: String },
    OpenBlock { doc_id: String, block_id: String },
    SetWorkspaceMode { mode: WorkspaceMode },
    ShowBacklinks { block_id: String },
    ShowOutline,
    ShowDatabase,
    ShowGraph,
    ShowSearch,
    RefreshDocument { doc_id: String },
}
```

Expose Tauri commands:

```text
show_workbench
hide_workbench
open_siyuan_document
open_siyuan_block
set_workbench_mode
get_workbench_status
```

- [ ] **Step 1: Add protocol validation tests**

Reject protocol versions other than `1`, missing IDs, non-loopback origins, and invalid mode strings.

- [ ] **Step 2: Run tests and verify failure**

```bash
cargo test -p aiks-desktop workbench::protocol -- --nocapture
```

- [ ] **Step 3: Implement controller state**

Controller owns only runtime UI state:

```rust
pub struct WorkbenchController {
    origin: Mutex<Option<Url>>,
    nonce: String,
    ready: AtomicBool,
    mode: Mutex<WorkspaceMode>,
}
```

No knowledge body is stored in the bridge.

- [ ] **Step 4: Register controller during lifecycle startup**

Once SiYuan base URL is known, set controller origin. If SiYuan is unavailable, workbench status reports unavailable and AIKS control pages remain functional.

- [ ] **Step 5: Run Rust unit tests and compile checks**

```bash
cargo test -p aiks-desktop workbench -- --nocapture
cargo check -p aiks-desktop --no-default-features
```

- [ ] **Step 6: Commit**

```bash
git add apps/aiks-desktop/src-tauri/src/workbench apps/aiks-desktop/src-tauri/src/lib.rs apps/aiks-desktop/src-tauri/src/lifecycle.rs
git commit -m "feat(v4.1): add embedded workbench bridge controller"
```

### Task 6: Add AIKS Bridge Plugin resource and SiYuan UI adapter

**Files:**
- Create: `apps/aiks-desktop/src-tauri/resources/siyuan/data/plugins/aiks-bridge/plugin.json`
- Create: `apps/aiks-desktop/src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.js`
- Create: `apps/aiks-desktop/src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.css`
- Test: `apps/aiks-desktop/src/api/workbench.test.ts`

**Interfaces:**
- Plugin exposes `window.__AIKS_BRIDGE__` with protocol version `1`.
- Plugin receives actions from the parent/workbench channel and emits sanitized events only.
- `SiyuanAdapter` methods: `openDocument`, `focusBlock`, `setReadOnly`, `showBacklinks`, `showOutline`, `showGraph`, `applyAiksLayout`.

- [ ] **Step 1: Add frontend bridge contract test**

Test expected action names and workspace modes without loading SiYuan:

```ts
expect(WORKBENCH_ACTIONS).toContain("openDocument");
expect(WORKBENCH_ACTIONS).toContain("showGraph");
expect(WORKSPACE_MODES).toEqual(["knowledge", "session"]);
```

- [ ] **Step 2: Verify failure**

```bash
cd apps/aiks-desktop && npm test -- workbench.test.ts
```

- [ ] **Step 3: Implement plugin and adapter**

Knowledge mode keeps editing enabled. Session mode applies read-only interception to editing actions while leaving selection, copy, fold, outline, backlink, graph and search navigation available.

The adapter must fail open: if a selector/API capability is unavailable after a SiYuan upgrade, remove AIKS-specific trimming and leave the complete SiYuan workbench visible rather than blanking the page.

- [ ] **Step 4: Run frontend tests/build**

```bash
cd apps/aiks-desktop
npm test
npm run build
```

- [ ] **Step 5: Commit**

```bash
git add apps/aiks-desktop/src-tauri/resources/siyuan/data/plugins/aiks-bridge apps/aiks-desktop/src/api/workbench.test.ts
git commit -m "feat(v4.1): add SiYuan bridge plugin and layout adapter"
```

### Task 7: Replace V4 native editor/detail with embedded workbench routing

**Files:**
- Create: `apps/aiks-desktop/src/pages/KnowledgeWorkspacePage.tsx`
- Create: `apps/aiks-desktop/src/components/WorkbenchHost.tsx`
- Modify: `apps/aiks-desktop/src/App.tsx`
- Modify: `apps/aiks-desktop/src/api/types.ts`
- Modify: `apps/aiks-desktop/src/api/index.ts`
- Modify: `apps/aiks-desktop/src/api/tauri.ts`
- Modify: `apps/aiks-desktop/src/api/mock.ts`
- Modify: `apps/aiks-desktop/src/pages/SearchPage.tsx`
- Test: `apps/aiks-desktop/src/api/workbench.test.ts`

**Interfaces:**

```ts
export type WorkspaceMode = "knowledge" | "session";
export interface WorkbenchStatus {
  available: boolean;
  ready: boolean;
  mode: WorkspaceMode;
  origin: string | null;
}
```

`AiksApi` gains:

```ts
getWorkbenchStatus(): Promise<WorkbenchStatus>;
showWorkbench(mode: WorkspaceMode): Promise<void>;
hideWorkbench(): Promise<void>;
openSiyuanDocument(docId: string, mode: WorkspaceMode): Promise<void>;
openSiyuanBlock(docId: string, blockId: string, mode: WorkspaceMode): Promise<void>;
```

- [ ] **Step 1: Add failing API mapping tests**

Assert Tauri invokes exact command names and passes camelCase payload fields expected by Tauri.

- [ ] **Step 2: Verify failure**

```bash
cd apps/aiks-desktop && npm test -- workbench.test.ts
```

- [ ] **Step 3: Implement WorkbenchHost and knowledge workspace tabs**

The knowledge page top navigation becomes:

```text
知识 | 原始会话 | 数据库 | 图谱
```

`知识` and `原始会话` display the persistent embedded workbench in the corresponding mode. Database/Graph dispatch bridge actions rather than reimplementing these views.

- [ ] **Step 4: Remove normal runtime dependency on `KnowledgeEditor`/`KnowledgeDetailPage` body rendering**

Keep compatibility components temporarily in the tree, but App routing must open `siyuan_doc_id` when one exists. Search results use doc/block binding to open the embedded workbench.

- [ ] **Step 5: Run frontend tests/build**

```bash
cd apps/aiks-desktop
npm test
npm run build
```

- [ ] **Step 6: Commit**

```bash
git add apps/aiks-desktop/src
git commit -m "feat(v4.1): route knowledge and sessions through embedded workbench"
```

### Task 8: Add provenance navigation and raw-session protection

**Files:**
- Modify: `crates/aiks-core/src/renderer/knowledge.rs`
- Modify: `crates/aiks-core/src/sync/engine.rs` or the actual raw-session sync module owning SiYuan writes
- Modify: `apps/aiks-desktop/src-tauri/src/workbench/controller.rs`
- Test: `crates/aiks-core/tests/v41_provenance.rs`

**Interfaces:**
- Conversation knowledge renderer emits `((<session-doc-id> "查看原始 Session"))` when session doc ID exists.
- Raw sync compares current remote content hash to last managed hash before overwrite.
- A modified raw document returns `RawSessionConflict { session_id, doc_id }` instead of overwriting.

- [ ] **Step 1: Add failing provenance tests**

```rust
assert!(markdown.contains("查看原始 Session"));
assert!(markdown.contains("((session-doc-id"));
```

Add raw-session hash policy tests for unchanged vs modified remote docs.

- [ ] **Step 2: Verify failure**

```bash
cargo test -p aiks-core --test v41_provenance -- --nocapture
```

- [ ] **Step 3: Implement provenance and protection**

For unexpected session edits, preserve the remote document and surface conflict state. Do not silently discard user text.

- [ ] **Step 4: Run tests**

```bash
cargo test -p aiks-core --test v41_provenance -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add crates/aiks-core/src crates/aiks-core/tests/v41_provenance.rs apps/aiks-desktop/src-tauri/src/workbench/controller.rs
git commit -m "feat(v4.1): add knowledge provenance and session protection"
```

### Task 9: Run startup migration, cache invalidation, and index refresh

**Files:**
- Modify: `apps/aiks-desktop/src-tauri/src/lifecycle.rs`
- Modify: `crates/aiks-core/src/pipeline/search.rs`
- Modify: `crates/aiks-core/src/knowledge/workbench.rs`
- Test: `crates/aiks-core/tests/v41_content_migration.rs`

**Interfaces:**
- Startup performs migration only when SiYuan is healthy and migration is incomplete.
- `documentChanged` invalidates cached metadata/chunks/embeddings for the bound knowledge ID and schedules a debounced re-read/re-index.
- Search results carry `siyuan_doc_id` and optional `siyuan_block_id` for navigation.

- [ ] **Step 1: Add failing migration-resume and invalidation tests**

Cover interrupted migration resuming from persisted statuses and content change invalidating embeddings without deleting canonical SiYuan content.

- [ ] **Step 2: Verify failures**

```bash
cargo test -p aiks-core --test v41_content_migration -- --nocapture
```

- [ ] **Step 3: Wire startup migration and re-index scheduling**

Migration failures are logged and surfaced in diagnostics but must not prevent the AIKS control shell from opening.

- [ ] **Step 4: Run targeted tests**

```bash
cargo test -p aiks-core --test v41_content_migration -- --nocapture
cargo test -p aiks-core pipeline::search -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add apps/aiks-desktop/src-tauri/src/lifecycle.rs crates/aiks-core/src/pipeline/search.rs crates/aiks-core/src/knowledge/workbench.rs crates/aiks-core/tests/v41_content_migration.rs
git commit -m "feat(v4.1): migrate and reindex canonical SiYuan content"
```

### Task 10: Full regression, Windows compile, E2E diagnostics and cutover cleanup

**Files:**
- Modify: `apps/aiks-desktop/src/pages/DiagnosticsPage.tsx`
- Modify: `apps/aiks-desktop/src/api/types.ts`
- Modify: `docs/superpowers/specs/2026-09-16-aiks-v4-1-siyuan-embedded-workbench-design.md` only if implementation facts require clarification
- Keep: V4 legacy editor/publisher compatibility code until V4.2 unless it becomes unreachable dead code with warnings.

**Interfaces:**
- Diagnostics exposes: `SiYuan ready`, `Workbench ready`, `Bridge protocol`, `content migration totals`, `conflicts`, `failed`, and current workspace mode.

- [ ] **Step 1: Run formatting and Rust quality gates**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 2: Run Windows compile gate**

```powershell
cargo check -p aiks-desktop --no-default-features
```

Expected: PASS on Windows CI.

- [ ] **Step 3: Run frontend gates**

```bash
cd apps/aiks-desktop
npm ci
npm test
npm run build
```

Expected: PASS.

- [ ] **Step 4: Verify V4.1 E2E manually with embedded kernel**

Required assertions:

```text
AIKS starts with one main product shell.
Knowledge -> opens /20 Knowledge in embedded SiYuan.
Raw Session -> opens /10 AI Sessions read-only.
Knowledge source reference opens the correct Session.
Session backlinks expose derived Knowledge.
Manual + New Knowledge creates SiYuan doc immediately.
Block editor, outline, backlink, database and graph are available.
User-edited AI knowledge is not overwritten by re-extraction.
Existing V4 published knowledge reuses the same doc ID.
Migration can rerun without duplicate documents.
SiYuan/Bridge failure leaves Overview/Sources/Settings usable.
```

- [ ] **Step 5: Commit final diagnostics and cleanup**

```bash
git add apps/aiks-desktop/src docs/superpowers/specs/2026-09-16-aiks-v4-1-siyuan-embedded-workbench-design.md
git commit -m "test(v4.1): verify embedded SiYuan workbench cutover"
```
