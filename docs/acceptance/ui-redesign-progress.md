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
