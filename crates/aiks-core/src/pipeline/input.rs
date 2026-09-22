//! The service input adapter reads a persisted revision, never a client path.

use std::sync::Arc;

use anyhow::{ensure, Context};
use rusqlite::{params, OptionalExtension};

use crate::model::NormalizedSession;
use crate::providers::ProviderRegistry;
use crate::service::{snapshot_content_hash, RevisionFence};
use crate::storage::StateDb;

#[derive(Clone)]
pub(super) enum PipelineInputSource {
    LegacyProviders(Arc<ProviderRegistry>),
    PersistedSnapshots,
}

impl PipelineInputSource {
    pub(super) fn is_snapshot(&self) -> bool {
        matches!(self, Self::PersistedSnapshots)
    }
}

pub struct SnapshotInput;

impl SnapshotInput {
    pub fn load_for_run(
        db: &StateDb,
        run_id: &str,
    ) -> anyhow::Result<(NormalizedSession, RevisionFence)> {
        let stored = {
            let conn = db.conn();
            conn.query_row(
                "SELECT s.id, s.session_id, s.revision, s.parser_version, s.content_hash,
                        CASE WHEN length(s.canonical_json)<=16777216 THEN s.canonical_json END,
                        ss.source, ss.external_session_id, b.upstream_id,
                        pr.source_hash, pj.source_hash
                 FROM service_job_input i
                 JOIN service_session_snapshot s ON s.id=i.snapshot_id
                 JOIN service_session_binding b ON b.session_id=s.session_id
                 JOIN service_source_registration r
                   ON r.id=b.registration_id AND r.principal_id=b.principal_id
                  AND r.space_id=b.space_id
                 JOIN source_session ss ON ss.id=s.session_id AND ss.source=r.source
                 JOIN pipeline_run pr ON pr.id=i.pipeline_run_id AND pr.session_id=ss.id
                 JOIN pipeline_job pj ON pj.id=i.durable_job_id
                  AND pj.pipeline_run_id=pr.id AND pj.session_id=ss.id
                  AND pj.source=ss.source AND pj.external_session_id=ss.external_session_id
                 WHERE i.pipeline_run_id=?1",
                params![run_id],
                |row| {
                    Ok(StoredInput {
                        fence: RevisionFence {
                            snapshot_id: row.get(0)?,
                            session_id: row.get(1)?,
                            revision: row.get(2)?,
                        },
                        parser_version: row.get(3)?,
                        content_hash: row.get(4)?,
                        bytes: row.get(5)?,
                        source: row.get(6)?,
                        external_id: row.get(7)?,
                        upstream_id: row.get(8)?,
                        run_hash: row.get(9)?,
                        job_hash: row.get(10)?,
                    })
                },
            )
            .optional()?
            .context("Persisted pipeline input is missing or inconsistent")?
        };
        let bytes = stored.bytes.context("Persisted input exceeds its byte budget")?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("Persisted pipeline input is invalid"))?;
        let hash = snapshot_content_hash(&stored.parser_version, &value)?;
        ensure!(
            hash == stored.content_hash
                && stored.run_hash.as_deref() == Some(hash.as_str())
                && stored.job_hash == hash,
            "Persisted pipeline input failed its integrity check"
        );
        let mut session: NormalizedSession = serde_json::from_value(value)
            .map_err(|_| anyhow::anyhow!("Persisted session structure is invalid"))?;
        ensure!(
            session.source.as_str() == stored.source
                && session.external_session_id == stored.upstream_id,
            "Persisted session identity is inconsistent"
        );
        // Downstream indexing validates AIKS identity, not the device's ID.
        // Retain the immutable original in storage, but never expose it as a
        // filesystem instruction to the service pipeline.
        session.external_session_id = stored.external_id;
        session.source_path = None;
        session.project_path = None;
        Ok((session, stored.fence))
    }
}

struct StoredInput {
    fence: RevisionFence,
    parser_version: String,
    content_hash: String,
    bytes: Option<Vec<u8>>,
    source: String,
    external_id: String,
    upstream_id: String,
    run_hash: Option<String>,
    job_hash: String,
}
