//! Revision identity carried by snapshot-backed pipeline work.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionFence {
    pub session_id: i64,
    pub snapshot_id: String,
    pub revision: u32,
}
