# AIKS V4.2 Native SiYuan Workbench Design

## Status

Approved design direction based on V4.1 embedded-workbench validation.

## Goal

Evolve the V4.1 "AIKS Shell + embedded full SiYuan" prototype into a productized AIKS knowledge workspace that feels native to AIKS while retaining SiYuan's mature knowledge engine capabilities.

The target architecture is:

```text
AIKS Desktop
  ├─ AIKS Shell / Control Plane
  └─ AIKS Knowledge Workbench
       └─ customized aiks-siyuan runtime
            ├─ customized SiYuan frontend shell
            └─ SiYuan kernel built from the same pinned upstream commit
```

AIKS remains the application shell and control plane. SiYuan remains the canonical knowledge/content engine. The user should not feel that a second standalone product has been embedded inside AIKS.

## Product Boundary

### AIKS owns

AIKS continues to own the product shell and operational/control experiences:

- Overview
- Work records / sessions management
- Processing center / pipelines
- Data sources
- Settings
- Diagnostics / help
- Provider and AI configuration
- Extraction and processing state
- Search/index services used by Agent/RAG and automation

The AIKS left-side primary navigation remains visible while the knowledge workbench is open.

### AIKS Knowledge Workbench owns

Once the user enters Knowledge, the right-side main content area becomes a dedicated knowledge workspace powered by the customized SiYuan runtime.

The workbench owns:

- document tree
- document/block editing
- SiYuan search
- block references
- backlinks
- outline
- document properties
- Attribute View / database
- graph
- document history
- document/block navigation
- read-only browsing of raw AI conversations

AIKS must not duplicate these user-facing capabilities with a second knowledge browser/search/editor implementation.

## Repository Strategy

Maintain SiYuan customization as an independent fork, for example:

```text
Sunan869/aiks
Sunan869/aiks-siyuan
```

`aiks-siyuan` tracks `siyuan-note/siyuan` and contains only the minimum AIKS-specific frontend/runtime integration patches.

Do not vendor the complete SiYuan source tree directly into the AIKS repository.

### Upstream-first patch policy

The fork should remain as close as possible to upstream. AIKS changes should be concentrated in a small number of layers:

1. AIKS embedded runtime profile
2. AIKS-specific frontend shell/layout
3. AIKS design tokens/theme
4. AIKS bridge/integration hooks
5. AIKS read-only-root behavior

Avoid arbitrary scattered changes across upstream source files.

The following areas are considered protected upstream implementation zones and should remain unchanged unless an upstream limitation makes a change unavoidable:

- kernel knowledge/storage semantics
- Protyle editor core
- Attribute View engine
- search engine
- graph engine
- block reference semantics
- native SiYuan document/storage format

## Build and Packaging

AIKS must build both the customized SiYuan frontend and the SiYuan kernel from the same pinned upstream commit.

Do not combine a customized frontend from one revision with an official or independently upgraded kernel binary from another revision.

Each AIKS release records at least:

```text
AIKS version
AIKS Workbench version
SiYuan base version
SiYuan upstream commit SHA
Bridge protocol version
```

The resulting verified Knowledge Engine Runtime is bundled with AIKS Desktop.

SiYuan self-update is disabled in the AIKS runtime. SiYuan runtime updates are delivered only as part of an AIKS release.

## Upstream Upgrade Policy

Do not follow upstream `master` continuously and do not ship upstream beta/RC releases by default.

Upgrade flow:

```text
stable upstream SiYuan release
  -> select/pin upstream commit
  -> merge/rebase aiks-siyuan patches
  -> build frontend + kernel from same commit
  -> run aiks-siyuan integration gates
  -> run AIKS integration/E2E gates
  -> release through AIKS
```

AIKS does not need to follow every stable SiYuan release. Upgrade when there is a security reason, important bug fix, required capability, or planned runtime refresh.

## Embedded Runtime Profile

Create an explicit runtime/build profile, conceptually:

```text
standard
aiks-embedded
```

The implementation may use a build flag, environment value, runtime configuration, or equivalent mechanism, but it must be centralized rather than implemented through scattered DOM/CSS hacks.

The `aiks-embedded` profile controls:

- branding
- layout
- menus
- enabled feature surfaces
- hidden/disabled product functions
- read-only roots
- bridge behavior
- AIKS theme/design tokens

The V4.1 bridge plugin may remain where it provides a clean integration boundary, but V4.2 should stop relying on fragile selectors merely to hide large portions of the standalone SiYuan product UI.

## Workbench Information Architecture

### AIKS primary navigation

The user-visible AIKS sidebar becomes:

```text
Overview
Work Records
Knowledge
Processing Center

Data Sources
Settings
Help & Diagnostics
```

Remove the dedicated user-facing AIKS Search tab.

AIKS hybrid search remains internally available for Agent/RAG/Pipeline use; only the separate end-user search page/navigation entry is removed.

### Knowledge entry behavior

Knowledge is a workspace, not a list page.

On opening Knowledge:

1. restore the last opened document when possible
2. otherwise restore the last workbench location/directory
3. on first use open the Knowledge root
4. when the workspace is completely empty, show a minimal welcome surface

The welcome surface only needs quick actions such as:

- Create Knowledge
- Open AI Conversation Records
- Search

Do not add another dashboard/list layer before the workbench.

## Workbench Layout

The default workbench layout is deliberately narrower than standalone SiYuan:

```text
AIKS primary sidebar | document tree | editor/main view | auxiliary panel
```

### Left workbench panel

The SiYuan document tree is retained as secondary navigation inside Knowledge.

User-facing roots are:

```text
AI Conversation Records  [read-only]
Knowledge
```

Physical/internal compatibility paths may remain:

```text
/10 AI Sessions
/20 Knowledge
```

The numeric/internal names are not exposed as product labels.

### Center work area

Default mode is single-document editing using Protyle.

Multi-tab behavior is removed or strongly de-emphasized. AIKS should feel like one integrated workspace rather than a nested standalone note application.

The top workbench toolbar stays minimal and aligned to AIKS styling. It may include:

- back / forward
- current document/path
- global knowledge search
- Document / Database / Graph mode controls
- auxiliary-panel toggle
- document more menu

Editor formatting should continue to rely primarily on Protyle contextual/floating controls and slash commands rather than a large permanent formatting toolbar.

### Right auxiliary panel

Only three primary surfaces are retained:

```text
Outline | Backlinks | Properties
```

Default: Outline.

The panel is collapsible.

History is accessed from document actions rather than occupying the auxiliary panel.

Database and Graph are not right-panel tools; they are main work-area modes.

## Main Workbench Modes

The workbench has three primary center-area modes:

```text
Document
Database
Graph
```

Search is a global action, not a fourth mode.

### Database

Database uses SiYuan Attribute View directly. AIKS does not create another table/database product.

Where practical, entering Database from a project/document context may pre-filter or focus the relevant project, but the underlying feature remains native SiYuan Attribute View.

### Graph

Graph uses SiYuan native graph capabilities. Selecting a node opens the corresponding document/block in Document mode.

Selecting a raw conversation node opens it in read-only mode.

## Search

End-user knowledge search is implemented using SiYuan search inside the Workbench.

The workbench top area exposes a unified search entry. A global shortcut such as `Ctrl/Cmd + K` may open this search even when the user is outside the Knowledge page; selecting a result navigates into Knowledge and opens the matching document/block.

Default user-visible search scope includes:

- document title
- block/body content
- tags
- document properties
- Knowledge
- AI Conversation Records

Recommended lightweight filters:

```text
All
Knowledge
AI Conversation Records
Project
Category
Tag
Time
```

Do not expose the full complexity of advanced SiYuan search syntax as the primary UI, though advanced capabilities may remain available underneath.

Selecting a result should open the document, scroll to the matched block, and visually focus/highlight the result where possible.

### Internal AIKS search remains

AIKS keeps its existing FTS/embedding hybrid search as an internal read model for:

- Agent/RAG retrieval
- processing pipelines
- automated related-knowledge operations

The removal applies only to the duplicate user-facing AIKS Search page/navigation entry.

## Feature Surface Reduction

### Retain

- document tree
- Protyle editor
- native SiYuan search
- block references / bidirectional links
- backlinks
- outline
- properties
- Attribute View / database
- graph
- history
- standard block editing operations
- copy/fold/navigation
- limited useful import/export where required

### De-emphasize or adapt

- multi-tab editing
- standalone left/right dock model
- templates as a primary feature
- broad import/export surfaces
- low-frequency standalone-product actions

### Remove from AIKS embedded UI

- plugin marketplace
- plugin management UI
- theme marketplace
- SiYuan AI UI
- Agent UI
- MCP UI
- SiYuan account UI
- cloud sync UI
- subscription/payment UI
- community/Bazaar entry points
- SiYuan update UI
- most standalone SiYuan settings

Underlying kernel modules do not have to be deleted merely because their UI is disabled. Prefer not initializing/exposing irrelevant surfaces over deep removal that would increase upstream merge cost.

## Settings Model

There is one product settings entry: AIKS Settings.

Do not expose a second standalone SiYuan Settings center.

Knowledge editor preferences that must remain configurable should be surfaced under an AIKS settings section such as:

```text
Settings -> Knowledge -> Editor
```

Examples:

- font
- font size
- editor width
- code block preferences
- editing/Markdown behavior
- editor shortcuts where applicable

AIKS writes or maps these preferences to the underlying SiYuan configuration.

## Knowledge Tree and User Ownership

### Default initialization

Initialize a useful structure without making path structure authoritative.

Recommended initial Knowledge tree:

```text
Knowledge
  ├─ Inbox
  ├─ Projects
  │    └─ ...
  └─ user-created directories...
```

Projects may receive default physical folders.

Categories should primarily be metadata/properties, not a parallel mandatory directory hierarchy, because one document can conceptually span multiple categories and users should retain control of organization.

### Directory semantics

Directories are a user-controlled organizational view, not business identity.

Users may:

- create directories
- rename directories
- move documents
- create arbitrary personal structures

AIKS must not treat these actions as data corruption.

Project/category/tag/source metadata remains authoritative through document properties/read models rather than being inferred solely from paths.

### New knowledge placement priority

Suggested default behavior:

1. explicit user-selected directory
2. configured default directory for the current project
3. project folder under `Knowledge/Projects/<project>`
4. `Knowledge/Inbox`

## AI Conversation Records

User-facing name: **AI Conversation Records**.

Internal compatibility path may remain `/10 AI Sessions`.

AI Conversation Records are canonical source content and read-only in normal user flows.

They remain:

- browsable
- searchable
- referenceable
- available for backlinks
- navigable from Work Records / provenance

They are not editable in the normal Workbench.

AIKS Work Records remains the management/control view containing operational information such as:

- source/provider
- time
- project
- processing status
- extraction status
- errors

"View original conversation" navigates into Knowledge -> AI Conversation Records and opens the corresponding read-only document without changing the AIKS primary navigation away from Knowledge.

## Knowledge Creation

Support both entry points:

1. AIKS-level `+ New Knowledge`
2. native Workbench/document-tree create action

Both produce canonical SiYuan documents through the same underlying creation path.

The AIKS entry may provide richer context such as current project, default directory, initial category, or provenance.

Documents created directly through the tree are valid first-class knowledge. AIKS may asynchronously attach/mirror required metadata rather than preventing native creation.

## Canonical Data Ownership

Core rule:

> Content belongs to SiYuan. Control state belongs to AIKS.

### SiYuan is Master for

- knowledge body
- knowledge title
- document path/hierarchy
- user-visible tags
- user-editable properties such as project/category
- document/block identity
- AI Conversation Record content

### AIKS is Master for

- source/provider configuration
- session/process control state
- extraction/pipeline status
- source relationships and operational provenance
- generated-hash/update guards
- processing diagnostics
- chunks/embeddings/read indexes
- Agent/RAG hybrid-search projections
- runtime/workbench state where appropriate

### SQLite knowledge fields

Legacy/cache fields such as `knowledge_item.content/title/tags` become read-model/cache only.

No normal V4.2 business path may use those cached values as an authoritative source to overwrite a SiYuan canonical document.

When SiYuan changes:

```text
SiYuan documentChanged
  -> AIKS reads canonical markdown/properties
  -> refresh SQLite read model
  -> invalidate stale chunks/embeddings
  -> asynchronously rebuild derived indexes
```

Never implement periodic/cache-driven overwrite from SQLite back to SiYuan.

## Document Metadata Model

Use three classes of metadata:

1. AIKS system metadata: hidden/read-only to normal users
2. business metadata: initialized by AIKS but user-editable
3. user custom metadata: entirely user-owned

### System metadata

Recommended conceptual fields:

```text
aiks-id
aiks-kind                 knowledge | session
aiks-source-session-id
aiks-source-type
aiks-origin               ai | manual
aiks-user-modified        true | false
aiks-generated-hash
```

System fields are hidden or read-only in normal property UI.

Avoid using ambiguous `managed-by=ai|user` as the primary ownership concept in V4.2. The document always belongs to the user; origin and modification state are separate concepts.

### Business/user-editable metadata

```text
project
category
tags
status
```

User changes to these values in SiYuan are authoritative and must not be silently reset by later scans/extractions.

### Provenance UI

Source provenance should be displayed in a user-friendly read-only form, such as:

```text
Source: Claude · <date/time>
```

Selecting it opens the corresponding AI Conversation Record.

## Knowledge Lifecycle

Keep the lifecycle intentionally small:

```text
draft
active
archived
conflict
```

`conflict` is an internal state; the user-facing language should prefer wording such as "AI update available" instead of alarming technical terminology.

### AI extraction destination

New AI-generated knowledge initially lands in:

```text
Knowledge / Inbox
```

with initial project/category/tags/source metadata and `draft` status.

The user may:

- edit directly
- move it to another directory/project folder
- mark it active/formal
- leave it in Inbox for later organization

Draft knowledge may still be searchable/referenceable.

V4.2 should not require mandatory approval for every generated item, and should not enable aggressive automatic final filing by default.

## AI Updates to Existing Knowledge

### User has not modified the generated document

If the canonical current content hash still equals the last AI generated hash:

```text
current_hash == aiks-generated-hash
```

AI may update the document automatically and then update the generated hash.

### User has modified the document

If:

```text
current_hash != aiks-generated-hash
```

AI must not overwrite the canonical document.

Instead AIKS creates an update candidate in its control/data layer.

Conceptual candidate fields:

```text
knowledge_id
source_session_id
base_hash
generated_content
generated_at
status
```

The canonical SiYuan document remains unchanged until the user chooses an action.

Recommended user actions:

- Apply update
- Partially merge / review differences
- Ignore

Do not create parallel formal documents such as `-v2`, `-new`, or `-AI` for update proposals.

## Navigation Rules

Content-viewing actions converge on the same Workbench:

```text
Work Records -> View original conversation
  -> Knowledge -> AI Conversation Records -> corresponding read-only document

Processing Center -> View generated knowledge
  -> Knowledge -> corresponding knowledge document

Search -> result
  -> corresponding document/block

Backlink/source reference
  -> corresponding knowledge or AI Conversation Record document
```

The AIKS Knowledge navigation item remains active while navigating between normal knowledge and AI Conversation Records inside the Workbench.

## Styling and Product Identity

The embedded runtime uses AIKS design tokens and interaction language:

- background colors
- borders
- typography
- spacing
- radii
- hover/active states
- light/dark modes
- icons where practical

The customized workbench must not visually present itself as a second standalone application nested inside AIKS.

Standalone SiYuan branding, marketplace/product chrome, account controls, and unrelated toolbar surfaces are absent from the normal workbench.

Licensing/open-source attribution remains present where required. Product-facing branding reduction does not remove license obligations.

## Licensing

SiYuan is AGPL-3.0 licensed. The V4.2 distribution and source-publication model must satisfy the applicable AGPL obligations for the modified/bundled SiYuan runtime.

This is a release requirement, not an optional documentation task.

## Bridge and Security

Retain a versioned AIKS <-> Workbench integration protocol.

The bridge should remain constrained to expected loopback/runtime origins and validate protocol version, action/event names, identifiers, and runtime nonce/session state.

The V4.2 bridge focuses on navigation and integration rather than hiding standalone UI with fragile DOM manipulation.

Expected capabilities include:

- open document
- open/focus block
- navigate to root
- switch document/database/graph context as needed
- set/read read-only session mode
- react to document change
- surface provenance navigation
- expose readiness/version information

## Diagnostics

AIKS diagnostics should expose enough runtime metadata to reproduce problems:

```text
SiYuan/Knowledge Engine ready
Workbench ready
Bridge protocol version
AIKS Workbench version
SiYuan base version
SiYuan upstream commit
current workbench mode
content migration status
index refresh/degradation state
```

## Upgrade and Regression Gates

Every upstream SiYuan refresh must verify at least:

### aiks-siyuan gates

- customized frontend build
- kernel build/tests
- AIKS Embedded Profile behavior
- AIKS theme/layout
- bridge contract
- Knowledge editable
- AI Conversation Records read-only
- search
- backlinks
- outline
- properties
- database
- graph
- document open
- block open/focus

### AIKS gates

- Rust formatting/lints/tests
- frontend tests/build
- Windows desktop compile/package gate
- migration compatibility
- content/read-model refresh
- Workbench E2E

Successful upstream merge alone is not considered a successful runtime upgrade.

## Migration from V4.1

V4.1 is treated as the architecture-validation baseline.

V4.2 should reuse rather than discard the working V4.1 foundations where they remain valid:

- canonical SiYuan content model
- existing document IDs
- `/10 AI Sessions` and `/20 Knowledge` physical compatibility roots
- migration state
- hash/conflict protection
- Workbench controller/bridge concepts
- document change -> read-model refresh path

V4.2 primarily changes the presentation/runtime packaging boundary from "full standalone SiYuan embedded and trimmed" to "AIKS-native customized SiYuan workbench".

## Non-Goals

V4.2 does not aim to:

- rewrite Protyle
- recreate SiYuan search
- recreate Attribute View
- recreate graph
- build a second AIKS knowledge editor
- build a second user-facing AIKS knowledge search page
- remove unused SiYuan kernel modules purely for binary minimalism
- follow every upstream SiYuan release
- create a standalone competing note-taking product

## Acceptance Criteria

V4.2 is successful when:

1. AIKS keeps one consistent primary shell/navigation while Knowledge feels visually native to AIKS.
2. Users do not see unnecessary standalone SiYuan product surfaces such as plugin marketplace, SiYuan AI/Agent/MCP, accounts, subscriptions, cloud sync, or update UI.
3. Knowledge opens directly into the Workbench rather than an intermediate AIKS knowledge list page.
4. User-facing knowledge search is performed inside the Workbench using SiYuan search; the AIKS Search sidebar item is removed.
5. AI Conversation Records are visible in the document tree under that product name and are read-only.
6. Knowledge directories remain under user control; project folders are defaults, not business identity.
7. Category/tags/project are metadata and user edits are authoritative.
8. Database and Graph reuse SiYuan native capabilities as main Workbench modes.
9. Outline/Backlinks/Properties form the compact auxiliary panel.
10. AI-generated knowledge enters Inbox as draft by default.
11. AI may auto-update only content that has not been user-modified; otherwise it creates a reviewable update candidate.
12. SiYuan remains canonical for content, while AIKS remains canonical for control/process state and derived retrieval indexes.
13. Both customized frontend and kernel are built from the same pinned upstream commit and shipped together with AIKS.
14. The fork remains sufficiently thin that upstream stable releases can be periodically integrated with bounded merge cost.
15. AGPL distribution/source obligations are handled as part of release engineering.
