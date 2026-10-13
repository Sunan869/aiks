# Reference Analysis: SiYuan

## Repository

- Name: siyuan
- URL: https://github.com/siyuan-note/siyuan
- Commit SHA: N/A (used via HTTP API only)
- Checked Date: 2026-09-11
- License: AGPL-3.0

## Why This Repository Matters

SiYuan is the V1 Knowledge Sink. AIKS integrates via HTTP API. No SiYuan source code is directly used or copied.

## Session/Data Location

SiYuan HTTP API at `http://127.0.0.1:6806` (default).

## Relevant API Endpoints

| Endpoint | Purpose |
|---|---|
| `POST /api/system/getConf` | Health check |
| `POST /api/notebook/lsNotebooks` | List notebooks |
| `POST /api/notebook/createNotebook` | Create notebook |
| `POST /api/filetree/createDocWithMd` | Create document with Markdown |
| `POST /api/block/updateBlock` | Update document content |
| `POST /api/block/setBlockAttrs` | Set custom attributes |
| `POST /api/block/getBlockAttrs` | Get custom attributes |
| `POST /api/search/searchAttr` | Search by custom attribute |

## AIKS-Managed Custom Attributes

```
custom-aiks-managed=true
custom-aiks-source={source}
custom-aiks-session-id={session-id}
custom-aiks-content-hash={sha256}
custom-aiks-parser-version={version}
custom-aiks-synced-at={iso8601}
```

## AIKS Decisions

- HTTP API only — no SiYuan source code copied.
- AGPL-3.0 license: no attribution needed since we don't copy/modify SiYuan code.
- Authentication via `Token` header or `SIYUAN_TOKEN` env variable.
- Document path format: `/10 AI Sessions/{Source}/{YYYY}/{MM}/{date} {title} [{short-id}]`
