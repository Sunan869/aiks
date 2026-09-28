# AIKS + WeKnora

The recommended team topology is now the standalone collector:

```text
AIKS Desktop / provider collectors
        |
        | AIKS snapshot protocol
        | collector token + employee WeKnora API key + private KB id
        v
aiks-service (mode = collector)
        |
        | verified Workspace -> isolated collector namespace
        | durable SQLite outbox
        | server-only WeKnora platform API key
        v
WeKnora
        |
        +-- workspace / RBAC / sharing
        +-- knowledge / Wiki / revision
        +-- embedding / retrieval / RAG
```

The old central S2 DingTalk/ACL/SiYuan stack is not part of this path.
The aiks-service runtime now supports only `personal` and `collector` modes;
legacy `mode = "team"` is rejected. Collector mode also rejects local
AI/embedding model capabilities so team knowledge intelligence remains in WeKnora.

## Server deployment

```bash
cd deploy/weknora
cp .env.example .env
openssl rand -hex 32
# fill .env with the generated collector token, WeKnora URL and platform API key
docker compose up -d --build
curl http://127.0.0.1:28082/healthz
```

The container publishes on host loopback by default. Put Nginx/TongHttpServer or
another HTTPS reverse proxy in front of it for remote Desktop access. Do not
expose the plain HTTP collector port directly to an untrusted network.

The server-side `AIKS_WEKNORA_API_KEY` must be a WeKnora **platform API key**
that can ingest into the validated target workspace. It never leaves the
collector host.

## Client identity and handshake

Each employee first creates a private WeKnora workspace/KB (for example
`AIKS Sessions`) and a WeKnora API key that can read that KB. The Desktop
configuration points `[weknora].knowledge_base_id` at that KB and keeps the
personal key in the environment named by `[weknora].api_key_env`.

The Desktop handshake is:

1. `GET /healthz` -> collector `instance_id`.
2. `GET /api/v1/collector/bootstrap` with the employee WeKnora key + private KB id.
3. Collector calls WeKnora `/auth/me` and the KB detail endpoint.
4. On success it returns a workspace-specific `space_id`.
5. `POST /api/v1/source-registrations` and `POST /api/v1/session-snapshots`
   then run inside that isolated namespace.

Authenticated collector requests include:

```http
Authorization: Bearer <AIKS_COLLECTOR_TOKEN>
X-AIKS-Instance-Id: <collector instance_id>
X-AIKS-WeKnora-API-Key: <employee workspace API key>
X-AIKS-WeKnora-KB-ID: <employee private KB id>
```

The employee key is used for live identity/target validation only. The collector
removes it from the request before business handlers run and never writes it to
SQLite. Durable retries use the server-side platform key together with the
validated tenant id and KB id.

A browser `Origin` header is rejected. The server accepts reverse-proxied Host
values in collector mode; personal ServiceLocal mode remains pinned to its exact
loopback Host.

## Delivery behavior

Snapshot acceptance and WeKnora delivery are intentionally decoupled. The
collector first commits the snapshot, then writes sanitized Markdown into the
durable WeKnora outbox. Network/5xx/rate-limit failures retry with bounded
backoff. Restarting the collector resumes pending deliveries.

The stable `external_id = aiks-<sha256>` is also enforced by the
`feature/aiks-team-integration` WeKnora fork, closing the duplicate window
where a create succeeded remotely but its HTTP response was lost.

## WeKnora fork

Deploy `Sunan869/aiks-WeKnora:feature/aiks-team-integration`. The fork adds:

- AIKS as a first-class ingestion channel;
- stable AIKS external identity on manual knowledge;
- idempotent create/replay behavior;
- a larger AIKS-only manual payload budget;
- AIKS source labels in the knowledge UI.

## Local POC fallback

Desktop `service_local` can still talk directly to WeKnora for development.
For the team deployment, prefer collector mode so employee machines hold only
their own workspace identity key; the cross-workspace platform key remains
server-side.

## Sharing and login

The WeKnora fork provides DingTalk login, direct user KB sharing and
DingTalk department-to-organization membership synchronization. Collected
sessions remain in each employee's private workspace/KB first; sharing is
applied afterwards in WeKnora. DingTalk directory state and ACLs are not
reimplemented in `aiks-service`.
