//! Offline adoption contract; not exposed by the HTTP router.
use crate::{model::SourceKind, storage::StateDb};
use super::{LocalContext, ServiceError, SnapshotReceipt, SnapshotSubmission};

#[derive(Clone)]
pub struct SessionAdoption {
    pub session_id: i64,
    pub source: SourceKind,
    pub upstream_id: String,
    pub registration_id: String,
    pub snapshot: Option<SnapshotSubmission>,
}

#[derive(Clone)]
pub struct AdoptionManifest {
    pub service_instance_id: String,
    pub sessions: Vec<SessionAdoption>,
    pub knowledge_ids: Vec<String>,
}

#[derive(Debug, Default)]
pub struct AdoptionReport {
    pub bound: Vec<i64>,
    pub snapshot_ready: Vec<i64>,
    pub source_unavailable: Vec<i64>,
    pub conflict: Vec<i64>,
    pub receipts: Vec<SnapshotReceipt>,
}

pub fn adopt_local_state(
    _db: &StateDb,
    _context: &LocalContext,
    _manifest: &AdoptionManifest,
) -> Result<AdoptionReport, ServiceError> {
    Err(ServiceError::Unavailable)
}
