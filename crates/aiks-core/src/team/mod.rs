//! Single-company authorization primitives, not a network entrypoint.
pub mod policy;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Read,
    Edit,
    ManageShares,
    Archive,
}
