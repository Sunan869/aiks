//! Transport-independent service contracts. No Tauri or source filesystem access.

mod adoption;
mod contracts;
mod ingestion;
mod repo;
mod revision;
mod validation;

pub use adoption::{adopt_local_state, AdoptionManifest, AdoptionReport, SessionAdoption};
pub use contracts::{ServiceError, SnapshotReceipt, SnapshotSubmission, ValidatedSnapshot};
pub use repo::{LocalContext, ServiceStore};
pub use revision::{RevisionFence, SupersededRevision};
pub(crate) use validation::snapshot_content_hash;
pub use validation::validate_submission;

pub mod query;
mod runtime;
pub use runtime::{ServiceRuntime, ServiceRuntimeConfig};
