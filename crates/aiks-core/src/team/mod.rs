//! Single-company authorization primitives. Network access remains opt-in and gated.
mod content;
mod directory;
mod directory_audit;
mod directory_types;
mod directory_validate;
mod imports;
pub mod policy;
pub mod provider;
mod repo;
mod shares;
mod types;

pub use content::{
    ContentOperation, ContentWorker, ManagedAsset, MAX_MANAGED_ASSET_BYTES, MAX_TEAM_CONTENT_BYTES,
};
pub use directory_types::{DirectorySnapshot, DirectoryUser, Membership, OrgRecord, UserRecord};
pub use imports::ImportReceipt;
pub use provider::{ExternalLogin, IdentityProvider};
pub use repo::TeamStore;
pub(crate) use shares::knowledge_access_in_conn;
pub use shares::{GrantInput, GrantTarget, ShareGrant, ShareState};
pub use types::TeamError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Read,
    Edit,
    ManageShares,
    Archive,
}

mod auth_types;
mod login;
mod sessions;
pub use auth_types::{AuthPolicy, AuthSecret, SessionTokens, TeamContext, TeamIdentity};
pub use login::{BrowserLogin, CallbackClaim, LoginStart, LoginStore};
pub use sessions::SessionStore;
