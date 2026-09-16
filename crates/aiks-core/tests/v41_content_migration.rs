use aiks_core::knowledge::migration::{decide_migration, MigrationDecision, MigrationSnapshot};

fn snapshot(
    target_id: Option<&str>,
    synced_hash: Option<&str>,
    target_hash: Option<&str>,
    local_hash: &str,
    remote_hash: Option<&str>,
) -> MigrationSnapshot {
    MigrationSnapshot {
        target_id: target_id.map(str::to_string),
        synced_hash: synced_hash.map(str::to_string),
        target_hash: target_hash.map(str::to_string),
        local_hash: local_hash.to_string(),
        remote_hash: remote_hash.map(str::to_string),
    }
}

#[test]
fn unpublished_legacy_knowledge_is_created_in_siyuan() {
    let decision = decide_migration(&snapshot(None, None, None, "local-a", None));
    assert_eq!(decision, MigrationDecision::Create);
}

#[test]
fn unchanged_existing_document_is_reused() {
    let decision = decide_migration(&snapshot(
        Some("doc-1"),
        Some("local-a"),
        Some("remote-a"),
        "local-a",
        Some("remote-a"),
    ));
    assert_eq!(
        decision,
        MigrationDecision::Reuse {
            doc_id: "doc-1".into()
        }
    );
}

#[test]
fn locally_changed_only_document_is_updated_before_cutover() {
    let decision = decide_migration(&snapshot(
        Some("doc-1"),
        Some("local-a"),
        Some("remote-a"),
        "local-b",
        Some("remote-a"),
    ));
    assert_eq!(
        decision,
        MigrationDecision::Update {
            doc_id: "doc-1".into()
        }
    );
}

#[test]
fn remotely_changed_document_is_preserved_as_conflict() {
    let decision = decide_migration(&snapshot(
        Some("doc-1"),
        Some("local-a"),
        Some("remote-a"),
        "local-a",
        Some("remote-b"),
    ));
    assert_eq!(
        decision,
        MigrationDecision::Conflict {
            doc_id: "doc-1".into()
        }
    );
}

#[test]
fn both_sides_changed_document_is_preserved_as_conflict() {
    let decision = decide_migration(&snapshot(
        Some("doc-1"),
        Some("local-a"),
        Some("remote-a"),
        "local-b",
        Some("remote-b"),
    ));
    assert_eq!(
        decision,
        MigrationDecision::Conflict {
            doc_id: "doc-1".into()
        }
    );
}

#[test]
fn existing_document_without_a_baseline_is_reused_conservatively() {
    let decision = decide_migration(&snapshot(
        Some("doc-1"),
        None,
        None,
        "local-a",
        Some("remote-a"),
    ));
    assert_eq!(
        decision,
        MigrationDecision::Reuse {
            doc_id: "doc-1".into()
        }
    );
}
