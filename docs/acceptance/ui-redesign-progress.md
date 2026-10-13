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

## 2026-10-13 shared surfaces batch

- Added UiSection, UiMetric and UiNotice presentation primitives with consistent neutral/semantic tones, focus-independent markup and dark-mode styles.
- Refactored Overview summary metrics/status section, Sync status panels/notices, Sources errors, Diagnostics errors and Processing refresh errors to use common surfaces. Log empty states are explicitly announced.
- This creates reusable UI components without adding unverified dependencies. It does **not** claim the official shadcn/ui + Radix migration is complete. Windows real-device visual and interaction acceptance is still required.

## 2026-10-13 frontend contract alignment

- CI #38380027772 failed two stale test assertions after UiNotice encapsulated accessible alert roles. The contracts now assert that Sync and Diagnostics render danger notices and that UiNotice itself maps danger to alert, retaining accessibility without duplicating markup in every page.
- No backend or native behavior changes. Frontend compile/tests and Windows testing still need independent confirmation.

## 2026-10-13 nested popup Escape interoperability

- Fixed the document capture-phase Escape handler in useModalFocus. When Escape comes from an open in-dialog SearchSelect or CalendarDateField popover, the parent modal now leaves dismissal to the nested control, preventing an accidental close of KnowledgeEditor, UnifiedSearch or Ask AIKS.
- Tagged nested popover surfaces, retained selected-date/list focus restoration and existing save-in-progress close gates. Added regression source contracts for the keyboard ownership boundary.
- This is not an official Radix/shadcn migration. A real Windows focused keyboard walkthrough is still needed to accept the behavior.

## 2026-10-13 modal stack and input lifecycle batch

- Modal focus containment now applies only to the topmost open modal, guarding Escape/Tab from competing document capture listeners and restoring focus only for a modal that was topmost on teardown.
- SearchSelect clears stale filter text when dismissed by outside click; Escape ownership remains within nested popovers.
- CalendarDateField responds to an externally changed controlled date by displaying that selected date's month on reopening, preserving keyboard/date selection semantics.
- Native/UI tests pending; this is an overlay interaction correctness pass, not official shadcn/Radix integration.

## 2026-10-13 live log center module

- Logs now classify errors/warnings from event text, retain the latest 500 log entries, filter by severity/text and copy the visible selection for support diagnostics.
- A pause control stops appending events without accumulating a hidden backlog and reports how many updates were omitted while paused. Resume handles new events; clear resets displayed history and omitted counter.
- The event subscription guards an unmount occurring before async listen resolves, preventing a leaked listener. This does not persist or upload logs or alter Rust providers.
- Pure log formatting, severity, capacity and filtering have executable Vitest coverage. Windows verification remains required; this is not the official shadcn/Radix migration.

## 2026-10-13 processing-center refresh operations

- Processing Center now allows pausing/resuming automatic two-second background status refresh while retaining explicit manual refresh and retry operations. It shows polling state, last successful refresh and consecutive failures.
- Added an unmount guard around asynchronous task polling updates, preserving current in-flight gating and avoiding state updates after the workspace leaves the screen. Backfill actions reject duplicate starts.
- No status data model, provider, Rust pipeline, task mutation or SiYuan overwrite behavior changed. Frontend CI and Windows functional walkthrough remain pending.

## 2026-10-13 consolidated component and acceptance gate

- Added workspace-level StatusBadge/EmptyState/StatGrid primitives and reused them in Overview, Sync, Diagnostics, Logs and Sources.
- Added source/contract CI gate and an explicit Windows keyboard, themes, nested overlay, provider-read-only and native-validation acceptance checklist at docs/acceptance/windows-ui-final-acceptance.md.
- **Important:** official shadcn/Radix migration remains incomplete because no dependency installation/lockfile verification is possible in this execution environment. Existing CI is not full Rust+Windows desktop compile. Do not call this work 100% complete.

## 2026-10-13 shared grid source contracts repair

- CI #38382553780 had 2 test failures, both caused by deprecated assertions for inline responsive CSS classes after pages migrated to UiStatGrid. Test expectations now validate the page-to-component linkage and the shared component's responsive CSS mapping.
- Extended the cross-screen acceptance gate to verify four/six-column breakpoint contracts in the single source of truth. No Rust, database, Provider or SiYuan behavior changes.
- Official shadcn/ui+Radix installation and hands-on Windows UI acceptance remain incomplete.

## 2026-10-13 keyboard-select and dialog acceptance batch

- SearchSelect now starts keyboard navigation at the current selected option, uses one open/close path for mouse and keyboard, and exposes list-autocomplete semantics.
- Calendar exposes the current day separately from the selected date using aria-current. Dialog focus traversal excludes inert/aria-hidden descendants and ignores OS-modified Tab keys.
- Strengthened source-contract regressions. This is an accessibility hardening batch, not verified official shadcn/ui or Radix installation; native Windows testing remains open.

## 2026-10-13 shared modal overlay migration

- Knowledge Editor, Unified Search and Ask AIKS now use a shared ModalBackdrop component, centralizing outside-click ownership and preserving individually tuned editor/search/drawer geometry.
- Backdrop dismissal only executes for actual clicks on backdrop itself (not dialog descendants); saving in the knowledge editor disables backdrop dismissal. Existing useModalFocus keyboard trapping and business actions are unchanged.
- Added cross-component contracts. Official shadcn/Radix installation remains outstanding; it requires a verified npm lockfile and Windows visual testing.

## 2026-10-13 Windows screenshot corrections

- Full-width page shell/project memory removes excess outer margins; sidebar removes redundant AI logo tile.
- Source cards use equal-height flex layout with footer controls anchored below variable content.
- Shared buttons gain consistent border/contrast, search popover input avoids global double-focus styling.
- Native select replacement and official shadcn/ui+Radix installation are still outstanding; this batch must pass CI and be validated on Windows before claiming resolution.

## 2026-10-13 visual acceptance build regression batch

- Fixed Vitest's CSS source inspection by loading index.css from node:fs, rather than relying on Vite's CSS ?raw transform which returned an empty module in CI.
- Fixed shared button styling being unexpectedly purged by Tailwind 3: variant and size class names now exist as literal strings, with component-class safelist coverage as a further guard.
- Source grid uses items-stretch and dedicated aiks-source-grid hook so cards align by grid row. Preserves variable-length descriptions and edit panels.
- Official Radix/shadcn integration and replacing native select dropdown menus remain separate tasks, not declared completed by this styling repair.
\n## Radix dependency-backed select migration\n\n- Added RadixSelectField using @radix-ui/react-select for fixed option lists, including scroll controls, collision padding and empty/all-option mapping.\n- Migrated Ask AIKS project/source scope and Project Memory Agent output format from native menus; resetting conversation history and invalidating Agent previews remains mandatory.\n- Full native UiSelect migration and other Radix integrations remain outstanding; verify Windows dropdown positioning and keyboard access.\n
## 2026-10-13 first Radix integration CI repairs

- Updated two old Ask AIKS tests to validate the actual Radix select label/value semantics rather than native UiSelect markup.
- Scoped Gitleaks false-positive handling to an exact benchmark documentation line naming an optional environment variable. No credentials are stored; genuine findings remain blocked.
- Radix Select migration is still in progress and must pass CI before expansion.

## 2026-10-13 Radix Select migration of all legacy UiSelect call sites

- UiSelect now delegates to actual @radix-ui/react-select through RadixSelectField, automatically mapping existing JSX option items, including numeric values, disabled options and empty/all-selection values.
- Preserves existing ChangeEvent-like onChange handlers and controlled/uncontrolled behavior for business pages; Radix renders a portal-positioned menu instead of the platform-native dropdown.
- The compatibility adapter is transitional and should be replaced with explicit onValueChange call sites when possible. SearchSelect and CalendarDateField still require their own Radix/Calendar migration. Complete Windows keyboard and visual verification remains required.
\n## Radix Popover migration (searchable project selection)\n\n- SearchSelect uses Radix Popover for portal positioning, collision avoidance, outside-click handling and trigger focus restoration. Search and listbox keyboard logic remains in the component, preserving the selected project identity.\n- Native document-level pointerdown listener removed. This is a stage of the full component migration, not completion of Calendar/Dialog yet.\n\n## 2026-10-13 CalendarDateField Radix Popover migration\n\n- Replaced custom calendar popup positioning/outside-click document listener with @radix-ui/react-popover, keeping month navigation, ISO date selection, day keyboard movement, today shortcut, controlled values and focus restoration.\n- Retained in-house calendar day grid until a separately verified react-day-picker v10 adaptation; this migration uses genuine Radix positioning/focus controls but does not yet replace the date grid with shadcn Calendar.\n
## 2026-10-13 coordinated Radix Switch/Tabs batch

- Settings switches now use @radix-ui/react-switch with controlled checked state.
- Knowledge editor Markdown/Preview controls now use Radix Tabs, while preserving preview state and existing content view rendering.
- Project Memory workspace navigation now uses Radix Tabs triggers with its existing tab state and lazy/hidden business sections retained.
- Windows focus, density, and screen reader checks remain necessary. Dialog and react-day-picker still need dedicated migration.

## Radix Switch/Tabs follow-up and composable Tailwind controls

- Updated legacy source-contract tests to assert Radix Switch checked state and accessible Tabs triggers instead of obsolete hand-crafted switch/pressed-button markup.
- Shared UiButton now uses class-variance-authority with clsx and tailwind-merge; variants and sizes remain stable, with explicit Tailwind utilities rather than relying exclusively on CSS @apply.
- Shared Input and Textarea now compose Tailwind tokens via cn() while retaining legacy classes for compatibility until legacy CSS is removed.
- Dialog, Calendar grid, and CSS reduction remain outstanding.
\n## react-day-picker migration\n\n- Replaced the custom calendar day grid and keyboard date-navigation engine with react-day-picker v10 DayPicker (controlled month/selection, Sunday/month handling via the library).\n- Existing Radix Popover still handles overlay positioning, Escape, outside dismissal and focus restoration. ISO date output, today shortcut and disabled state are retained.\n- Replaced brittle source-code assertions that required the removed handwritten grid with assertions for DayPicker integration.\n\n## 2026-10-13 cmdk search picker migration\n\n- Replaced the handwritten searchable list/active-index keyboard navigation with cmdk Command, including Command.Input, List, Item and Empty, hosted in Radix Popover.\n- Preserved controlled option values, project identity, trigger focus restore, outside dismissal and Escape isolation.\n- Updated keyboard, modal, narrow-screen and interaction contract tests to verify actual library primitives, not the removed handwritten event handlers.\n