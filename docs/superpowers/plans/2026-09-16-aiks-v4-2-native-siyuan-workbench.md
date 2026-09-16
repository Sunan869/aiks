# AIKS V4.2 Native SiYuan Workbench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the V4.1 “full SiYuan embedded inside AIKS” experience with a productized AIKS-native knowledge workspace backed by a thin `aiks-siyuan` fork, while keeping SiYuan canonical for user content and AIKS canonical for control/process state and derived retrieval indexes.

**Architecture:** AIKS keeps the Tauri shell, control pages, source/pipeline state, read models, embeddings, and Agent/RAG retrieval. A separate `Sunan869/aiks-siyuan` fork is pinned to SiYuan `v3.8.3` (`8641553a1f07374001902d3ce773285db1292b2d`) and adds only an AIKS embedded profile, AIKS visual tokens, compact workbench layout, bridge integration, and read-only AI Conversation Records behavior. AIKS builds and bundles the customized frontend and kernel from that same pinned commit.

**Tech Stack:** Rust, Tauri 2, React 18, TypeScript, Vitest, SQLite/rusqlite, SiYuan 3.8.3, Go, pnpm 11.25.0, Webpack, Windows desktop packaging.

**Spec:** `docs/superpowers/specs/2026-09-16-aiks-v4-2-native-siyuan-workbench-design.md`

## Global Constraints

- Preserve existing V4.1 SiYuan document IDs and migration state.
- Keep internal compatibility roots `/10 AI Sessions` and `/20 Knowledge`; expose them as `AI 对话记录` and `知识` in the product UI.
- `AI 对话记录` is read-only in normal user flows.
- SiYuan is canonical for knowledge/session content, titles, user-editable metadata, and document hierarchy.
- AIKS is canonical for processing state, provenance/control state, derived search indexes, chunks, and embeddings.
- Do not restore SQLite `knowledge_item.content/title/tags` as an authoritative write source.
- User-facing search uses SiYuan Workbench search; AIKS FTS/embedding hybrid search remains for Agent/RAG/Pipeline use.
- Do not fork Protyle, Attribute View, graph, block-reference semantics, or SiYuan storage format unless an upstream limitation is proven.
- Build customized SiYuan frontend and kernel from the same pinned upstream commit.
- Disable SiYuan self-update in the AIKS runtime.
- Keep bridge actions/events versioned, loopback-constrained, nonce-validated, and identifier-validated.
- V4.2 release engineering must include AGPL-3.0 source/license obligations for the modified bundled SiYuan runtime.

---

### Task 1: Pin the SiYuan runtime contract and AIKS runtime metadata

**Files (AIKS repo):**
- Create: `apps/aiks-desktop/src-tauri/resources/siyuan-runtime.json`
- Modify: `apps/aiks-desktop/src-tauri/src/diagnostics.rs`
- Modify: `apps/aiks-desktop/src/api/types.ts`
- Modify: `apps/aiks-desktop/src/pages/DiagnosticsPage.tsx`
- Test: `apps/aiks-desktop/src-tauri/src/diagnostics.rs`

**Interfaces:**
- Produces a single runtime manifest bundled with AIKS:

```json
{
  "workbenchVersion": "4.2.0",
  "siyuanBaseVersion": "3.8.3",
  "siyuanUpstreamCommit": "8641553a1f07374001902d3ce773285db1292b2d",
  "bridgeProtocolVersion": 2
}
```

- Diagnostics exposes the four values above plus current Workbench readiness/mode.

- [ ] **Step 1: Add failing Rust test for bundled runtime metadata**

Add a test that loads `resources/siyuan-runtime.json` with `include_str!` and asserts exact base version, exact pinned SHA, and bridge protocol `2`.

```rust
#[test]
fn bundled_siyuan_runtime_is_pinned() {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../resources/siyuan-runtime.json"
    ))
    .unwrap();
    assert_eq!(manifest["siyuanBaseVersion"], "3.8.3");
    assert_eq!(
        manifest["siyuanUpstreamCommit"],
        "8641553a1f07374001902d3ce773285db1292b2d"
    );
    assert_eq!(manifest["bridgeProtocolVersion"], 2);
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p aiks-desktop bundled_siyuan_runtime_is_pinned -- --exact
```

Expected: FAIL because `siyuan-runtime.json` does not exist.

- [ ] **Step 3: Add the runtime manifest and expose it through diagnostics**

Create the exact JSON above. Extend the diagnostics DTO with:

```rust
pub workbench_version: String,
pub siyuan_base_version: String,
pub siyuan_upstream_commit: String,
pub bridge_protocol_version: u16,
```

Mirror these fields into `apps/aiks-desktop/src/api/types.ts` and render them in Diagnostics.

- [ ] **Step 4: Run focused and frontend tests**

```bash
cargo test -p aiks-desktop bundled_siyuan_runtime_is_pinned -- --exact
cd apps/aiks-desktop && npm test && npm run build
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/aiks-desktop/src-tauri/resources/siyuan-runtime.json \
  apps/aiks-desktop/src-tauri/src/diagnostics.rs \
  apps/aiks-desktop/src/api/types.ts \
  apps/aiks-desktop/src/pages/DiagnosticsPage.tsx
git commit -m "feat(v4.2): pin SiYuan runtime metadata"
```

### Task 2: Bootstrap the independent `aiks-siyuan` fork and centralized embedded profile

**Repository:** `Sunan869/aiks-siyuan` (independent fork of `siyuan-note/siyuan`)

**Pinned upstream:**

```text
Tag: v3.8.3
Commit: 8641553a1f07374001902d3ce773285db1292b2d
```

**Files (aiks-siyuan repo):**
- Create: `app/src/aiks/profile.ts`
- Create: `app/src/aiks/profile.test.ts`
- Create: `app/src/aiks/index.ts`
- Modify: `app/src/index.ts`
- Modify: `app/package.json`

**Interfaces:**

```ts
export interface AiksEmbeddedProfile {
  enabled: boolean;
  productName: "AIKS Knowledge Workbench";
  hidePluginMarketplace: boolean;
  hidePluginManagement: boolean;
  hideThemeMarketplace: boolean;
  hideSiyuanAi: boolean;
  hideAgent: boolean;
  hideMcp: boolean;
  hideAccount: boolean;
  hideCloudSync: boolean;
  hideSubscription: boolean;
  hideCommunity: boolean;
  hideSelfUpdate: boolean;
  readOnlyRootPaths: string[];
}

export function getAiksEmbeddedProfile(): AiksEmbeddedProfile;
export function isAiksEmbedded(): boolean;
```

Runtime activation rule:

```text
SIYUAN_PROFILE=aiks-embedded
```

- [ ] **Step 1: Create the fork from exact upstream base**

Use the GitHub fork UI/API, then verify locally:

```bash
git clone https://github.com/Sunan869/aiks-siyuan.git
cd aiks-siyuan
git remote add upstream https://github.com/siyuan-note/siyuan.git
git fetch upstream --tags
git checkout -b aiks/v4.2 8641553a1f07374001902d3ce773285db1292b2d
```

Expected:

```bash
git rev-parse HEAD
# 8641553a1f07374001902d3ce773285db1292b2d
```

- [ ] **Step 2: Write failing embedded-profile tests**

```ts
import test from "node:test";
import assert from "node:assert/strict";
import { resolveAiksEmbeddedProfile } from "./profile";

test("aiks embedded profile disables standalone product surfaces", () => {
  const profile = resolveAiksEmbeddedProfile("aiks-embedded");
  assert.equal(profile.enabled, true);
  assert.equal(profile.hidePluginMarketplace, true);
  assert.equal(profile.hideSiyuanAi, true);
  assert.equal(profile.hideCloudSync, true);
  assert.deepEqual(profile.readOnlyRootPaths, ["/10 AI Sessions"]);
});
```

- [ ] **Step 3: Verify RED**

```bash
cd app
pnpm install --frozen-lockfile
pnpm test
```

Expected: FAIL because `profile.ts` is missing.

- [ ] **Step 4: Implement the centralized profile resolver**

`resolveAiksEmbeddedProfile("aiks-embedded")` returns all standalone-product visibility flags as disabled and `/10 AI Sessions` as the read-only root. `isAiksEmbedded()` reads the build/runtime value from one centralized adapter only; feature code must not read `process.env` directly.

- [ ] **Step 5: Wire profile initialization at `app/src/index.ts`**

Before standalone shell initialization, load the profile once and export it through the AIKS module. Do not hide individual features here; Task 3 consumes the profile.

- [ ] **Step 6: Run SiYuan frontend gates**

```bash
cd app
pnpm test
pnpm run typecheck
pnpm run build:app
```

Expected: PASS.

- [ ] **Step 7: Commit in `aiks-siyuan`**

```bash
git add app/src/aiks app/src/index.ts app/package.json
git commit -m "feat(aiks): add embedded runtime profile"
```

### Task 3: Build the AIKS-native SiYuan shell and remove standalone product chrome

**Files (aiks-siyuan repo):**
- Create: `app/src/aiks/layout.ts`
- Create: `app/src/aiks/roots.ts`
- Create: `app/src/aiks/search.ts`
- Create: `app/src/aiks/theme.scss`
- Create: `app/src/aiks/layout.test.ts`
- Modify: `app/src/layout/*` only at the smallest entry points required to select AIKS layout
- Modify: `app/src/menus/*` only at centralized menu-construction entry points
- Modify: `app/src/config/*` only where the standalone Settings entry is registered

**Interfaces:**

```ts
export type AiksWorkbenchMode = "document" | "database" | "graph";

export interface AiksRootPresentation {
  path: string;
  label: string;
  readOnly: boolean;
}

export const AIKS_ROOTS: AiksRootPresentation[] = [
  { path: "/10 AI Sessions", label: "AI 对话记录", readOnly: true },
  { path: "/20 Knowledge", label: "知识", readOnly: false },
];
```

The AIKS shell layout must expose:

```text
Document tree | main work area | Outline/Backlinks/Properties
```

The top toolbar provides search and `Document / Database / Graph` mode actions.

- [ ] **Step 1: Add failing tests for product-root labels and feature visibility**

```ts
assert.deepEqual(AIKS_ROOTS.map(root => root.label), ["AI 对话记录", "知识"]);
assert.equal(AIKS_ROOTS[0].readOnly, true);
assert.equal(AIKS_ROOTS[1].readOnly, false);
```

Add a layout test asserting `pluginMarketplace`, `agent`, `mcp`, `cloudSync`, `account`, `subscription`, `community`, and `selfUpdate` are not part of the AIKS toolbar/menu model.

- [ ] **Step 2: Verify RED**

```bash
cd app && pnpm test
```

- [ ] **Step 3: Implement the AIKS layout model**

Use existing SiYuan document tree, Protyle editor, Attribute View, graph, search, outline, backlinks, properties, and history implementations. Only replace the standalone product composition layer.

Required behavior:

```text
left secondary panel: document tree
center: one primary document/database/graph work area
right panel: Outline | Backlinks | Properties, collapsible
```

Multi-tab controls are hidden/de-emphasized in AIKS profile, but underlying upstream tab/editor primitives are not deleted.

- [ ] **Step 4: Remove standalone product surfaces through profile-aware registration**

Do not implement this as CSS selectors over existing rendered controls. Prevent irrelevant product entries from being registered/rendered when `isAiksEmbedded()` is true.

- [ ] **Step 5: Apply AIKS visual tokens**

`theme.scss` defines shared CSS variables for AIKS-compatible background, border, typography, spacing, radius, hover/active states, and dark mode. Avoid per-component hardcoded overrides when a token can be used.

- [ ] **Step 6: Verify retained native capabilities**

```bash
cd app
pnpm test
pnpm run typecheck
pnpm run build:app
```

Manually smoke the development build and confirm search, editor, outline, backlinks, properties, Attribute View, graph, history, block refs, and slash commands remain reachable.

- [ ] **Step 7: Commit in `aiks-siyuan`**

```bash
git add app/src/aiks app/src/layout app/src/menus app/src/config
git commit -m "feat(aiks): add native knowledge workbench shell"
```

### Task 4: Enforce read-only `AI 对话记录` without making navigation read-only

**Files (aiks-siyuan repo):**
- Create: `app/src/aiks/readonly.ts`
- Create: `app/src/aiks/readonly.test.ts`
- Modify: the smallest Protyle lifecycle hook needed to call the AIKS read-only adapter

**Interfaces:**

```ts
export function isAiksReadOnlyPath(path: string): boolean;
export function applyAiksDocumentReadOnly(editor: unknown, path: string): void;
```

Read-only behavior blocks body mutation but keeps:

```text
selection
copy
fold/unfold
outline
backlinks
graph
search
block-reference navigation
```

- [ ] **Step 1: Write failing pure policy tests**

Cover exact path and descendants:

```ts
assert.equal(isAiksReadOnlyPath("/10 AI Sessions"), true);
assert.equal(isAiksReadOnlyPath("/10 AI Sessions/Claude/foo"), true);
assert.equal(isAiksReadOnlyPath("/20 Knowledge/foo"), false);
```

- [ ] **Step 2: Verify RED**

```bash
cd app && pnpm test
```

- [ ] **Step 3: Implement the policy and editor adapter**

Prefer Protyle’s supported disable/read-only API where available. Add event interception only as a defense-in-depth fallback; do not block non-mutating navigation/selection events.

- [ ] **Step 4: Add an integration regression test or deterministic adapter test**

Assert session editor receives read-only/disable treatment and knowledge editor does not.

- [ ] **Step 5: Run gates and commit**

```bash
cd app && pnpm test && pnpm run typecheck && pnpm run build:app
git add app/src/aiks app/src/protyle
git commit -m "feat(aiks): protect AI conversation records"
```

### Task 5: Simplify the AIKS shell navigation and route all end-user knowledge search into Workbench

**Files (AIKS repo):**
- Modify: `apps/aiks-desktop/src/App.tsx`
- Modify: `apps/aiks-desktop/src/components/Sidebar.tsx`
- Modify: `apps/aiks-desktop/src/pages/KnowledgeWorkspacePage.tsx`
- Modify: `apps/aiks-desktop/src/components/WorkbenchHost.tsx`
- Modify: `apps/aiks-desktop/src/api/workbench.ts`
- Modify: `apps/aiks-desktop/src/api/workbench.test.ts`
- Keep but stop routing to: `apps/aiks-desktop/src/pages/SearchPage.tsx`

**Interfaces:**

User-visible `Page` becomes:

```ts
export type Page =
  | "overview"
  | "sessions"
  | "knowledge"
  | "processing"
  | "sources"
  | "settings"
  | "diagnostics";
```

Workbench API adds:

```ts
openWorkbenchSearch(): Promise<void>;
restoreWorkbenchLocation(): Promise<void>;
showWorkbenchMode(mode: "document" | "database" | "graph"): Promise<void>;
```

- [ ] **Step 1: Update frontend tests first**

Add assertions that `search` is no longer a sidebar/page route and that Ctrl/Cmd+K maps to Workbench search.

- [ ] **Step 2: Verify RED**

```bash
cd apps/aiks-desktop
npm test -- workbench.test.ts
```

- [ ] **Step 3: Remove Search from the AIKS navigation/routing surface**

Keep backend hybrid search APIs intact. Do not delete search engine code.

- [ ] **Step 4: Make Knowledge open the Workbench directly**

Default resolution order:

```text
last opened document
last workbench location
Knowledge root
minimal welcome surface if empty
```

Remove the V4.1 top-level `知识 / 原始会话 / 数据库 / 图谱` tabs from `KnowledgeWorkspacePage`; those concerns now live inside the customized Workbench.

- [ ] **Step 5: Route global Ctrl/Cmd+K to Workbench search**

When invoked outside Knowledge, show the Workbench search surface; selecting a result navigates into Knowledge and focuses the matching document/block.

- [ ] **Step 6: Run frontend tests/build and commit**

```bash
cd apps/aiks-desktop
npm test
npm run build
```

```bash
git add apps/aiks-desktop/src
git commit -m "feat(v4.2): make knowledge workbench the user search surface"
```

### Task 6: Introduce V4.2 metadata semantics and Inbox lifecycle without path coupling

**Files (AIKS repo):**
- Create: `crates/aiks-core/migrations/009_v42_knowledge_metadata.sql`
- Modify: `crates/aiks-core/src/storage/db.rs`
- Modify: `crates/aiks-core/src/knowledge/mod.rs`
- Modify: `crates/aiks-core/src/knowledge/workbench.rs`
- Modify: `crates/aiks-core/src/sink/v41.rs`
- Test: `crates/aiks-core/tests/v42_metadata.rs`

**Interfaces:**

System metadata semantics:

```text
aiks-id
aiks-kind = knowledge | session
aiks-source-session-id
aiks-source-type
aiks-origin = ai | manual
aiks-user-modified = true | false
aiks-generated-hash
```

Business metadata:

```text
project
category
tags
status = draft | active | archived | conflict
```

SQLite control/read model gains the minimum additive columns required to mirror origin/modification/lifecycle state; path is never the business identifier.

- [ ] **Step 1: Add failing schema and behavior tests**

Test fresh DB contains V4.2 columns and that moving a document path does not change its `knowledge_id`, project, category, or provenance.

- [ ] **Step 2: Verify RED**

```bash
cargo test -p aiks-core --test v42_metadata -- --nocapture
```

- [ ] **Step 3: Add additive migration and metadata DTOs**

Do not remove V4.1 columns in this release. Keep compatibility reads where required.

- [ ] **Step 4: Change new AI knowledge default placement**

Creation priority:

```text
explicit directory
project default directory
/20 Knowledge/Projects/<sanitized project>
/20 Knowledge/Inbox
```

AI-generated items start `draft`; manual items may start `active` unless the calling flow explicitly requests draft.

- [ ] **Step 5: Stop deriving category/project identity from physical path**

Path remains display/organization state only.

- [ ] **Step 6: Run tests and commit**

```bash
cargo test -p aiks-core --test v42_metadata -- --nocapture
cargo test -p aiks-core --test v41_content_migration -- --nocapture
```

```bash
git add crates/aiks-core/migrations/009_v42_knowledge_metadata.sql \
  crates/aiks-core/src/storage/db.rs \
  crates/aiks-core/src/knowledge \
  crates/aiks-core/src/sink/v41.rs \
  crates/aiks-core/tests/v42_metadata.rs
git commit -m "feat(v4.2): add user-owned knowledge metadata semantics"
```

### Task 7: Add non-destructive AI update candidates for user-modified knowledge

**Files (AIKS repo):**
- Create: `crates/aiks-core/migrations/010_v42_update_candidates.sql`
- Create: `crates/aiks-core/src/knowledge/update_candidate.rs`
- Modify: `crates/aiks-core/src/knowledge/publisher.rs`
- Modify: `crates/aiks-core/src/knowledge/mod.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/knowledge_commands.rs`
- Modify: `apps/aiks-desktop/src/api/types.ts`
- Test: `crates/aiks-core/tests/v42_update_candidates.rs`

**Interfaces:**

```rust
pub struct KnowledgeUpdateCandidate {
    pub id: String,
    pub knowledge_id: String,
    pub source_session_id: Option<i64>,
    pub base_hash: String,
    pub generated_content: String,
    pub generated_at: String,
    pub status: String, // pending | applied | ignored
}
```

Decision rule:

```text
current_siyuan_hash == generated_hash -> update canonical document directly
current_siyuan_hash != generated_hash -> leave canonical document unchanged and create/update candidate
```

- [ ] **Step 1: Write failing policy tests**

Cover automatic update for untouched content and candidate creation for user-modified content.

- [ ] **Step 2: Verify RED**

```bash
cargo test -p aiks-core --test v42_update_candidates -- --nocapture
```

- [ ] **Step 3: Add additive candidate schema and repository/service**

Use one active `pending` candidate per knowledge item/source update cycle; do not create `-v2` documents.

- [ ] **Step 4: Integrate publisher decision**

Fetch canonical SiYuan hash before writing. Never use cached SQLite body to decide that the remote document is unchanged.

- [ ] **Step 5: Expose apply/ignore commands**

Applying writes candidate content to canonical SiYuan and then updates generated hash/read model. Ignoring marks the candidate `ignored` and changes no canonical content.

- [ ] **Step 6: Run tests and commit**

```bash
cargo test -p aiks-core --test v42_update_candidates -- --nocapture
cargo test -p aiks-core --test v41_provenance -- --nocapture
```

```bash
git add crates/aiks-core/migrations/010_v42_update_candidates.sql \
  crates/aiks-core/src/knowledge \
  apps/aiks-desktop/src-tauri/src/knowledge_commands.rs \
  apps/aiks-desktop/src/api/types.ts \
  crates/aiks-core/tests/v42_update_candidates.rs
git commit -m "feat(v4.2): protect user edits with AI update candidates"
```

### Task 8: Upgrade the Workbench bridge to protocol v2 and remove UI-trimming responsibilities

**Files (AIKS repo):**
- Modify: `apps/aiks-desktop/src-tauri/src/workbench/protocol.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/workbench/controller.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/workbench/commands.rs`
- Modify: `apps/aiks-desktop/src-tauri/src/workbench/events.rs`
- Modify: `apps/aiks-desktop/src/api/workbench.ts`
- Modify: `apps/aiks-desktop/src/api/workbench.test.ts`

**Files (aiks-siyuan repo):**
- Create/Modify: `app/src/aiks/bridge.ts`
- Test: `app/src/aiks/bridge.test.ts`

**Interfaces:**

Bridge protocol `2` supports:

```text
openDocument
openBlock
showRoot
openSearch
setWorkbenchMode(document|database|graph)
setReadOnlyContext
getRuntimeInfo
documentChanged
bridgeReady
provenanceNavigate
```

The bridge no longer carries actions whose purpose is to hide standalone SiYuan chrome with DOM selectors.

- [ ] **Step 1: Update Rust and TypeScript protocol tests to expect version 2**

Reject version 1 at the V4.2 bridge boundary, invalid identifiers, non-loopback origins, unknown actions/events, and wrong nonce.

- [ ] **Step 2: Verify RED in both repos**

```bash
cargo test -p aiks-desktop workbench::protocol -- --nocapture
cd apps/aiks-desktop && npm test -- workbench.test.ts
```

```bash
cd aiks-siyuan/app && pnpm test
```

- [ ] **Step 3: Implement protocol v2 symmetrically**

Keep payloads primitive/sanitized and document IDs/block IDs validated before backend use.

- [ ] **Step 4: Retain V4.1 documentChanged -> read-model refresh**

Do not regress the existing path that reads canonical markdown, refreshes FTS/read model, clears stale chunks/embeddings, and rebuilds derived indexes.

- [ ] **Step 5: Run tests and commit in each repo**

AIKS:

```bash
cargo test -p aiks-desktop workbench -- --nocapture
cd apps/aiks-desktop && npm test && npm run build
git add apps/aiks-desktop/src-tauri/src/workbench apps/aiks-desktop/src/api
git commit -m "feat(v4.2): upgrade workbench bridge protocol"
```

`aiks-siyuan`:

```bash
cd app && pnpm test && pnpm run typecheck && pnpm run build:app
git add app/src/aiks/bridge.ts app/src/aiks/bridge.test.ts
git commit -m "feat(aiks): add AIKS bridge protocol v2"
```

### Task 9: Build and bundle frontend + kernel from the same pinned `aiks-siyuan` commit

**Files (AIKS repo):**
- Create: `scripts/build-aiks-siyuan.ps1`
- Create: `scripts/verify-siyuan-runtime.ps1`
- Modify: `apps/aiks-desktop/src-tauri/tauri.conf.json`
- Modify: `.github/workflows/ci.yml`

**Files (aiks-siyuan repo):**
- Modify only if needed: `scripts/win-build.bat`
- Do not change kernel source solely for packaging.

**Interfaces:**

The PowerShell build script accepts:

```powershell
-RepoPath <path-to-aiks-siyuan>
-ExpectedCommit 8641553a1f07374001902d3ce773285db1292b2d-or-approved-aiks-fork-descendant
-OutDir <aiks runtime resource directory>
```

It must fail when frontend/kernel artifacts do not originate from the same checked-out commit.

- [ ] **Step 1: Add verification script testable failure cases**

`verify-siyuan-runtime.ps1` fails on missing manifest, wrong base SHA, missing frontend artifact, or missing kernel executable.

- [ ] **Step 2: Build customized frontend**

From `aiks-siyuan/app`:

```bash
pnpm install --frozen-lockfile
pnpm run build:app
```

- [ ] **Step 3: Build Windows kernel from the same checkout**

Use SiYuan’s pinned source and its Windows build requirements. The upstream build path uses Go with `fts5 sqlcipher` tags and produces `SiYuan-Kernel.exe`; preserve those tags.

- [ ] **Step 4: Copy only required runtime assets into AIKS resources**

Do not bundle an independent self-updating Electron SiYuan desktop app. Bundle the kernel and web assets used by the AIKS-managed runtime.

- [ ] **Step 5: Verify runtime manifest and artifacts before Tauri packaging**

```powershell
./scripts/verify-siyuan-runtime.ps1
```

Expected: exit `0` only when manifest, frontend, kernel, version, and commit contract match.

- [ ] **Step 6: Add CI gates**

At minimum, AIKS CI must verify the runtime manifest contract even when full upstream source build is delegated to a release/runtime pipeline. Release CI must build both artifacts from the same `aiks-siyuan` checkout.

- [ ] **Step 7: Commit**

```bash
git add scripts apps/aiks-desktop/src-tauri/tauri.conf.json .github/workflows/ci.yml
git commit -m "build(v4.2): bundle pinned AIKS SiYuan runtime"
```

### Task 10: V4.1 -> V4.2 migration compatibility, full regression, and release obligations

**Files (AIKS repo):**
- Modify: `apps/aiks-desktop/src-tauri/src/lifecycle.rs`
- Modify: `apps/aiks-desktop/src/pages/DiagnosticsPage.tsx`
- Create: `docs/licenses/SIYUAN-AGPL-NOTICE.md`
- Modify: release/package documentation where license notices are assembled
- Test: `crates/aiks-core/tests/v42_migration.rs`

**Interfaces:**

Migration keeps:

```text
existing siyuan_doc_id
existing /10 AI Sessions and /20 Knowledge roots
existing content_migration records
existing generated_hash/current_remote_hash guards
existing source provenance
```

It adds V4.2 metadata/lifecycle defaults without moving canonical documents unnecessarily.

- [ ] **Step 1: Add failing migration compatibility tests**

Create a V4.1-shaped DB fixture with bound SiYuan doc IDs and assert V4.2 migration preserves those IDs and classifies roots/metadata without generating replacement IDs.

- [ ] **Step 2: Verify RED**

```bash
cargo test -p aiks-core --test v42_migration -- --nocapture
```

- [ ] **Step 3: Implement additive migration/defaulting**

Existing AI-generated knowledge receives `aiks-origin=ai`; existing manual knowledge receives `manual` where determinable. Do not mark all old knowledge user-modified merely because V4.1 used `managed_by=user` for conservative conflict protection; preserve conflict safety and derive new flags conservatively.

- [ ] **Step 4: Add AGPL notice/release checklist**

Document the exact SiYuan base version/SHA, location of corresponding modified source, AGPL-3.0 license reference, and how recipients can obtain the source corresponding to the shipped runtime.

- [ ] **Step 5: Run full AIKS gates**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --test-threads=1
cd apps/aiks-desktop
npm ci
npm test
npm run build
```

Windows gate:

```powershell
cargo check -p aiks-desktop --no-default-features
./scripts/verify-siyuan-runtime.ps1
```

Expected: PASS.

- [ ] **Step 6: Run `aiks-siyuan` gates**

```bash
cd aiks-siyuan/app
pnpm install --frozen-lockfile
pnpm test
pnpm run typecheck
pnpm run build:app
```

Build the kernel from the same checkout and verify runtime packaging.

- [ ] **Step 7: Manual V4.2 E2E acceptance**

Required assertions:

```text
AIKS keeps one primary shell/sidebar.
Knowledge opens directly into the workbench.
The standalone AIKS Search nav item is absent.
Workbench search finds Knowledge and AI Conversation Records and opens matching blocks.
AI Conversation Records is visible under that label and cannot be edited.
Knowledge documents remain editable.
Users can move/rename directories without breaking AIKS identity/provenance.
New AI knowledge lands in Knowledge/Inbox as draft.
Project default folders are used when configured.
Category/tags/project remain editable metadata.
Outline, Backlinks, Properties work in the auxiliary panel.
Database and Graph work as main-area modes.
Work Records -> View original conversation opens the matching read-only document.
Processing -> View generated knowledge opens the canonical knowledge document.
User-modified AI knowledge is never overwritten by re-extraction.
Untouched AI knowledge may be auto-updated and receives a new generated hash.
A modified document gets a reviewable AI update candidate instead of a duplicate document.
Existing V4.1 SiYuan doc IDs are preserved.
SiYuan self-update/account/cloud/plugin/AI/Agent/MCP/subscription/community UI is absent.
If Workbench/SiYuan fails, Overview/Work Records/Processing/Data Sources/Settings/Diagnostics remain usable.
Diagnostics show Workbench version, SiYuan base version, upstream commit, bridge protocol, mode, and migration/index status.
```

- [ ] **Step 8: Commit final migration/release work**

```bash
git add apps/aiks-desktop/src-tauri/src/lifecycle.rs \
  apps/aiks-desktop/src/pages/DiagnosticsPage.tsx \
  crates/aiks-core/tests/v42_migration.rs \
  docs/licenses
git commit -m "test(v4.2): verify native SiYuan workbench cutover"
```
