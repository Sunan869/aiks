# AIKS + WeKnora POC

This directory documents the first deployable integration slice on
`feature/aiks-weknora-adapter`.

## Topology

```text
AIKS Desktop
  -> local provider collection
  -> owned aiks-service (ServiceLocal, loopback only)
  -> durable SQLite WeKnora outbox
  -> WeKnora REST API
  -> WeKnora workspace / knowledge base / RAG
```

The old central AIKS S2 DingTalk/ACL/SiYuan stack is not part of this POC.

## WeKnora

Use the `feature/aiks-team-integration` branch of
`Sunan869/aiks-WeKnora`. Because this fork contains AIKS-specific ingestion
changes, build/deploy the fork itself rather than pulling an unmodified upstream
release image.

After WeKnora is running:

1. create the target workspace / knowledge base;
2. create a scoped API key that can write that knowledge base;
3. record the knowledge-base ID.

## AIKS Desktop

In the normal AIKS configuration:

```toml
[backend]
mode = "service_local"

[weknora]
enabled = true
base_url = "https://weknora.example.com"
knowledge_base_id = "replace-with-kb-id"
api_key_env = "AIKS_WEKNORA_API_KEY"
channel = "aiks"
```

Set the secret in the environment that launches AIKS Desktop:

```text
AIKS_WEKNORA_API_KEY=<scoped-key>
```

The owned aiks-service inherits the Desktop process environment. The raw key is
never copied into `aiks.toml` or the generated service runtime TOML.

## Verification

Collect one session from AIKS. In the local service status, the
`weknora.pending` count should return to zero. In WeKnora, the created manual
knowledge should show source channel `AIKS`.

Edit or extend the same source session and collect it again. The same WeKnora
knowledge ID should be updated rather than duplicated.

If WeKnora is temporarily unavailable, local snapshot acceptance still
succeeds. The delivery stays in the SQLite outbox and is retried after WeKnora
recovers.

## Not yet part of this slice

- DingTalk login federation into WeKnora;
- automatic provisioning of per-user WeKnora credentials;
- document-to-individual-user sharing beyond WeKnora's current sharing model;
- removal of the old S2 implementation.

Those are separate team-identity/sharing tasks after this ingestion POC is
green.
