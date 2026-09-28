# AIKS → WeKnora adapter

This branch replaces the old assumption that the AIKS team service must own
document ACL, sharing, RAG and a SiYuan content container.

## Responsibility split

AIKS owns provider discovery, local Session collection, normalization,
secret-redacted Session → Markdown rendering, the remote collector protocol and
durable delivery/retry state.

WeKnora owns users, DingTalk login, workspaces, RBAC, sharing, knowledge
lifecycle, indexing, retrieval/RAG, Wiki/history and audit.

## Team topology

```text
employee AIKS Desktop
  -> provider scan
  -> private Desktop outbox
  -> HTTPS aiks-service collector
       -> validates employee WeKnora API key + private KB
       -> assigns workspace-specific collector namespace
       -> durable server outbox
       -> server-only WeKnora platform API key + X-Tenant-ID
  -> employee private WeKnora workspace / AIKS KB
  -> optional WeKnora user / organization sharing
```

A company-wide destination KB is not the team default. Every employee must use
a private WeKnora workspace/KB (or another workspace dedicated exclusively to
that employee) before sharing is applied.

## Server configuration

Collector mode requires dynamic target routing:

```toml
mode = "collector"

[collector]
token_env = "AIKS_COLLECTOR_TOKEN"

[weknora]
enabled = true
base_url = "https://weknora.example.com"
knowledge_base_id = ""
api_key_env = "AIKS_WEKNORA_API_KEY"
channel = "aiks"
dynamic_targets = true
```

`AIKS_WEKNORA_API_KEY` is a server-side WeKnora platform API key with
cross-workspace ingest authority. It never leaves the collector host.

## Desktop configuration

```toml
[backend]
mode = "service_remote"
collector_url = "https://aiks-collector.example.com"
collector_token_env = "AIKS_COLLECTOR_TOKEN"

[weknora]
enabled = true
knowledge_base_id = "<this employee's private AIKS KB>"
api_key_env = "AIKS_WEKNORA_USER_API_KEY"
```

The employee key is used only for live identity/KB validation. The collector
removes the credential header before business handlers execute and never stores
that key in its SQLite snapshot database or durable WeKnora outbox.

## Identity handshake and namespace isolation

1. Desktop reads the collector `instance_id` from `GET /healthz`.
2. Desktop calls `GET /api/v1/collector/bootstrap` with collector auth,
   its WeKnora API key and private KB ID.
3. Collector verifies the key against WeKnora `/auth/me`.
4. Collector verifies that the target KB belongs to that active workspace.
5. Collector returns a workspace-specific AIKS `space_id`.
6. Source registration, snapshots, sessions, receipts and jobs are scoped by
   the verified `principal_id + space_id`.
7. The durable WeKnora outbox persists only the validated tenant/KB target.
   Retries use the server platform key with `X-Tenant-ID`.

Two workspaces may therefore upload the same provider, registration key,
submission ID and upstream Session ID without sharing a collector Session row.

## Delivery semantics

Accepted AIKS snapshots are committed before WeKnora delivery. Each target has a
durable outbox. The worker creates a manual knowledge item on first delivery,
updates it for later revisions, skips unchanged rendered content, and retries
transport/rate-limit/conflict/5xx failures with bounded backoff.

Each WeKnora write carries a stable opaque
`external_id = aiks-<sha256(source + session-id)>`. The AIKS WeKnora fork
makes create/replay idempotent inside the destination KB, covering the failure
window where WeKnora committed a POST but the HTTP response was lost.

## Team sharing boundary

The WeKnora fork now provides DingTalk login and direct user KB sharing.
Organization sharing remains available upstream. Sharing happens after private
placement; aiks-service does not recreate WeKnora ACLs.

DingTalk department-to-organization membership synchronization now lives in
the WeKnora fork. aiks-service does not own DingTalk directory state or ACLs.

## Collector API boundary

Collector mode is intentionally smaller than the personal loopback service.
It exposes collection/session transport, receipts/jobs and WeKnora delivery
status. It does **not** expose AIKS knowledge browsing, search or AI assist:
those team-facing capabilities belong to WeKnora. Personal/offline mode keeps
the existing local knowledge/search/assist APIs unchanged.

## Legacy team runtime retirement

The old aiks-service `mode = "team"` runtime is no longer a supported startup
mode on this branch. Server-side DingTalk authentication, directory sync,
team ACL/share endpoints and SiYuan team content are superseded by WeKnora.

The legacy source modules remain temporarily in-tree only to keep the retirement
reviewable and to avoid mixing a large physical deletion with the runtime
cut-over. They are not part of the supported deployment path and can be removed
after the consolidated integration gate passes.
