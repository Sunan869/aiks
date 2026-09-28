# AIKS → WeKnora adapter

This branch replaces the old assumption that the team service must own document
ACL, sharing, RAG and a SiYuan content container.

## Responsibility split

AIKS keeps:

- provider discovery and local Session collection;
- normalization to `NormalizedSession`;
- secret redaction and Session → Markdown rendering;
- durable delivery state and the local mapping from a source session to a
  WeKnora knowledge ID.

WeKnora owns:

- users, workspaces, RBAC and sharing;
- knowledge-base document lifecycle;
- indexing, embedding, retrieval and RAG;
- Wiki/history/audit and team-facing knowledge UI.

## Service configuration

```toml
[weknora]
enabled = true
base_url = "https://weknora.example.com"
knowledge_base_id = "replace-with-kb-id"
api_key_env = "AIKS_WEKNORA_API_KEY"
channel = "aiks"
```

The secret is resolved only from the named environment variable. It is not
accepted inline in the TOML.

The API key must be scoped to the destination workspace / knowledge base and
must grant ingestion/write access.

## Delivery semantics

Accepted AIKS snapshots are committed locally first. When WeKnora integration
is enabled, the service then persists a separate SQLite outbox row containing
the sanitized Markdown. Remote availability never changes whether the local
snapshot was accepted.

The background worker:

1. claims due outbox rows serially;
2. creates a WeKnora manual knowledge item on first delivery;
3. stores `(source, external_session_id) -> knowledge_id + content_hash`;
4. updates the same knowledge item for later revisions;
5. skips a remote rewrite when a newer AIKS revision renders to the same hash;
6. retries transport, auth/rate-limit, conflict and 5xx failures with bounded
   exponential backoff;
7. keeps non-retryable 4xx failures as terminal outbox rows for diagnosis;
8. automatically resumes non-terminal rows after process restart.

Each request also carries a stable opaque `external_id = aiks-<sha256>`.
The AIKS WeKnora fork stores that identifier in manual-knowledge metadata so a
future server-side idempotent create can close the remaining "POST succeeded
but the response was lost" duplicate window.

## Current boundary

The existing S2 DingTalk/ACL/SiYuan implementation remains in history while
this adapter is proven. Do not extend that old ACL layer on this branch.
