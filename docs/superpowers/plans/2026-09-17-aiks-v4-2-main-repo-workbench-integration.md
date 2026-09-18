# AIKS V4.2 Main-Repo Workbench Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Connect the AIKS desktop shell to the productized `aiks-siyuan` V4.2 Workbench so Knowledge becomes the single user-facing content workspace, the duplicate AIKS Search page disappears, raw AI conversations navigate into Knowledge, and Graph/Search actions use native Workbench capabilities.

**Architecture:** Keep the existing persistent Tauri child webview and bridge protocol v1. AIKS owns navigation and control pages; the embedded SiYuan runtime owns document browsing/editing/search/database/graph surfaces. The main repo only adds routing, a compact Workbench toolbar, and bridge adapters; it does not reimplement SiYuan search, Attribute View, Graph, or editor internals.

**Tech Stack:** React 18, TypeScript 5.4, Vitest, Tauri 2 / Rust, SiYuan plugin bridge JavaScript.

**Spec:** `docs/superpowers/specs/2026-09-16-aiks-v4-2-native-siyuan-workbench-design.md`

## Global Constraints

- AIKS primary navigation remains visible while Knowledge is open.
- Remove only the user-facing AIKS Search page/navigation entry; keep internal FTS/embedding search APIs for Agent/RAG/Pipeline use.
- Knowledge opens the Workbench directly; do not add another AIKS knowledge dashboard/list layer.
- Work Records remains the management/control view. Raw conversation content opens under the Knowledge route in `session` mode and remains read-only.
- Main Workbench center modes are `document`, `database`, and `graph`. Search is a global Workbench action, not a center mode.
- Reuse SiYuan native Database/Attribute View, Graph, search, document tree, Outline, Backlinks, and Properties surfaces.
- Preserve physical compatibility roots `/10 AI Sessions` and `/20 Knowledge`; product labels are handled by the embedded runtime.
- Preserve bridge protocol version `1` for this integration task.
- Graph must prefer `window.aiksWorkbench.openGraph()` from the customized V4.2 runtime and retain a selector fallback for compatibility.
- Do not modify SiYuan kernel/editor/search/graph/storage internals in the main repo.

---

### Task 1: Product Navigation and Raw-Conversation Route

**Files:**
- Create: `apps/aiks-desktop/src/navigation.ts`
- Create: `apps/aiks-desktop/src/navigation.test.ts`
- Modify: `apps/aiks-desktop/src/App.tsx`
- Modify: `apps/aiks-desktop/src/components/Sidebar.tsx`
- Modify: `apps/aiks-desktop/src/pages/SessionDetailPage.tsx`

**Interfaces:**
- Produces `Page`, `NavState`, `MAIN_NAV_ITEMS`, `BOTTOM_NAV_ITEMS`.
- Produces `rawConversationNavState(docId?: string | null): NavState`.
- `SessionDetailPage` adds `onViewRawConversation: (docId: string | null) => void`.

- [ ] **Step 1: Write the failing navigation contract**

```ts
import { describe, expect, it } from "vitest";
import {
  MAIN_NAV_ITEMS,
  BOTTOM_NAV_ITEMS,
  rawConversationNavState,
} from "./navigation";

describe("V4.2 product navigation", () => {
  it("removes the duplicate user-facing Search page", () => {
    expect(MAIN_NAV_ITEMS.map(item => item.id)).toEqual([
      "overview",
      "sessions",
      "knowledge",
      "processing",
    ]);
    expect([...MAIN_NAV_ITEMS, ...BOTTOM_NAV_ITEMS].some(item => item.id === "search")).toBe(false);
  });

  it("routes raw conversations into Knowledge session mode", () => {
    expect(rawConversationNavState("session-doc-1")).toEqual({
      page: "knowledge",
      workbenchMode: "session",
      workbenchDocId: "session-doc-1",
    });
    expect(rawConversationNavState(null)).toEqual({
      page: "knowledge",
      workbenchMode: "session",
    });
  });
});
```

- [ ] **Step 2: Run frontend tests and verify RED**

Run in PR CI: `npm test -- --run` from `apps/aiks-desktop`.

Expected: the new suite fails because `./navigation` does not exist.

- [ ] **Step 3: Implement the navigation model and wire the shell**

Create `navigation.ts` with:

```ts
export type Page =
  | "overview"
  | "sessions"
  | "knowledge"
  | "processing"
  | "sources"
  | "settings"
  | "diagnostics";

export type WorkbenchRouteMode = "knowledge" | "session";

export interface NavState {
  page: Page;
  sessionDetailId?: number;
  knowledgeDetailId?: string;
  pipelineDetailRunId?: string;
  workbenchMode?: WorkbenchRouteMode;
  workbenchDocId?: string;
}

export const MAIN_NAV_ITEMS = [
  { id: "overview", label: "概览" },
  { id: "sessions", label: "工作记录" },
  { id: "knowledge", label: "知识库" },
  { id: "processing", label: "处理中心" },
] as const;

export const BOTTOM_NAV_ITEMS = [
  { id: "sources", label: "数据源" },
  { id: "settings", label: "设置" },
  { id: "diagnostics", label: "帮助与诊断" },
] as const;

export function rawConversationNavState(docId?: string | null): NavState {
  return {
    page: "knowledge",
    workbenchMode: "session",
    ...(docId?.trim() ? { workbenchDocId: docId.trim() } : {}),
  };
}
```

Then:
- move the `Page`/`NavState` ownership from `App.tsx` to `navigation.ts`;
- remove `SearchPage` import and `case "search"`;
- use `rawConversationNavState()` for the Session detail callback;
- render Knowledge with route `workbenchMode` / `workbenchDocId`;
- update the visible version badge to `V4.2 Workbench`;
- have `Sidebar.tsx` render the navigation model with its existing icon mapping and remove the Search icon;
- remove the embedded raw-conversation Workbench from `SessionDetailPage`; add a `查看原始对话` button that invokes `onViewRawConversation(sessionDocId)`.

- [ ] **Step 4: Run frontend tests/build and verify GREEN**

Run in PR CI:

```bash
npm test -- --run
npm run build
```

Expected: both exit 0.

---

### Task 2: Single Knowledge Workbench and Native Search Action

**Files:**
- Create: `apps/aiks-desktop/src/knowledge-workspace.ts`
- Create: `apps/aiks-desktop/src/knowledge-workspace.test.ts`
- Modify: `apps/aiks-desktop/src/pages/KnowledgeWorkspacePage.tsx`
- Modify: `apps/aiks-desktop/src/api/index.ts`
- Modify: `apps/aiks-desktop/src/api/tauri.ts`
- Modify: `apps/aiks-desktop/src/api/session-workbench.test.ts`
- Modify: `apps/aiks-desktop/src/api/workbench.test.ts`
- Modify: `apps/aiks-desktop/src/api/workbench.ts`

**Interfaces:**
- Produces `WorkbenchMainMode = "document" | "database" | "graph"`.
- Produces `WORKBENCH_MAIN_MODES` and `resolveWorkbenchSurface(mainMode, workspaceMode)`.
- `AiksApi` gains `showWorkbenchSearch(): Promise<void>`.

- [ ] **Step 1: Write failing Workbench mode/search tests**

Create `knowledge-workspace.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { WORKBENCH_MAIN_MODES, resolveWorkbenchSurface } from "./knowledge-workspace";

describe("V4.2 Knowledge Workbench modes", () => {
  it("uses only Document Database and Graph as center modes", () => {
    expect(WORKBENCH_MAIN_MODES).toEqual(["document", "database", "graph"]);
  });

  it("keeps raw conversations in document mode and maps native main surfaces", () => {
    expect(resolveWorkbenchSurface("document", "knowledge")).toBe("knowledge");
    expect(resolveWorkbenchSurface("document", "session")).toBe("session");
    expect(resolveWorkbenchSurface("database", "session")).toBe("database");
    expect(resolveWorkbenchSurface("graph", "knowledge")).toBe("graph");
  });
});
```

Change the Session routing test to:

```ts
expect(shouldKeepWorkbenchMounted("knowledge", false)).toBe(true);
expect(shouldKeepWorkbenchMounted("sessions", true)).toBe(false);
```

Add to `workbench.test.ts`:

```ts
it("opens native SiYuan search through the Rust bridge", async () => {
  await new TauriAiksApi().showWorkbenchSearch();
  expect(invokeMock.mock.calls).toEqual([
    ["show_workbench"],
    ["show_workbench_search"],
  ]);
});
```

- [ ] **Step 2: Run frontend tests and verify RED**

Run: `npm test -- --run`.

Expected failures:
- missing `knowledge-workspace.ts`;
- Session detail still keeps the Workbench mounted;
- `showWorkbenchSearch` is absent.

- [ ] **Step 3: Implement minimal mode/search support**

Create:

```ts
import type { WorkbenchSurface, WorkspaceMode } from "./api/workbench";

export const WORKBENCH_MAIN_MODES = ["document", "database", "graph"] as const;
export type WorkbenchMainMode = (typeof WORKBENCH_MAIN_MODES)[number];

export function resolveWorkbenchSurface(
  mainMode: WorkbenchMainMode,
  workspaceMode: WorkspaceMode,
): WorkbenchSurface {
  return mainMode === "document" ? workspaceMode : mainMode;
}
```

Then:
- simplify `shouldKeepWorkbenchMounted()` so only the Knowledge route keeps the child Workbench mounted;
- add `showWorkbenchSearch()` to `AiksApi` and `TauriAiksApi`, implemented as `show_workbench` then `show_workbench_search`;
- rewrite `KnowledgeWorkspacePage` as a direct full-height Workbench surface with a compact AIKS toolbar;
- toolbar center modes: `文档`, `数据库`, `图谱`;
- toolbar Search button invokes `getApi().showWorkbenchSearch()` without changing `mainMode`;
- when `knowledgeId` resolves a `siyuan_doc_id`, Document mode opens it in `knowledge` mode;
- when route mode is `session`, Document mode passes the routed `workbenchDocId` and remains read-only through the existing bridge mode;
- remove the V4.1 `知识 / 原始会话 / 数据库 / 图谱` outer-tab UI and explanatory product layer.

- [ ] **Step 4: Run frontend tests/build and verify GREEN**

Run:

```bash
npm test -- --run
npm run build
```

Expected: both exit 0.

---

### Task 3: Graph Host API Bridge Compatibility

**Files:**
- Modify: `apps/aiks-desktop/src/api/workbench.test.ts`
- Modify: `apps/aiks-desktop/src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.js`

**Interfaces:**
- Consumes the customized runtime API `window.aiksWorkbench.openGraph(): Promise<boolean>` from `aiks-siyuan` V4.2.
- Preserves the existing protocol action name `showGraph` and selector fallback.

- [ ] **Step 1: Write the failing bridge integration test**

Add to the bridge contract suite:

```ts
it("prefers the V4.2 center Graph host API before legacy dock selectors", () => {
  expect(pluginSource).toContain("window.aiksWorkbench");
  expect(pluginSource).toContain("openGraph");
  expect(pluginSource.indexOf("openGraph")).toBeLessThan(pluginSource.indexOf("#barGraph"));
});
```

- [ ] **Step 2: Run frontend tests and verify RED**

Run: `npm test -- --run`.

Expected: the new source-contract test fails because the bridge still goes directly to `#barGraph`/Dock selectors.

- [ ] **Step 3: Implement host-API-first Graph opening**

Change `SiyuanAdapter.showGraph()` so it:
1. calls `window.aiksWorkbench?.openGraph()` when available;
2. treats a resolved `true` as success;
3. falls back to the existing Graph selectors when the API is absent, rejects, throws, or resolves `false`;
4. returns immediately without removing the selector fallback, preserving compatibility with the V4.1/standard runtime.

The implementation must remain asynchronous internally without changing the protocol action name.

- [ ] **Step 4: Run frontend tests/build and verify GREEN**

Run:

```bash
npm test -- --run
npm run build
```

Expected: both exit 0.

---

### Task 4: Integration Verification and Review Gate

**Files:**
- No feature files beyond fixes required by verification.

**Interfaces:**
- Validates the V4.2 main-repo integration against the approved spec and existing V4.1 regression suite.

- [ ] **Step 1: Run the full PR CI gates**

Required evidence:
- Repository hygiene: success
- Windows desktop compile: success
- Rust `fmt`: success
- Rust `clippy`: success
- Rust tests: success
- Desktop frontend tests: success
- Desktop frontend build: success

- [ ] **Step 2: Inspect the PR diff against the spec**

Confirm:
- no visible Search navigation/page route remains;
- internal `searchKnowledge`/hybrid-search APIs are not removed;
- Session detail does not embed raw conversation content;
- `查看原始对话` routes to `page: "knowledge", workbenchMode: "session"`;
- Knowledge has one direct Workbench surface and a compact toolbar;
- Search is an action, not a fourth center mode;
- Graph prefers the new center host API;
- no SiYuan kernel/Protyle/search/graph implementation is copied into React.

- [ ] **Step 3: Check review threads/comments**

No Important/Critical unresolved review finding may remain before marking the task complete.
