# AIKS single-company deployment

This directory is a deployment reference, not a ready-to-run configuration. The checked-in template keeps team mode disabled and contains no credentials.

## Prepare

1. Copy `service.toml.example` to `/etc/aiks/service.toml`.
2. Copy `.env.example` to `/etc/aiks/team.env` and set mode `0600`, or configure an equivalent service-manager secret source.
3. Fill the single-company CorpId, Client ID, callback, authorized department IDs and server-side secrets. Keep SiYuan bound to loopback.
4. Set both `team.enabled=true` and `team.dingtalk.enabled=true`.
5. Run `/opt/aiks/bin/aiks-service --config /etc/aiks/service.toml --check-config`.
6. Only after the check succeeds, install/start the systemd unit and enable the Nginx TLS endpoint.

The team service itself listens only on the configured numeric loopback address. Nginx terminates TLS and forwards only the allowlisted AIKS paths with the configured public Host. Do not add generic `/proxy`, SQL, file, SiYuan or arbitrary URL forwarding. Do not add `Forwarded` or `X-Forwarded-*` identity headers; the service rejects them.

## Secrets

`AIKS_DINGTALK_CLIENT_SECRET`, `AIKS_SIYUAN_TOKEN` and optional model keys are server-only. They must not be put in desktop configuration, URLs, logs or Git. On Unix, DingTalk Client Secret may alternatively use `client_secret_file` with an absolute, non-symlink file owned by the service account and no group/other permissions.

## Backup and rollback

Before a backup: stop accepting new writes, let AIKS work drain, stop the internal SiYuan instance, checkpoint/copy the SQLite database together with the SiYuan workspace, and write a manifest containing file hashes but no usernames or secrets. Restore first into a new isolated directory and verify it before touching the original installation.

For rollback, stop the service and restore the matching database + SiYuan workspace pair. Do not let an older binary open a database after a newer schema has been introduced. A failed restore must leave the original data directory intact.

## Validation status

CI uses synthetic identities, temporary databases and synthetic secrets. Real DingTalk enterprise authorization, real SiYuan recovery and the desktop browser callback are separate deployment evidence and require manually supplied environment-specific parameters.
