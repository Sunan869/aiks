# AIKS Desktop Internationalization (Work in progress)

Desktop locale implementation: `react-i18next` + `i18next`.

## Locales

- `zh-CN`: Simplified Chinese, fallback for missing keys and unsupported system locales.
- `en-US`: English.
- The system language is used only if the user has not selected a language.
- **Settings → Language** changes the interface immediately. Selection persists in WebView localStorage under `aiks.ui.language`.

## What localization is allowed to change

**Only UI presentation.** Original provider sessions, knowledge data, source names/IDs, diagnostic logs, paths, SQL state, settings configuration, LLM prompts, model responses and historical data are not translated or mutated. Provider file access remains read-only. SiYuan user-edited documents remain protected.

A temporary Vite TSX migration plugin in `plugins/localize.ts` converts authored visible JSX text and presentation labels to translation lookups. Complex dynamic notices use `translateUiText`, which only recognizes explicit UI-authored message patterns. `src/locales/en-US.json` contains curated translations, while `zh-CN.json` preserves the original UI copy. Do not auto-translate user content.

## Adding a new UI string

1. Create an appropriate English entry in `src/locales/en-US.json`; add the matching Chinese source key in `src/locales/zh-CN.json`.
2. Prefer explicit `useTranslation()` / `t(key, vars)` for new code. The migration bridge only exists to support older hardcoded TSX screens.
3. Use interpolation variables for dynamic UI numbers, never string concatenation that changes persisted data.
4. For feedback that is stored in React state, localize **at display time** so switching languages updates visible messages.
5. Keep warnings about irreversible actions, document ownership, conflicting edits, review provenance and original-source protection complete in both languages.

## Validation

From `apps/aiks-desktop`:

```bash
npm ci
npm test
npm run build
```

Manual desktop acceptance:

- Confirm Simplified Chinese and English in Settings. Change language without restarting; navigate through every workspace.
- Restart AIKS and verify that the user-selected locale persists; switch to system-based English on a fresh profile.
- Check all menus, dialogs, buttons, tooltips, tab labels, date pickers, pagination, empty/error states and status toasts in both languages.
- Trigger a failed sync/connection test, permission warning, void preflight, knowledge review flow, AI question and model failure.
- Verify that imported Chinese sessions, knowledge content, raw logs, provider identities and model prompts remain **unchanged** when language is switched.
- Confirm both light/dark mode and narrow-window layouts; watch for overflowing long English labels.

Until complete manual verification and a CI pass, **do not claim every native prompt or backend diagnostic message is localized**. Do not merge the PR or release a build solely because a translation file exists.
