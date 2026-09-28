# AIKS + WeKnora

The recommended team topology is now the standalone collector:

```text
AIKS Desktop / provider collectors
        |
        | AIKS snapshot protocol
        | Bearer AIKS_COLLECTOR_TOKEN
        v
aiks-service (mode = collector)
        |
        | durable SQLite outbox
        | server-only AIKS_WEKNORA_API_KEY
        v
WeKnora
        |
        +-- workspace / RBAC / sharing
        +-- knowledge / Wiki / revision
        +-- embedding / retrieval / RAG
```

The old central S2 DingTalk/ACL/SiYuan stack is not part of this path.

## Server deployment

```bash
cd deploy/weknora
cp .env.example .env
openssl rand -hex 32
# fill .env with the generated collector token, WeKnora URL, KB ID and API key
docker compose up -d --build
curl http://127.0.0.1:28082/healthz
```

The container publishes on host loopback by default. Put Nginx/TongHttpServer or
another HTTPS reverse proxy in front of it for remote Desktop access. Do not
expose the plain HTTP collector port directly to an untrusted network because
the collector bearer token authorizes Session ingestion.

## Client handshake

`GET /healthz` is public and returns the non-secret `instance_id` and
`space_id`. The client then uses the existing AIKS service protocol:

1. `POST /api/v1/source-registrations`
2. `POST /api/v1/session-snapshots`
3. optional `GET /api/v1/integrations/weknora/status`

Authenticated requests include:

```http
Authorization: Bearer <AIKS_COLLECTOR_TOKEN>
X-AIKS-Instance-Id: <instance_id returned by /healthz>
```

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
For the team deployment, prefer collector mode so employee machines never hold
the WeKnora API key.

## Still separate work

DingTalk login federation into WeKnora and finer individual/department sharing
are WeKnora-side identity/permission tasks. They are intentionally not
reimplemented in aiks-service.
