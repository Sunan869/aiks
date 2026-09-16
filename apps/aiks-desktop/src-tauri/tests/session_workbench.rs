use aiks_core::storage::{SourceSessionRepo, StateDb};
use aiks_desktop_lib::session_workbench::lookup_session_doc_id;

#[test]
fn session_doc_binding_is_available_to_the_desktop_workbench() {
    let path = std::env::temp_dir().join(format!(
        "aiks-session-workbench-{}.db",
        uuid::Uuid::new_v4()
    ));
    let db = StateDb::open(&path).unwrap();
    let session_id = SourceSessionRepo::new(&db)
        .upsert(
            "opencode",
            "session-1",
            None,
            None,
            Some("AIKS"),
            Some("Session one"),
            None,
            Some("source-hash"),
            Some("v1"),
        )
        .unwrap();

    db.conn()
        .execute(
            "UPDATE source_session SET siyuan_doc_id = 'session-doc-1' WHERE id = ?1",
            [session_id],
        )
        .unwrap();

    assert_eq!(
        lookup_session_doc_id(&db, session_id).unwrap().as_deref(),
        Some("session-doc-1")
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}
