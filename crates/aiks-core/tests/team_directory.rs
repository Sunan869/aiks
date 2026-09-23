use std::sync::Arc;

use aiks_core::{storage::StateDb, team::{DirectorySnapshot, DirectoryUser, Membership, OrgRecord, TeamError, TeamStore}};

fn snapshot(time: u64) -> DirectorySnapshot {
    DirectorySnapshot {
        complete: true,
        scope: vec!["root".into()],
        observed_at: time,
        orgs: vec![
            OrgRecord { id: "root".into(), parent_id: None, name: "Company".into() },
            OrgRecord { id: "research".into(), parent_id: Some("root".into()), name: "Research".into() },
            OrgRecord { id: "backend".into(), parent_id: Some("research".into()), name: "Backend".into() },
        ],
        users: vec![
            DirectoryUser { external_user_id: "a".into(), union_id: "ua".into(), display_name: "Same name".into(), active: true },
            DirectoryUser { external_user_id: "b".into(), union_id: "ub".into(), display_name: "Same name".into(), active: true },
            DirectoryUser { external_user_id: "c".into(), union_id: "uc".into(), display_name: "C".into(), active: true },
        ],
        memberships: vec![
            Membership { user_id: "a".into(), org_id: "research".into() },
            Membership { user_id: "b".into(), org_id: "backend".into() },
            Membership { user_id: "c".into(), org_id: "research".into() },
            Membership { user_id: "c".into(), org_id: "backend".into() },
        ],
    }
}
fn setup() -> (tempfile::TempDir, TeamStore) {
    let root = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open_exclusive(&root.path().join("team.db")).unwrap());
    let store = TeamStore::bind(db, "synthetic-corp", "synthetic-client").unwrap();
    (root, store)
}

#[test]
fn directory_publication_allocates_stable_identity_and_bounded_freshness() {
    let (_root, store) = setup();
    assert!(!store.directory_is_fresh(1000, 900).unwrap());
    assert_eq!(store.publish_directory(snapshot(1000), 1000).unwrap(), 1);
    let a = store.user_by_union("ua").unwrap().unwrap();
    let b = store.user_by_union("ub").unwrap().unwrap();
    assert_ne!(a.id, b.id, "names must not merge identities");
    assert_ne!(a.private_space_id, b.private_space_id);
    assert_ne!(a.id, a.external_user_id);
    assert!(store.directory_is_fresh(1900, 900).unwrap());
    assert!(!store.directory_is_fresh(1901, 900).unwrap());
    assert!(!store.directory_is_fresh(999, 900).unwrap());
    assert_eq!(store.publish_directory(snapshot(1010), 1010).unwrap(), 2);
    let same = store.user_by_union("ua").unwrap().unwrap();
    assert_eq!(a.id, same.id);
    assert_eq!(a.private_space_id, same.private_space_id);
}

#[test]
fn direct_department_and_subtree_memberships_are_not_equivalent() {
    let (_root, store) = setup();
    store.publish_directory(snapshot(1000), 1000).unwrap();
    let a = store.user_by_union("ua").unwrap().unwrap();
    let b = store.user_by_union("ub").unwrap().unwrap();
    let c = store.user_by_union("uc").unwrap().unwrap();
    let research = store.org_id_by_external("research").unwrap().unwrap();
    assert!(store.is_org_member(&a.id, &research, false).unwrap());
    assert!(!store.is_org_member(&b.id, &research, false).unwrap());
    assert!(store.is_org_member(&b.id, &research, true).unwrap());
    assert!(store.is_org_member(&c.id, &research, false).unwrap());
    assert!(!store.is_org_member("unknown", &research, true).unwrap());
}

#[test]
fn incomplete_cyclic_or_inconsistent_snapshots_cannot_replace_current_directory() {
    let (_root, store) = setup();
    store.publish_directory(snapshot(1000), 1000).unwrap();
    let b = store.user_by_union("ub").unwrap().unwrap();
    let mut cases = Vec::new();
    let mut s = snapshot(1010); s.complete = false; cases.push(s);
    let mut s = snapshot(1010); s.orgs[1].parent_id = Some("backend".into()); cases.push(s);
    let mut s = snapshot(1010); s.orgs[1].parent_id = Some("research".into()); cases.push(s);
    let mut s = snapshot(1010); s.orgs[1].parent_id = Some("missing".into()); cases.push(s);
    let mut s = snapshot(1010); s.users[1].union_id = "ua".into(); cases.push(s);
    let mut s = snapshot(1010); s.memberships[0].org_id = "missing".into(); cases.push(s);
    let mut s = snapshot(1010); s.memberships[0].user_id = "missing".into(); cases.push(s);
    let mut s = snapshot(1010); s.scope = vec!["missing".into()]; cases.push(s);
    for invalid in cases {
        assert!(store.publish_directory(invalid, 1010).is_err());
        assert_eq!(store.directory_generation().unwrap(), 1);
        assert!(store.user_by_union("ub").unwrap().unwrap().active);
        assert_eq!(store.user_by_union("ub").unwrap().unwrap().id, b.id);
    }
    assert!(matches!(store.publish_directory(snapshot(999), 999), Err(TeamError::Conflict)));
}

#[test]
fn moving_departments_changes_membership_without_reassigning_a_user() {
    let (_root, store) = setup();
    store.publish_directory(snapshot(1000), 1000).unwrap();
    let b = store.user_by_union("ub").unwrap().unwrap();
    let research = store.org_id_by_external("research").unwrap().unwrap();
    let mut moved = snapshot(1010);
    moved.memberships[1].org_id = "research".into();
    store.publish_directory(moved, 1010).unwrap();
    assert!(store.is_org_member(&b.id, &research, false).unwrap());
    assert_eq!(store.user_by_union("ub").unwrap().unwrap().id, b.id);
}

#[test]
fn deactivation_advances_auth_generation_and_reactivation_does_not_undo_it() {
    let (_root, store) = setup();
    store.publish_directory(snapshot(1000), 1000).unwrap();
    let before = store.user_by_union("ub").unwrap().unwrap();
    store.mark_member_inactive(&before.id, 1001).unwrap();
    let inactive = store.user_by_union("ub").unwrap().unwrap();
    assert!(!inactive.active);
    assert!(inactive.auth_version > before.auth_version);
    store.publish_directory(snapshot(1010), 1010).unwrap();
    let restored = store.user_by_union("ub").unwrap().unwrap();
    assert!(restored.active);
    assert_eq!(restored.auth_version, inactive.auth_version, "old tokens cannot revive");
    let mut removed = snapshot(1020);
    removed.users.retain(|u| u.external_user_id != "b");
    removed.memberships.retain(|m| m.user_id != "b");
    store.publish_directory(removed, 1020).unwrap();
    assert!(!store.user_by_union("ub").unwrap().unwrap().active);
}

#[test]
fn sql_failure_rolls_back_member_changes_and_directory_generation() {
    let (_root, store) = setup();
    store.publish_directory(snapshot(1000), 1000).unwrap();
    store.db().conn().execute_batch("CREATE TRIGGER reject_generation BEFORE UPDATE OF directory_generation ON team_company BEGIN SELECT RAISE(ABORT,'synthetic rollback'); END;").unwrap();
    let mut update = snapshot(1010); update.users[1].active = false;
    assert!(matches!(store.publish_directory(update, 1010), Err(TeamError::Storage)));
    assert_eq!(store.directory_generation().unwrap(), 1);
    assert!(store.user_by_union("ub").unwrap().unwrap().active);
}
