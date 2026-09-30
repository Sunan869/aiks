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
allow_insecure_http = false

[weknora]
enabled = true
base_url = "https://weknora.example.com"
# Normal flow leaves these empty: browser login bootstraps them automatically.
knowledge_base_id = ""
api_key_env = ""
```

In the normal Desktop flow, clicking "登录 WeKnora 并自动初始化" starts a
verifier-bound browser handoff. After WeKnora/DingTalk login, the server
creates or reuses the private `AIKS Sessions` knowledge base, creates or
repairs a `retrieve`-only KB-scoped API key, and returns the credential only
to the native Desktop exchange. Desktop then bootstraps the collector with the
returned KB ID/API key; the WebView never receives the key.

`knowledge_base_id` + `api_key_env` are retained only as a legacy/non-
interactive fallback. The employee key is used only for live identity/KB
validation. The collector removes the credential header before business
handlers execute and never stores that key in its SQLite snapshot database or
durable WeKnora outbox.

Remote HTTP is rejected by default. `backend.allow_insecure_http=true` may be
set explicitly for integration testing against a public/LAN HTTP endpoint;
production should keep the default `false` and use HTTPS.

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

## Desktop team entry

The old native `team_*` business client is no longer initialized by
Desktop. The "团队空间" tab owns only the verifier-bound WeKnora browser
pairing plus workbench/collector status. In `service_remote` mode, AIKS keeps
Session browsing, source collection and delivery tasks; knowledge search, RAG,
sharing and department access are opened in WeKnora.

The obsolete native team-client source remains temporarily in-tree but is not
registered with Tauri and is not part of the supported runtime.


## S3: delivery reconciliation and failure visibility

Collector acceptance, WeKnora delivery, and WeKnora parsing/indexing are
separate stages. The Desktop local upload record says **Collector accepted**
only after the delivery intent was durably enqueued; if enqueue fails after the
idempotent snapshot commit, Collector returns a retryable error, keeping the
Desktop outbox pending until the same submission can repair the intent.

In `service_remote` mode, the Desktop task page displays the latest 100 local
upload records with upstream Session IDs, plus the current authenticated KB's
Collector-to-WeKnora summary:

- `delivered`: knowledge mappings for which WeKnora returned a knowledge ID
  (does not prove the knowledge has finished parsing or embedding).
- `pending`: queued delivery or automatic retry after a transient error.
- `terminal`: Collector-to-WeKnora failures requiring intervention, with
  up to 20 recent sanitized error codes and Session IDs.
- Unavailable or old-version status displays **unknown**, not fabricated zeros.

The user can explicitly requeue up to ten terminal delivery failures at once;
the endpoint verifies the employee's WeKnora identity and affects only their
target tenant and knowledge base. It does **not** retry WeKnora's internal
parsing/Embedding failures; use WeKnora's knowledge UI for those.

Deploy the new collector before updating Desktop. No WeKnora fork change is
required for these delivery-status improvements.


### Per-Session reconciliation and observability

The collector status response includes up to 100 recently changed Session
deliveries (source + original external Session ID, latest revision,
`delivered`/`pending`/`failed`, optional knowledge ID and error code).
The Desktop matches those against its local upload records and labels each
stage separately. A missing or outdated collector response is **unknown**,
not zero; records outside the latest-100 window do not claim delivery.

The standalone collector now installs a tracing subscriber and emits sanitized
accept/intent and delivery lifecycle logs to container stderr. Use:

```sh
docker logs --tail 100 aiks-collector
```

`AIKS_LOG` optionally overrides the default `warn,aiks_service=info` filter.
Do not set a verbose HTTP body logger on the collector or log Desktop user keys.

### Integration gate

The source changes and regression tests have been committed on the feature
branch, but a successful Windows Desktop build, Rust tests and live server
integration are **required before calling the rollout verified**. Upgrade the
collector before Desktop; deploy no new WeKnora fork for this slice.


When a previously paired collector becomes temporarily unreachable, native
upload records remain readable under the last verified target identity.
The Desktop distinguishes an unreachable remote from an account that has never
been paired; it never presents another account's outbox as the current user's.
