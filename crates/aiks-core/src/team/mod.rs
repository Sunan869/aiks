//! Single-company authorization primitives. Network access remains opt-in and gated.
mod directory;
mod directory_audit;
mod directory_types;
mod directory_validate;
pub mod policy;
pub mod provider;
mod repo;
mod types;

pub use directory_types::{DirectorySnapshot, DirectoryUser, Membership, OrgRecord, UserRecord};
pub use provider::{ExternalLogin, IdentityProvider};
pub use repo::TeamStore;
pub use types::TeamError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Read,
    Edit,
    ManageShares,
    Archive,
}
