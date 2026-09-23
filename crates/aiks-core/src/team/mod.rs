//! Single-company authorization primitives. Network access remains opt-in and gated.
pub mod policy;
mod repo;
mod types;

pub use repo::TeamStore;
pub use types::TeamError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Read,
    Edit,
    ManageShares,
    Archive,
}
