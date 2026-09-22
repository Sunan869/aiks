//! Transport-independent service contracts. No Tauri or source filesystem access.

mod contracts;
mod ingestion;
mod repo;
mod revision;
mod validation;

pub use contracts::{ServiceError, SnapshotReceipt, SnapshotSubmission, ValidatedSnapshot};
pub use repo::{LocalContext, ServiceStore};
pub use revision::{RevisionFence, SupersededRevision};
pub(crate) use validation::snapshot_content_hash;
pub use validation::validate_submission;
