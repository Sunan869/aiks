use std::sync::Arc;

use aiks_core::{
    storage::StateDb,
    team::{
        Action, AuthPolicy, DirectorySnapshot, DirectoryUser, ExternalLogin, GrantInput,
        GrantTarget, LoginStore, Membership, OrgRecord, TeamError, TeamStore,
    },
};
use sha2::{Digest, Sha256};

fn setup() -> (tempfile::TempDir, Arc<TeamStore>, u64) {
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(
        TeamStore::bind(
            Arc::new(StateDb::open_exclusive(&root.path().join("team.db")).unwrap()),
            "synthetic-corp",
            "synthetic-client",
        )
        .unwrap(),
    );
    let at = 1_000;
    store
        .publish_directory(
            DirectorySnapshot {
                complete: true,
                scope: vec!["root".into()],
                observed_at: at,
                orgs: vec![
                    OrgRecord {
                        id: "root".into(),
                        parent_id: None,
                        name: "Root".into(),
                    },
                    OrgRecord {
                        id: "research".into(),
                        parent_id: Some("root".into()),
                        name: "Research".into(),
                    },
                    OrgRecord {
                        id: "child".into(),
                        parent_id: Some("research".into()),
                        name: "Child".into(),
                    },
                ],
                users: vec![
                    DirectoryUser {
                        external_user_id: "owner".into(),
                        union_id: "uo".into(),
                        display_name: "Owner".into(),
                        active: true,
                    },
                    DirectoryUser {
                        external_user_id: "reader".into(),
                        union_id: "ur".into(),
                        display_name: "Reader".into(),
                        active: true,
                    },
                    DirectoryUser {
                        external_user_id: "child-user".into(),
                        union_id: "uc".into(),
                        display_name: "Child".into(),
                        active: true,
                    },
                ],
                memberships: vec![
                    Membership {
                        user_id: "owner".into(),
                        org_id: "root".into(),
                    },
                    Membership {
                        user_id: "reader".into(),
                        org_id: "research".into(),
                    },
                    Membership {
                        user_id: "child-user".into(),
                        org_id: "child".into(),
                    },
                ],
            },
            at,
        )
        .unwrap();
    (root, store, at)
}

fn context(
    store: &Arc<TeamStore>,
    external: &str,
    union: &str,
    at: u64,
) -> aiks_core::team::TeamContext {
    let login = LoginStore::new(store.clone(), AuthPolicy::default()).unwrap();
    let verifier = hex::encode(Sha256::digest(format!("verifier:{external}").as_bytes()));
    let hash = hex::encode(Sha256::digest(verifier.as_bytes()));
    let start = login.start(&hash, at).unwrap();
    let browser = login
        .open_browser(&start.attempt_id, start.launch_token.expose(), at)
        .unwrap();
    let code = format!("code-{external}");
    let claim = login
        .claim_callback(
            browser.state.expose(),
            browser.nonce.expose(),
            Some(&code),
            at,
        )
        .unwrap();
    login
        .finish_callback(
            &claim,
            ExternalLogin {
                corp_id: "synthetic-corp".into(),
                external_user_id: external.into(),
                union_id: union.into(),
                display_name: external.into(),
            },
            at,
        )
        .unwrap();
    let tokens = login.exchange(&start.attempt_id, &verifier, at).unwrap();
    aiks_core::team::SessionStore::new(store.clone(), AuthPolicy::default())
        .unwrap()
        .authenticate(tokens.access_token.expose(), at)
        .unwrap()
}

#[test]
fn owner_managed_read_grants_are_versioned_union_scoped_and_revocable() {
    let (_root, store, at) = setup();
    let owner = store.user_by_union("uo").unwrap().unwrap();
    let reader = store.user_by_union("ur").unwrap().unwrap();
    let child = store.user_by_union("uc").unwrap().unwrap();
    let research = store.org_id_by_external("research").unwrap().unwrap();
    store.db().conn().execute(
        "INSERT INTO knowledge_item(id,title,summary,category,tags,content,created_at,updated_at) VALUES ('k','K','','','[]','body','x','x')",
        [],
    ).unwrap();
    store.db().conn().execute(
        "INSERT INTO team_knowledge_owner(company_id,knowledge_id,owner_user_id) VALUES (?1,'k',?2)",
        rusqlite::params![store.company_id(), owner.id],
    ).unwrap();
    let owner_ctx = context(&store, "owner", "uo", at + 1);
    let reader_ctx = context(&store, "reader", "ur", at + 1);
    let child_ctx = context(&store, "child-user", "uc", at + 1);

    let initial = store.list_grants(&owner_ctx, "k", at + 1).unwrap();
    assert_eq!(initial.grant_version, 0);
    assert!(initial.grants.is_empty());
    let version = store
        .replace_grants(
            &owner_ctx,
            "k",
            0,
            &[
                GrantInput {
                    target: GrantTarget::User(reader.id.clone()),
                },
                GrantInput {
                    target: GrantTarget::Org {
                        id: research.clone(),
                        descendants: false,
                    },
                },
            ],
            at + 1,
        )
        .unwrap();
    assert_eq!(version, 1);
    assert!(store
        .knowledge_access(&reader_ctx, "k", Action::Read, at + 1)
        .is_ok());
    assert!(matches!(
        store.knowledge_access(&child_ctx, "k", Action::Read, at + 1),
        Err(TeamError::NotFound)
    ));
    assert!(matches!(
        store.list_grants(&reader_ctx, "k", at + 1),
        Err(TeamError::Forbidden)
    ));

    assert_eq!(
        store
            .replace_grants(
                &owner_ctx,
                "k",
                1,
                &[GrantInput {
                    target: GrantTarget::Org {
                        id: research,
                        descendants: false
                    }
                },],
                at + 1
            )
            .unwrap(),
        2
    );
    assert!(store
        .knowledge_access(&reader_ctx, "k", Action::Read, at + 1)
        .is_ok());
    assert!(matches!(
        store.knowledge_access(&child_ctx, "k", Action::Read, at + 1),
        Err(TeamError::NotFound)
    ));
    assert!(matches!(
        store.replace_grants(&owner_ctx, "k", 1, &[], at + 1),
        Err(TeamError::Conflict)
    ));
    assert_eq!(
        store
            .replace_grants(&owner_ctx, "k", 2, &[], at + 1)
            .unwrap(),
        3
    );
    assert!(matches!(
        store.knowledge_access(&reader_ctx, "k", Action::Read, at + 1),
        Err(TeamError::NotFound)
    ));
}

#[test]
fn a_context_from_another_company_cannot_cross_the_database_boundary() {
    let (_root_a, store_a, at) = setup();
    let owner = store_a.user_by_union("uo").unwrap().unwrap();
    store_a.db().conn().execute(
        "INSERT INTO knowledge_item(id,title,summary,category,tags,content,created_at,updated_at) VALUES ('k','K','','','[]','body','x','x')",
        [],
    ).unwrap();
    store_a.db().conn().execute(
        "INSERT INTO team_knowledge_owner(company_id,knowledge_id,owner_user_id) VALUES (?1,'k',?2)",
        rusqlite::params![store_a.company_id(), owner.id],
    ).unwrap();

    let root_b = tempfile::tempdir().unwrap();
    let store_b = Arc::new(
        TeamStore::bind(
            Arc::new(StateDb::open_exclusive(&root_b.path().join("team.db")).unwrap()),
            "other-corp",
            "other-client",
        )
        .unwrap(),
    );
    store_b
        .publish_directory(
            DirectorySnapshot {
                complete: true,
                scope: vec!["root".into()],
                observed_at: at,
                orgs: vec![OrgRecord {
                    id: "root".into(),
                    parent_id: None,
                    name: "Root".into(),
                }],
                users: vec![DirectoryUser {
                    external_user_id: "owner".into(),
                    union_id: "uo".into(),
                    display_name: "Owner".into(),
                    active: true,
                }],
                memberships: vec![Membership {
                    user_id: "owner".into(),
                    org_id: "root".into(),
                }],
            },
            at,
        )
        .unwrap();
    let login = LoginStore::new(store_b.clone(), AuthPolicy::default()).unwrap();
    let verifier = "99".repeat(32);
    let hash = hex::encode(Sha256::digest(verifier.as_bytes()));
    let start = login.start(&hash, at + 1).unwrap();
    let browser = login
        .open_browser(&start.attempt_id, start.launch_token.expose(), at + 1)
        .unwrap();
    let claim = login
        .claim_callback(
            browser.state.expose(),
            browser.nonce.expose(),
            Some("code"),
            at + 1,
        )
        .unwrap();
    login
        .finish_callback(
            &claim,
            ExternalLogin {
                corp_id: "other-corp".into(),
                external_user_id: "owner".into(),
                union_id: "uo".into(),
                display_name: "Owner".into(),
            },
            at + 1,
        )
        .unwrap();
    let tokens = login
        .exchange(&start.attempt_id, &verifier, at + 1)
        .unwrap();
    let foreign = aiks_core::team::SessionStore::new(store_b, AuthPolicy::default())
        .unwrap()
        .authenticate(tokens.access_token.expose(), at + 1)
        .unwrap();
    assert!(matches!(
        store_a.knowledge_access(&foreign, "k", Action::Read, at + 1),
        Err(TeamError::Unauthorized)
    ));
}
