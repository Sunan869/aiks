use aiks_core::providers::local_io::{ReadLimits, ScopedReader};
use std::path::Path;

#[test]
fn jsonl_reports_partial_tail_and_never_reads_parent_or_absolute_paths() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("events.jsonl"), b"{\"ok\":true}\n{\"type\":").unwrap();
    let reader = ScopedReader::new(root.path().to_path_buf(), ReadLimits::default()).unwrap();
    let mut count = 0;
    let report = reader.for_each_jsonl(Path::new("events.jsonl"), |_, _| { count += 1; Ok(()) }).unwrap();
    assert_eq!(count, 1);
    assert!(!report.complete);
    assert!(report.partial_tail);
    assert_eq!(report.malformed_lines, 0);
    assert!(reader.checked_path(Path::new("../events.jsonl")).is_err());
    assert!(reader.checked_path(&root.path().join("events.jsonl")).is_err());
}

#[test]
fn malformed_middle_and_line_budget_are_explicit() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("events.jsonl"), b"bad\n{\"ok\":true}\n").unwrap();
    let reader = ScopedReader::new(root.path().to_path_buf(), ReadLimits::default()).unwrap();
    let report = reader.for_each_jsonl(Path::new("events.jsonl"), |_, _| Ok(())).unwrap();
    assert!(!report.complete);
    assert!(!report.partial_tail);
    assert_eq!(report.malformed_lines, 1);
    let limits = ReadLimits { max_line_bytes: 4, ..ReadLimits::default() };
    let reader = ScopedReader::new(root.path().to_path_buf(), limits).unwrap();
    assert!(reader.for_each_jsonl(Path::new("events.jsonl"), |_, _| Ok(())).is_err());
}

#[test]
fn valid_last_line_without_newline_is_complete_and_utf8_bom_is_supported() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("events.jsonl"), "\u{feff}{\"ok\":\"中文\"}").unwrap();
    let reader = ScopedReader::new(root.path().to_path_buf(), ReadLimits::default()).unwrap();
    let report = reader.for_each_jsonl(Path::new("events.jsonl"), |_, _| Ok(())).unwrap();
    assert!(report.complete);
}

#[cfg(unix)]
#[test]
fn symlinked_root_leaf_and_parent_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    std::fs::write(other.path().join("private.json"), "{}").unwrap();
    std::os::unix::fs::symlink(other.path(), root.path().join("linked")).unwrap();
    std::os::unix::fs::symlink(other.path().join("private.json"), root.path().join("leaf.json")).unwrap();
    let reader = ScopedReader::new(root.path().to_path_buf(), ReadLimits::default()).unwrap();
    assert!(reader.read_json(Path::new("linked/private.json")).is_err());
    assert!(reader.read_json(Path::new("leaf.json")).is_err());
    assert!(ScopedReader::new(root.path().join("linked"), ReadLimits::default()).is_err());
}

#[test]
fn readonly_sqlite_sees_uncheckpointed_wal() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("state.db");
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE data(value TEXT); INSERT INTO data VALUES('visible');").unwrap();
    let reader = ScopedReader::new(root.path().to_path_buf(), ReadLimits::default()).unwrap();
    let conn = reader.open_readonly(Path::new("state.db")).unwrap();
    let value: String = conn.query_row("SELECT value FROM data", [], |row| row.get(0)).unwrap();
    assert_eq!(value, "visible");
    assert!(conn.execute("DELETE FROM data", []).is_err());
}
