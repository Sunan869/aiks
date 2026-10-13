# AIKS V4 Native Knowledge Workbench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn AIKS Desktop into a native local-first knowledge workbench with manual knowledge CRUD, user-edit protection, favorites/archive/search, and SiYuan as an optional publisher rather than a second embedded product.

**Architecture:** `knowledge_item` remains the canonical knowledge entity for both AI-extracted and manual knowledge. Core owns CRUD/reconciliation/search semantics; Tauri only exposes Core services; React provides native list/detail/editor UX. SiYuan remains an external publish target using the existing mapping/baseline/conflict infrastructure.

**Tech Stack:** Rust, rusqlite/SQLite FTS5, Tauri 2, React 18, TypeScript, Vite, embedded SiYuan HTTP API.

**Spec:** `docs/superpowers/specs/2026-09-15-aiks-v4-native-knowledge-workbench-design.md`

## Global Constraints

- AIKS is the primary knowledge product and source of truth.
- Manual and conversation-derived knowledge share `knowledge_item`.
- Manual knowledge has no `source_session_id` and must never participate in session reconciliation.
- Once a user edits conversation-derived knowledge, `managed_by` becomes `user`; re-extraction must not silently overwrite or delete it.
- Archived knowledge stays locally stored but is excluded from normal search and default list views.
- SiYuan is an optional publisher/integration target, not the primary editor or primary knowledge page action.
- No direct writes to SiYuan SQLite or `.sy` files.
- Windows desktop compilation remains a permanent CI gate.

---

### Task 1: V4 knowledge schema and Core CRUD

**Files:**
- Create: `crates/aiks-core/migrations/007_v4_native_knowledge.sql`
- Modify: `crates/aiks-core/src/storage/db.rs`
- Create: `crates/aiks-core/src/knowledge/workbench.rs`
- Modify: `crates/aiks-core/src/knowledge/mod.rs`
- Modify: `crates/aiks-core/src/lib.rs`
- Test: `crates/aiks-core/tests/v4_native_knowledge.rs`

**Interfaces:**
- Produces: `KnowledgeService`, `CreateKnowledgeInput`, `UpdateKnowledgeInput`, `KnowledgeListFilter`, `KnowledgeRecord`.
- Produces methods: `create_manual`, `update`, `set_favorite`, `archive`, `restore`, `get`, `list`.

- [ ] Write tests proving manual knowledge can be created with `source_session_id = NULL`, `source_type = manual`, `managed_by = user`.
- [ ] Write tests proving update refreshes FTS and invalidates chunks/embeddings.
- [ ] Write tests proving favorite/archive/restore state changes are persisted.
- [ ] Add guarded V4 migration and make migrations idempotent.
- [ ] Implement Core CRUD transactionally with FTS maintenance and tag normalization.

### Task 2: Protect user-edited knowledge during AI re-extraction

**Files:**
- Modify: `crates/aiks-core/src/pipeline/knowledge_repo.rs`
- Test: `crates/aiks-core/tests/v4_native_knowledge.rs`

**Interfaces:**
- Consumes: V4 `managed_by`, `source_type`, `status` fields.
- Produces: safe `save_items` reconciliation.

- [ ] Add a test where an AI item is user-edited and then re-extracted with the same semantic key; keep the user content and ID.
- [ ] Add a test where a user-edited AI item no longer semantically matches; preserve it while allowing a new pipeline item.
- [ ] Reconcile/delete only `managed_by = pipeline` rows for the session.
- [ ] Do not invalidate chunks/FTS for matched user-managed rows.

### Task 3: V4 search semantics

**Files:**
- Modify: `crates/aiks-core/src/pipeline/search.rs`
- Modify: `crates/aiks-core/src/pipeline/knowledge_repo.rs`
- Test: `crates/aiks-core/tests/v4_native_knowledge.rs`

**Interfaces:**
- Normal search returns only `status = active` knowledge.
- Manual and conversation-derived active knowledge share the same FTS/vector path.

- [ ] Add tests for manual knowledge search.
- [ ] Add tests proving archived knowledge is excluded from search.
- [ ] Filter FTS, LIKE, and vector candidates to active knowledge.

### Task 4: Tauri V4 knowledge API

**Files:**
- Modify: `apps/aiks-desktop/src-tauri/src/commands.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Produces commands: `create_knowledge`, `update_knowledge`, `set_knowledge_favorite`, `archive_knowledge`, `restore_knowledge`, `publish_knowledge`.
- Extends: `list_knowledge`, `get_knowledge_detail`.

- [ ] Route CRUD through `KnowledgeService`; keep SQL out of the new Tauri handlers.
- [ ] Extend list filters with `source_type`, `favorite`, `status`.
- [ ] Return source/management/favorite/archive metadata in summaries/details.
- [ ] Add single-item publish command using the existing SiYuan publisher mapping/conflict model.

### Task 5: Desktop native knowledge workbench

**Files:**
- Modify: `apps/aiks-desktop/src/api/types.ts`
- Modify: `apps/aiks-desktop/src/api/index.ts`
- Modify: `apps/aiks-desktop/src/api/tauri.ts`
- Modify: `apps/aiks-desktop/src/api/mock.ts`
- Create: `apps/aiks-desktop/src/components/KnowledgeEditor.tsx`
- Create: `apps/aiks-desktop/src/pages/KnowledgeWorkbenchPage.tsx`
- Modify: `apps/aiks-desktop/src/pages/KnowledgeDetailPage.tsx`
- Modify: `apps/aiks-desktop/src/App.tsx`

**Interfaces:**
- Main knowledge page exposes `+ 新建知识`.
- Filters: all / AI extracted / manual / favorites / archived plus category.
- Detail actions: edit, favorite, archive/restore, publish to SiYuan.

- [ ] Add reusable Markdown textarea editor with preview and title/category/project/tags/summary fields.
- [ ] Add manual-create flow backed by `create_knowledge`.
- [ ] Replace the V3 knowledge page with the native workbench.
- [ ] Add source badges and favorite/archive metadata.
- [ ] Upgrade detail page to native CRUD actions and manual-vs-conversation source cards.
- [ ] Keep SiYuan publish secondary; remove bulk SiYuan sync as the knowledge page primary CTA.

### Task 6: Verification

**Files:**
- Modify documentation only if implementation behavior differs from the V4 spec.

- [ ] Run GitHub Actions frontend tests/build.
- [ ] Run Rust fmt, Clippy `-D warnings`, and workspace tests.
- [ ] Run Windows desktop compile CI.
- [ ] Verify migration idempotency and existing V3 data upgrade.
- [ ] Verify no full SiYuan web workspace was introduced into the V4 primary UX.
