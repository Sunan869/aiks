-- ── Knowledge sync conflict baseline ────────────────────────────────────────────
-- target_hash stores the hash of the remote SiYuan text (via getBlockKramdown,
-- attributes stripped) captured at the last successful sync. The earlier design
-- compared the remote export against synced_hash (the LOCAL markdown hash),
-- which always differs after SiYuan re-serializes the content — flagging every
-- item as CONFLICT. With a stored baseline:
--   NULL  → skip conflict detection (baseline unknown)
--   value → remote hash must match, otherwise someone edited it manually.

ALTER TABLE knowledge_sync_target ADD COLUMN target_hash TEXT;
