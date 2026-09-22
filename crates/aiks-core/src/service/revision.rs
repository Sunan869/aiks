//! A revision is checked inside the same transaction as every derived write.

use anyhow::{ensure, Context};
use rusqlite::{params, OptionalExtension, Transaction};

use crate::storage::StateDb;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionFence {
    pub session_id: i64,
    pub snapshot_id: String,
    pub revision: u32,
}

/// Only a valid historical snapshot overtaken by a newer snapshot has this error.
/// Missing rows, corrupt identities and SQLite failures remain ordinary errors.
#[derive(Debug, thiserror::Error)]
#[error("snapshot superseded by a newer accepted revision")]
pub struct SupersededRevision;

impl RevisionFence {
    pub fn check_in_tx(&self, tx: &Transaction<'_>) -> anyhow::Result<()> {
        let (session_id, revision, current): (i64, u32, u32) = tx
            .query_row(
                "SELECT s.session_id, s.revision, b.current_revision
                 FROM service_session_snapshot s
                 JOIN service_session_binding b ON b.session_id=s.session_id
                 WHERE s.id=?1",
                [&self.snapshot_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .context("Snapshot revision identity is missing")?;
        ensure!(
            session_id == self.session_id && revision == self.revision,
            "Snapshot revision identity is inconsistent"
        );
        if revision < current {
            return Err(SupersededRevision.into());
        }
        ensure!(
            revision == current,
            "Snapshot revision is ahead of its binding"
        );
        Ok(())
    }

    pub(crate) fn check_session_in_tx(
        &self,
        tx: &Transaction<'_>,
        session_id: i64,
    ) -> anyhow::Result<()> {
        ensure!(
            self.session_id == session_id,
            "Revision belongs to another session"
        );
        self.check_in_tx(tx)
    }

    pub(crate) fn check_run_in_tx(&self, tx: &Transaction<'_>, run_id: &str) -> anyhow::Result<()> {
        let matches: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_job_input i
             JOIN pipeline_run r ON r.id=i.pipeline_run_id
             WHERE i.pipeline_run_id=?1 AND i.snapshot_id=?2 AND r.session_id=?3)",
            params![run_id, self.snapshot_id, self.session_id],
            |row| row.get(0),
        )?;
        ensure!(matches, "Revision belongs to another pipeline run");
        self.check_in_tx(tx)
    }

    /// An early read avoids unnecessary model calls; writes must still recheck.
    pub(crate) fn check_current(&self, db: &StateDb) -> anyhow::Result<()> {
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        self.check_in_tx(&tx)
    }

    pub(crate) fn for_job_in_tx(
        tx: &Transaction<'_>,
        job_id: &str,
    ) -> anyhow::Result<Option<Self>> {
        let fence = tx
            .query_row(
                "SELECT s.session_id, s.id, s.revision FROM service_job_input i
             JOIN service_session_snapshot s ON s.id=i.snapshot_id
             JOIN pipeline_job j ON j.id=i.durable_job_id
              AND j.pipeline_run_id=i.pipeline_run_id AND j.session_id=s.session_id
             JOIN pipeline_run r ON r.id=i.pipeline_run_id AND r.session_id=s.session_id
             WHERE j.id=?1",
                [job_id],
                |row| {
                    Ok(Self {
                        session_id: row.get(0)?,
                        snapshot_id: row.get(1)?,
                        revision: row.get(2)?,
                    })
                },
            )
            .optional()?;
        if fence.is_none() {
            let needs_snapshot: bool = tx.query_row(
                "SELECT r.pipeline_version LIKE 'service-v1/%' FROM pipeline_job j
                 JOIN pipeline_run r ON r.id=j.pipeline_run_id WHERE j.id=?1",
                [job_id],
                |row| row.get(0),
            )?;
            ensure!(!needs_snapshot, "Service job lost its immutable input");
        }
        Ok(fence)
    }

    pub(crate) fn record_indexed_in_tx(&self, tx: &Transaction<'_>) -> anyhow::Result<()> {
        self.check_in_tx(tx)?;
        tx.execute(
            "INSERT INTO service_derived_state(session_id,indexed_revision) VALUES (?1,?2)
             ON CONFLICT(session_id) DO UPDATE SET indexed_revision=excluded.indexed_revision",
            params![self.session_id, self.revision],
        )?;
        Ok(())
    }

    pub(crate) fn record_knowledge_in_tx(&self, tx: &Transaction<'_>) -> anyhow::Result<()> {
        self.check_in_tx(tx)?;
        tx.execute(
            "INSERT INTO service_derived_state(session_id,knowledge_revision) VALUES (?1,?2)
             ON CONFLICT(session_id) DO UPDATE SET knowledge_revision=excluded.knowledge_revision",
            params![self.session_id, self.revision],
        )?;
        Ok(())
    }

    pub(crate) fn record_completed_in_tx(&self, tx: &Transaction<'_>) -> anyhow::Result<()> {
        self.check_in_tx(tx)?;
        tx.execute(
            "INSERT INTO service_derived_state(session_id,completed_revision) VALUES (?1,?2)
             ON CONFLICT(session_id) DO UPDATE SET completed_revision=excluded.completed_revision",
            params![self.session_id, self.revision],
        )?;
        Ok(())
    }
}
