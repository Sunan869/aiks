# AIKS Diagnostics: partial failure and safe reporting contract

The Diagnostics page probes local Doctor, AI connectivity and embedded SiYuan/Workbench independently. A rejected check does not hide successful results from the other two checks. A completed false AI connectivity result remains a visible negative health result, not a request failure.

## Regression and manual acceptance

- From apps/aiks-desktop, run npm test -- diagnostics-probes.test.ts and npm run build, then the normal Desktop frontend CI.
- Disable only the model endpoint: Doctor and SiYuan/Workbench observations must still render; AI must report unavailable or failed. Restore the endpoint and retest.
- Make only workbench diagnostics unavailable: Doctor and AI observations remain usable. A failed probe must never display a stale green health state from the preceding refresh.
- Simulate a rejected Doctor call while the others finish. Show the failing probe name, never arbitrary raw exception text. Collect all three probes independently.
- Recheck and confirm the timestamp only reports all requests successfully completed when every API call completed. A negative health response means that the check itself ran, not that the service is healthy.
- Verify dark mode, narrow windows, keyboard-only refresh, and the failure notice with a Windows screen reader.

These are code-level acceptance criteria. CI is not a Windows, SiYuan, or real-model service test. This module changes no Provider files, SQLite schema, user-edited SiYuan documents, or backup/restore state.
