//! Single-company authorization primitives. Network access remains opt-in and gated.
mod directory;
mod directory_types;
mod directory_validate;
pub mod policy;
mod repo;
mod types;

pub use directory_types::{DirectorySnapshot, DirectoryUser, Membership, OrgRecord, UserRecord};
pub use repo::TeamStore;
pub use types::TeamError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Read,
    Edit,
    ManageShares,
    Archive,
}
