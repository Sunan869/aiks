# AIKS UI redesign — 2026-10-13 batch status

Base: 91fdef1 (Windows Provider green, frontend CI blocked by outdated date calendar string contract).

This batch delivers new functional filtering in Sources and Processing, fixed calendar-contract drift, and stable per-instance combobox popup IDs. The existing read-only provider API, manual processing actions, knowledge extraction backend, database schema and SiYuan documents are unchanged.

**Not yet delivered:** formal shadcn/ui + Radix installation. The execution environment could not resolve the npm registry, so no unverifiable package-lock changes or false migration claims were made. Existing React/Tailwind shared components remain in use.

Acceptance after CI: verify source filtering with warnings and empty results, pipeline search, project selection, calendar date changes, keyboard focus and both light/dark themes. CI does not replace Windows visual testing.

## 2026-10-13 follow-up: filter consistency and keyboard accessibility

- Processing search now filters both recent task rows and pipeline runs, using explicit visible fields rather than serializing entire records; task state filters combine with search and report visible/loaded counts. Aggregate statistics remain full-database counts.
- Sources search also matches provider keys and configured paths without changing provider read-only behavior.
- SearchSelect moves focus to the search input when opened and exposes an active option relationship, stable option IDs, and a listbox association to assistive technology. No new dependency or shadcn/ui installation is claimed.
- Windows visual acceptance, automated CI, and the actual Radix/shadcn migration remain separate validation items.

## 2026-10-13 foundation consolidation

- Introduced common native-backed input, textarea, and five-variant button primitives in `components/ui/fields.tsx`, with shared Tailwind component tokens and keyboard focus/disabled styling.
- Adopted the primitives in source-provider actions and search, and processing-page primary actions and search. Existing onClick handlers and provider read-only safeguards remain unchanged.
- npm Registry could not be reached from the execution environment; **Radix and official shadcn/ui packages are not installed**, and this commit must not be counted as a completed official migration.
- CI, desktop build and actual Windows visual acceptance remain required after commit.

## 2026-10-13 shared-control adoption, six-page batch

- Reused the accessible button primitive across Overview, Sync, Logs, Diagnostics, Sessions and Search pages, preserving all previous click handlers, disabled logic, and domain operations.
- Reused the input primitive in Sessions and Search while retaining existing source selection, pagination, and search logic.
- This is **shared-component consolidation**, not the official shadcn/Radix migration. A real npm install, locked dependencies, keyboard/visual checks and CI remain prerequisites for calling that migration complete.
- No data model, provider write permissions, SiYuan overwrite behavior, or backup entry points were modified.

## 2026-10-13 expanded action-control consolidation

- One batch moves remaining native action buttons in 11 pages to the existing UiButton primitive: knowledge base, knowledge detail, knowledge list, knowledge workbench, knowledge workspace, processing detail, processing center, project memory, session detail, settings and sources.
- Preserves original click handlers, disabled and visibility conditions, action labels and domain calls. Adds source-level contract coverage for native-button regressions.
- Official shadcn/ui and Radix dependency installation remains **not done**; this is an interim shared-controls refactor. Requires CI and Windows visual validation before acceptance.

## 2026-10-13 form control consolidation batch

- Unifies native text/number inputs across Settings, Project Memory, Knowledge Detail and Knowledge Base under the shared UiInput component, while retaining native checkboxes for consent and selection.
- Consolidates evidence and editable report/organization draft textarea presentation under UiTextarea. Existing model configuration, token constraints, source evidence, confirmation gates and write APIs remain unchanged.
- Adds cross-page guard tests for critical labels and consent boundaries. This is an intermediate shared-form migration; official Radix/shadcn installation and real Windows UI acceptance are outstanding.

## 2026-10-13 calendar and project picker keyboard improvements

- Calendar dialog places keyboard focus on the selected or first available day; supports Home/End and arrow keys while retaining ISO date output, Today shortcut, outside-click dismissal, disabled state and Escape-to-trigger behavior.
- Search picker opens with keyboard arrows from the trigger, cleans search state when Escape closes, and announces zero-match results.
- Retains dependency-free controls until official Radix/shadcn installation is testable. Windows visual focus and screen-reader behavior are not yet verified.

## 2026-10-13 sources and processing information architecture batch

- Data Sources now shows full-catalog health, needs-attention, and disabled counts before filters; counts intentionally do not change with search and never infer health from scan volume.
- Processing now gives task and pipeline tables independent, keyboard-focusable scrolling regions with sticky task column headers, explicit displayed-versus-loaded counts and responsive minimum table widths.
- No provider writes, task-state mutations, schema changes or backup entry points; real Windows sizing, screen reader experience and official shadcn/Radix migration remain outstanding.

## 2026-10-13 meaningful usability follow-up (one batch)

- Processing stage disclosure is a genuine keyboard-operable button with `aria-expanded`, instead of a pointer-only clickable div.
- Knowledge workbench cards expose a dedicated title action instead of making the article itself a pointer-only click target, preserving separate favorite actions and their event isolation.
- Knowledge grid uses accessible focus indicators and responsive columns; sidebar navigation supports dense window widths and bounded scrolling without changing navigation or provider permissions.
- No official Radix/shadcn installation is claimed. Complete Windows visual, keyboard and screen-reader acceptance remains outstanding.

## 2026-10-13 modal usability batch

- KnowledgeEditor now uses responsive narrow-window layout, shared form/button primitives, a labelled modal dialog, autofocus, keyboard Escape, disabled close during save, and announced validation errors. Business save/update APIs and manual-edit protection remain unchanged.
- UnifiedSearchDialog now has narrow-window spacing, explicit labelled modal semantics, an accessible search field and announced errors while retaining progressive retrieval and cancellation.
- CI and Windows visual acceptance are still required; no claim of official Radix/shadcn installation.

## 2026-10-13 AI assistant and storage setup control batch

- Ask AIKS panel now reuses shared button and selection controls while preserving progressive RAG streaming, source/project filters, result actions, history, and disabled conditions.
- First-run storage setup and storage management now reuse shared buttons; first-run setup has a labelled dialog and a bounded scroll region on small screens, without changing storage migration or restart operations.
- A lightweight cross-component contract test verifies nonregression of core AI streaming and data-location calls. Radix/shadcn and Windows UI acceptance remain outstanding.

## 2026-10-13 overlay keyboard safety and Windows acceptance gate

- Introduced one reusable modal focus boundary, used by KnowledgeEditor, UnifiedSearchDialog and Ask AIKS. It traps Tab/Shift+Tab, closes with Escape where safe, and returns focus to the previous control after closing.
- KnowledgeEditor blocks Escape during saving. Existing progressive search abort and knowledge write logic remain unchanged.
- Windows acceptance: test every overlay with keyboard only, narrow windows, and dark mode; CI source-contract checks are not a substitute for visual interaction testing.

## 2026-10-13 UI fast CI / final native validation strategy

- `CI` workflow dispatch now defaults to fast frontend verification: npm ci, full Vitest suite, tsc+Vite build, and repository hygiene/secret scanning. Rust quality and Windows desktop compilation are skipped **only for dispatch runs without `full_validation`**. Main/review branch push and pull requests retain full jobs.
- Before Windows real-machine acceptance, manually dispatch `CI` with `full_validation=true`; this executes Rust fmt/Clippy/tests, Windows compile, frontend build/tests, and hygiene. Also dispatch `Windows provider contracts` with `full_validation=true` for native provider tests.
- Native/Rust/Tauri/Cargo changes must choose full validation immediately rather than relying on fast UI validation. Windows provider workflow remains fully enabled on its configured native-code push/PR paths; only manual dispatch defaults to skip.
- Both workflow dispatch modes are intended CI configurations, not evidence that any Windows real-device tests have been performed.

## 2026-10-13 settings and knowledge workspace acceptance polish

- Settings now uses the shared desktop page shell, labels model/endpoint/dimension fields with explicit `htmlFor` references, and keeps save/restart actions visible in a sticky footer while scrolling.
- Switches expose accessible names and retain existing save and restart semantics. Knowledge workbench gives a clearer responsive header, announces result counts and shows pagination totals.
- This is a code-level usability pass, not Windows visual acceptance or an official shadcn/Radix migration. Retain existing offline, provider read-only and SiYuan user-edit boundaries.

## 2026-10-13 consolidated UI readiness pass

- Settings switch retains compact dimensions even with shared button primitives; primary Save uses the primary action variant.
- KnowledgeEditor preview toggle exposes selection state via `aria-pressed`, and the footer wraps safely on narrow windows.
- Embedded workbench retry adopts shared action visuals and error alert semantics.
- Offline Restore (already present on this branch) adopts shared fields and buttons without changing backup/restore APIs or confirmation gates. **No new backup functionality is introduced by this UI pass.**
- Remaining blocking acceptance work: official shadcn/Radix installation is not completed, Windows real-device interaction/visual QA, and a final Full Validation run. These are not implied by green frontend CI.

## 2026-10-13 batch — narrow-window and picker safeguards

- SearchSelect now bounds popup width to the available viewport, clamps the active index when filtering/options change and constrains menu height in short windows.
- CalendarDateField now permits narrower triggers and popovers and clearly highlights focused date buttons.
- Standardized minimum-width behavior across knowledge, processing, project memory and sidebar layout foundations so long content does not force unconstrained horizontal expansion.
- This batch changes only UI presentation and keyboard behavior. Official shadcn/Radix dependencies remain uninstalled and Windows visual/device acceptance remains pending. Neither CI success nor file-count coverage is a substitute for those acceptance steps.

## 2026-10-13 CI blocker and dropdown keyboard consolidation

- Frontend build failure #38375645104: SearchSelect referenced `matching` before declaration (TS2448/TS2454). The filtered options are now declared before the effect that uses them, and zero-result filtering clamps keyboard state to a safe index.
- Escape inside project selector and calendar now stops propagation so enclosing dialogs do not close unexpectedly. Calendar month headings announce month changes.
- Added regression source contracts for declaration order and nested popup Escape. No native business logic, provider access or backup behavior altered.
- Fast frontend build and Windows real-device keyboard test remain the validation gate; this record does not claim validation already passed.

## 2026-10-13 knowledge workspace functional consolidation

- Native Knowledge Workbench now supports clearly scoped **current-page** search across title, summary, project, category and tags, without pretending it searches the entire database. Search combines with server-backed status/category filters, and empty-query outcomes have a dedicated reset action.
- Knowledge tabs expose selected states; category and refresh controls have accessible names. Counts explicitly distinguish visible/current-page items from database totals. Existing favorite, manual create, archive and pagination actions stay intact.
- Knowledge Base and Sources headers wrap in narrow desktop windows. This batch does not change SiYuan documents, Provider permissions, data schema or backup behavior.
- Formal shadcn/Radix dependency migration and Windows visual acceptance remain outstanding. CI pass and real-device testing are separate gates.

## 2026-10-13 list accessibility batch

- Session and Knowledge list pages now explicitly distinguish visible matches, loaded page size and database total in filtering results. Filter-empty guidance no longer incorrectly tells users to scan/extract when there is merely no current-page match.
- Session rows no longer rely on click-only table row navigation. The session title is a keyboard-operable detail button. The table has an independently keyboard-focusable scroll region and a minimum width in narrow windows.
- Knowledge cards expose a real, labelled keyboard-operable detail button while preserving the existing card display; no new backend writes or provider changes.
- This is code-level accessibility work only. Formal shadcn/Radix migration and Windows visual acceptance remain outstanding.

## 2026-10-13 page inventory and CI repair

- Inventory: 18 TSX page modules. 17 already had the shared desktop shell or workspace layout; legacy KnowledgePage was the remaining page without it. It now has responsive header/cards and a keyboard-operated knowledge action. This is shell coverage, not 18 completed shadcn/ui migrations.
- Fixed frontend CI #38377024085: the source-contract test had assumed there was no native button on KnowledgeBasePageV3. That button is an intentional accessible knowledge-card action, so the contract now accepts it while continuing to guard the other reviewed pages.
- Full shadcn/Radix integration, visual parity and Windows real-device acceptance remain outstanding. Do not mark those completed from static TSX coverage alone.

## 2026-10-13 cohesive action hierarchy pass

- Across Knowledge Workbench, Overview, Settings, Knowledge Editor and the legacy Knowledge view, primary actions use the primary variant and inactive navigation/refresh actions use ghost/secondary variants. Avoid conflicting old background utility overrides for the main knowledge action.
- Knowledge view tabs communicate selection using both aria-pressed and the shared button variant. Markdown/Preview uses the same visual selection hierarchy without changing editor state.
- This is an intermediate Tailwind component consolidation, **not** official shadcn/ui installation, and still requires CI and Windows visual acceptance.

## 2026-10-13 monitoring and search workflow batch

- Overview and Sync now use coherent status hierarchy, safe in-flight guards and responsive metric layouts; sync success/errors announce their result.
- Diagnostics reports failures to the UI and shows last successful check time instead of silently logging errors.
- Logs now support severity + text filtering, displayed/loaded counts, accessible scroll region and an explicit no-match state; no logging backend changes.
- Knowledge Search discards stale async responses when the query changes and uses keyboard-operable result detail actions; preserves the existing search API.
- Frontend CI and native Windows acceptance are separate from this code submission. Official shadcn/Radix installation remains outstanding.

## 2026-10-13 consolidated desktop header architecture

- A reusable typed PageHeader now drives eight core workspaces (Overview, Sync, Diagnostics, Logs, Sources, Sessions, Knowledge Base V3 and Search). Header title, description, responsive action layout and light/dark text hierarchy are standardized in one implementation instead of duplicated per page.
- Existing handlers for sync, provider refresh, import, diagnostics, clearing logs and knowledge publishing are retained. This is a structural frontend component consolidation, not a declaration of official shadcn/Radix migration.
- Windows visual acceptance and full native validation remain separate steps. No Rust, schema or provider changes.

## 2026-10-13 unified major-workspace headers

- Added Processing Center, Native Knowledge Workbench, Project Memory and Settings to PageHeader, bringing the shared title/action component to 12 primary pages.
- Retains prior backend API operations, backfill/refresh actions, knowledge creation, report tabs and settings controls. Project Memory retains its specialized hero metrics below the shared header.
- This remains React/Tailwind consolidation, not official Radix/shadcn installation. Windows visual and keyboard QA are still outstanding.
