#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationSnapshot {
    pub target_id: Option<String>,
    pub synced_hash: Option<String>,
    pub target_hash: Option<String>,
    pub local_hash: String,
    pub remote_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationDecision {
    Create,
    Reuse { doc_id: String },
    Update { doc_id: String },
    Conflict { doc_id: String },
}

pub fn decide_migration(snapshot: &MigrationSnapshot) -> MigrationDecision {
    let Some(doc_id) = snapshot.target_id.clone() else {
        return MigrationDecision::Create;
    };

    let (Some(synced_hash), Some(target_hash)) = (
        snapshot.synced_hash.as_deref(),
        snapshot.target_hash.as_deref(),
    ) else {
        return MigrationDecision::Reuse { doc_id };
    };

    let Some(remote_hash) = snapshot.remote_hash.as_deref() else {
        return MigrationDecision::Conflict { doc_id };
    };

    let local_changed = snapshot.local_hash != synced_hash;
    let remote_changed = remote_hash != target_hash;

    if remote_changed {
        MigrationDecision::Conflict { doc_id }
    } else if local_changed {
        MigrationDecision::Update { doc_id }
    } else {
        MigrationDecision::Reuse { doc_id }
    }
}
