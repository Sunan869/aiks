//! Normalized, complete directory input, supplied only by a trusted adapter.
//! External IDs here are never accepted as authenticated AIKS user identities.
#[derive(Clone)]
pub struct DirectorySnapshot {
    pub complete: bool,
    pub scope: Vec<String>,
    pub users: Vec<DirectoryUser>,
    pub orgs: Vec<OrgRecord>,
    pub memberships: Vec<Membership>,
    pub observed_at: u64,
}
#[derive(Clone)]
pub struct DirectoryUser {
    pub external_user_id: String,
    pub union_id: String,
    pub display_name: String,
    pub active: bool,
}
#[derive(Clone)]
pub struct OrgRecord {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
}
#[derive(Clone)]
pub struct Membership {
    /// External organization-specific user ID in adapter input only.
    pub user_id: String,
    pub org_id: String,
}
#[derive(Clone)]
pub struct UserRecord {
    pub id: String,
    pub external_user_id: String,
    pub union_id: String,
    pub display_name: String,
    pub active: bool,
    pub private_space_id: String,
    pub auth_version: u64,
}
