use std::sync::Arc;

use aiks_core::{
    service::ServiceStore,
    storage::{SourceSessionRepo, StateDb},
    team::{TeamError, TeamStore},
};

#[test]
fn company_binding_survives_restart_and_cannot_be_reassigned() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("team.db");
    let (company, instance) = {
        let db = Arc::new(StateDb::open_exclusive(&path).unwrap());
        let store = TeamStore::bind(db.clone(), "synthetic-corp", "synthetic-client").unwrap();
        let again = TeamStore::bind(db.clone(), "synthetic-corp", "synthetic-client").unwrap();
        assert_eq!(store.company_id(), again.company_id());
        assert_eq!(store.instance_id(), again.instance_id());
        assert!(matches!(
            TeamStore::bind(db.clone(), "other-corp", "synthetic-client"),
            Err(TeamError::ConfigInvalid)
        ));
        assert!(matches!(
            TeamStore::bind(db.clone(), "synthetic-corp", "other-client"),
            Err(TeamError::ConfigInvalid)
        ));
        assert!(
            ServiceStore::open(db.clone()).is_err(),
            "a company DB cannot become a local personal identity"
        );
        let personal: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM service_instance", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(personal, 0);
        (
            store.company_id().to_owned(),
            store.instance_id().to_owned(),
        )
    };
    let db = Arc::new(StateDb::open_exclusive(&path).unwrap());
    let store = TeamStore::bind(db, "synthetic-corp", "synthetic-client").unwrap();
    assert_eq!(company, store.company_id());
    assert_eq!(instance, store.instance_id());
}

#[test]
fn personal_and_legacy_data_are_not_adopted_implicitly() {
    for initialized in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let db = Arc::new(StateDb::open_exclusive(&root.path().join("personal.db")).unwrap());
        if initialized {
            ServiceStore::open(db.clone()).unwrap();
        }
        let id = SourceSessionRepo::new(&db)
            .upsert(
                "continue",
                "legacy",
                None,
                None,
                None,
                Some("KEEP"),
                None,
                None,
                None,
            )
            .unwrap();
        assert!(matches!(
            TeamStore::bind(db.clone(), "synthetic-corp", "synthetic-client"),
            Err(TeamError::ConfigInvalid)
        ));
        let title: String = db
            .conn()
            .query_row(
                "SELECT title FROM source_session WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(title, "KEEP");
        let companies: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM team_company", [], |row| row.get(0))
            .unwrap();
        assert_eq!(companies, 0);
    }
}

#[test]
fn binding_requires_writer_ownership_and_strict_identity() {
    let root = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open(&root.path().join("no-lease.db")).unwrap());
    assert!(matches!(
        TeamStore::bind(db, "synthetic-corp", "synthetic-client"),
        Err(TeamError::Storage)
    ));
    let db = Arc::new(StateDb::open_exclusive(&root.path().join("invalid.db")).unwrap());
    for (corp, client) in [
        ("", "client"),
        ("corp", ""),
        (" corp", "client"),
        ("corp", "client\n"),
    ] {
        assert!(matches!(
            TeamStore::bind(db.clone(), corp, client),
            Err(TeamError::ConfigInvalid)
        ));
    }
    db.conn().execute_batch("DROP TABLE team_company").unwrap();
    assert!(
        matches!(
            TeamStore::bind(db, "synthetic-corp", "synthetic-client"),
            Err(TeamError::Storage)
        ),
        "database errors must not turn into an ordinary authorization denial"
    );
}

#[test]
fn sharing_schema_enforces_read_only_and_same_company_ownership() {
    let root = tempfile::tempdir().unwrap();
    let db = Arc::new(StateDb::open_exclusive(&root.path().join("team.db")).unwrap());
    let store = TeamStore::bind(db.clone(), "synthetic-corp", "synthetic-client").unwrap();
    let conn = db.conn();
    for (id, union) in [("owner", "union-a"), ("reader", "union-b")] {
        conn.execute("INSERT INTO team_user(company_id,id,external_user_id,union_id,display_name) VALUES (?1,?2,?2,?3,'Same name')", rusqlite::params![store.company_id(),id,union]).unwrap();
    }
    conn.execute("INSERT INTO knowledge_item(id,title,content,created_at,updated_at) VALUES ('knowledge','Title','Preserve body','test','test')", []).unwrap();
    conn.execute("INSERT INTO team_knowledge_owner(company_id,knowledge_id,owner_user_id) VALUES (?1,'knowledge','owner')", [store.company_id()]).unwrap();
    conn.execute("INSERT INTO document_share_grant(company_id,id,knowledge_id,target_user_id,permission) VALUES (?1,'g1','knowledge','reader','read')", [store.company_id()]).unwrap();
    assert!(conn
        .execute("UPDATE document_share_grant SET permission='edit'", [])
        .is_err());
    assert!(conn
        .execute(
            "UPDATE document_share_grant SET company_id='other-company'",
            []
        )
        .is_err());
    assert!(conn
        .execute(
            "UPDATE document_share_grant SET target_user_id='missing-user'",
            []
        )
        .is_err());
    assert!(conn
        .execute("UPDATE document_share_grant SET include_descendants=1", [])
        .is_err());
    assert!(conn.execute("INSERT INTO team_user(company_id,id,external_user_id,union_id,display_name) VALUES (?1,'new-id','new-external','union-a','Same name')", [store.company_id()]).is_err());
    let version: i64 = conn
        .query_row(
            "SELECT grant_version FROM team_knowledge_owner WHERE knowledge_id='knowledge'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 0);
}
