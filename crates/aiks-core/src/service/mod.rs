//! Transport-independent service contracts. No Tauri or source filesystem access.

mod contracts;
mod validation;

pub use contracts::{ServiceError, SnapshotReceipt, SnapshotSubmission, ValidatedSnapshot};
pub use validation::validate_submission;
