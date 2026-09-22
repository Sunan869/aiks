use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Arc;

use aiks_core::service::ServiceStore;
use aiks_core::storage::StateDb;

#[test]
fn local_identity_and_schema_survive_reopening_without_rebinding_legacy_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let identity = {
        let db = Arc::new(StateDb::open_exclusive(&path).unwrap());
        db.conn()
            .execute_batch(
                "INSERT INTO source_session
                 (id, source, external_session_id, title, last_seen_at, created_at,
                  updated_at, siyuan_doc_id)
                 VALUES (71, 'continue', 'legacy', 'Legacy', 'now', 'now', 'now', 'doc-71');
                 INSERT INTO sync_target
                 (session_id, sink, target_id, status)
                 VALUES (71, 'siyuan', 'doc-71', 'SYNCED');",
            )
            .unwrap();
        let store = ServiceStore::open(db.clone()).unwrap();
        let context = store.local_context();
        assert!(!context.instance_id().is_empty());
        assert!(!context.principal_id().is_empty());
        assert!(!context.space_id().is_empty());
        assert_eq!(
            db.conn()
                .query_row("SELECT COUNT(*) FROM service_session_binding", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        context
    };
    let db = Arc::new(StateDb::open_exclusive(&path).unwrap());
    let store = ServiceStore::open(db.clone()).unwrap();
    assert_eq!(identity, store.local_context());
    let row: (i64, String, String) = db
        .conn()
        .query_row(
            "SELECT ss.id, ss.external_session_id, st.target_id
             FROM source_session ss JOIN sync_target st ON st.session_id=ss.id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(row, (71, "legacy".into(), "doc-71".into()));
}

#[test]
fn only_one_business_writer_can_open_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let first = StateDb::open_exclusive(&path).unwrap();
    assert!(StateDb::open_exclusive(&path).is_err());
    assert!(StateDb::open_exclusive(&dir.path().join(".").join("state.db")).is_err());
    drop(first);
    assert!(StateDb::open_exclusive(&path).is_ok());
}

#[test]
fn a_second_process_is_rejected_before_it_can_run_migrations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let first = StateDb::open_exclusive(&path).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "writer_child_probe", "--nocapture"])
        .env("AIKS_TEST_LOCK_PATH", &path)
        .env("AIKS_TEST_LOCK_MODE", "denied")
        .status()
        .unwrap();
    assert!(status.success());
    drop(first);
}

#[test]
fn an_abrupt_process_exit_releases_the_writer_lease() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "writer_child_probe", "--nocapture"])
        .env("AIKS_TEST_LOCK_PATH", &path)
        .env("AIKS_TEST_LOCK_MODE", "hold")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let reader = BufReader::new(child.stdout.take().unwrap());
    let mut ready = false;
    for line in reader.lines() {
        if line.unwrap().contains("AIKS_LEASE_READY") {
            ready = true;
            break;
        }
    }
    assert!(ready, "child must acquire its lease before the crash");
    assert!(StateDb::open_exclusive(&path).is_err());
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(StateDb::open_exclusive(&path).is_ok());
}

#[test]
fn writer_child_probe() {
    let Some(path) = std::env::var_os("AIKS_TEST_LOCK_PATH") else {
        return;
    };
    if std::env::var("AIKS_TEST_LOCK_MODE").unwrap() == "denied" {
        assert!(StateDb::open_exclusive(std::path::Path::new(&path)).is_err());
        return;
    }
    let _db = StateDb::open_exclusive(std::path::Path::new(&path)).unwrap();
    println!("AIKS_LEASE_READY");
    use std::io::Write;
    std::io::stdout().flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(30));
}

#[cfg(unix)]
#[test]
fn symlinked_database_parent_and_lock_are_rejected_without_writes() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), dir.path().join("linked")).unwrap();
    assert!(StateDb::open_exclusive(&dir.path().join("linked/state.db")).is_err());
    assert!(!outside.path().join("state.db").exists());
    let target = outside.path().join("unrelated");
    std::fs::write(&target, "unchanged").unwrap();
    symlink(&target, dir.path().join("state.db")).unwrap();
    assert!(StateDb::open_exclusive(&dir.path().join("state.db")).is_err());
    std::fs::remove_file(dir.path().join("state.db")).unwrap();
    symlink(&target, dir.path().join("state.db.aiks-lock")).unwrap();
    assert!(StateDb::open_exclusive(&dir.path().join("state.db")).is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "unchanged");
}
