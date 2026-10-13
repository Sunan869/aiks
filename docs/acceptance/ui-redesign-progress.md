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
