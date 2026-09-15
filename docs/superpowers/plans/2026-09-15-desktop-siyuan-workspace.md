# Desktop SiYuan Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reuse the embedded SiYuan native workspace inside AIKS Desktop for manual create/edit/favorite/archive/search while preserving AIKS extraction, tracing, sync, and hybrid search.

**Architecture:** Keep SiYuan as the mature authoring/organization surface and AIKS as the work-knowledge pipeline. Add a thin React integration surface backed by the existing Tauri runtime URL and existing `open_knowledge_window` fallback; do not add duplicate KnowledgeItem state or direct SiYuan storage access.

**Tech Stack:** React 18, TypeScript, Vite, Tauri 2, Rust, embedded SiYuan HTTP runtime.

**Spec:** `docs/superpowers/specs/2026-09-15-desktop-siyuan-workspace-design.md`

## Global Constraints

- SiYuan integration uses the runtime Web UI/public integration only; never modify SiYuan SQLite or `.sy` files directly.
- Do not add AIKS favorite/archive/editor persistence that duplicates SiYuan.
- Desktop Tauri calls go through `AiksApi` rather than new page-level `invoke()` calls.
- Existing AIKS KnowledgeItem, search, sync conflict/baseline, and pipeline semantics remain unchanged.
- Embedded workspace URLs must be loopback-only.

---

### Task 1: Unified SiYuan workspace API

**Files:**
- Modify: `apps/aiks-desktop/src/api/index.ts`
- Modify: `apps/aiks-desktop/src/api/tauri.ts`
- Modify: `apps/aiks-desktop/src/api/mock.ts`
- Test: `apps/aiks-desktop/src/api/siyuan-workspace.test.ts`

**Interfaces:**
- Consumes: existing Tauri command `open_knowledge_window` and `get_siyuan_url`.
- Produces: `AiksApi.openSiyuanWorkspace(): Promise<void>`.

- [x] **Step 1: Write the failing test**

```ts
const api = new MockAiksApi() as unknown as Record<string, unknown>;
expect(api.openSiyuanWorkspace).toEqual(expect.any(Function));
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test`
Expected: `siyuan-workspace.test.ts` fails because `openSiyuanWorkspace` is undefined.

- [ ] **Step 3: Implement the API method**

Add the method to the interface, invoke `open_knowledge_window` in Tauri, and make Mock resolve without side effects.

- [ ] **Step 4: Run test to verify it passes**

Run: `npm test`
Expected: all frontend tests pass.

### Task 2: Trusted embedded-workspace URL policy

**Files:**
- Create: `apps/aiks-desktop/src/features/siyuan/workspace.ts`
- Create: `apps/aiks-desktop/src/features/siyuan/workspace.test.ts`

**Interfaces:**
- Produces: `normalizeSiyuanWorkspaceUrl(raw: string | null): string | null`.

- [ ] **Step 1: Write failing URL-policy tests**

Cover `127.0.0.1`, `localhost`, `[::1]`, trailing slash normalization, malformed URLs, and remote hosts.

- [ ] **Step 2: Verify RED with `npm test`**

- [ ] **Step 3: Implement minimal loopback-only normalizer**

Use the platform `URL` parser; allow only HTTP/HTTPS loopback hosts and return an origin URL without a trailing slash.

- [ ] **Step 4: Verify GREEN with `npm test`**

### Task 3: Embedded SiYuan workspace component

**Files:**
- Create: `apps/aiks-desktop/src/components/SiYuanWorkspace.tsx`

**Interfaces:**
- Consumes: `getApi().getSiyuanUrl()`, `getApi().openSiyuanWorkspace()`, `normalizeSiyuanWorkspaceUrl`, `shouldUseMock()`.
- Produces: reusable full-height SiYuan native workspace surface.

- [ ] **Step 1: Implement loading/not-ready/untrusted/mock states using the tested URL policy**
- [ ] **Step 2: Render the SiYuan runtime in an iframe with a thin AIKS toolbar**
- [ ] **Step 3: Add reload and independent-window fallback actions**
- [ ] **Step 4: Run `npm run build` and fix TypeScript/build errors**

### Task 4: Knowledge page integration

**Files:**
- Modify: `apps/aiks-desktop/src/pages/KnowledgeBasePageV3.tsx`

**Interfaces:**
- Consumes: `SiYuanWorkspace`.
- Produces: `AI 提炼` / `SiYuan 工作区` segmented views in one Desktop page.

- [ ] **Step 1: Add the two-view navigation without changing existing AI knowledge-list behavior**
- [ ] **Step 2: Keep sync/category controls scoped to AI 提炼 view**
- [ ] **Step 3: Render SiYuan workspace full-height in the second view**
- [ ] **Step 4: Run frontend tests/build**

### Task 5: Search-page integration

**Files:**
- Modify: `apps/aiks-desktop/src/pages/SearchPage.tsx`

**Interfaces:**
- Consumes: `SiYuanWorkspace`.
- Produces: explicit `AIKS 搜索` / `SiYuan 全库` views.

- [ ] **Step 1: Preserve existing AIKS search unchanged in the AIKS view**
- [ ] **Step 2: Add the SiYuan full-library view with explanatory copy**
- [ ] **Step 3: Run frontend tests/build**

### Task 6: Documentation and full verification

**Files:**
- Modify: `TODO.md` only if its current roadmap incorrectly says Desktop must reimplement CRUD/search.

- [ ] **Step 1: Run `npm test` and `npm run build`**
- [ ] **Step 2: Run/observe repository CI: Rust formatting, Clippy, `cargo test --workspace`, frontend, hygiene**
- [ ] **Step 3: Confirm no migration/schema/core search semantics changed**
- [ ] **Step 4: Mark PR ready only after all checks are green**
