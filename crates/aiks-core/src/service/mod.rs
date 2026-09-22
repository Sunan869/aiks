//! Transport-independent service contracts. No Tauri or source filesystem access.

mod contracts;
mod repo;
mod validation;

pub use contracts::{ServiceError, SnapshotReceipt, SnapshotSubmission, ValidatedSnapshot};
pub use repo::{LocalContext, ServiceStore};
pub use validation::validate_submission;
