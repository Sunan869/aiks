use aiks_core::providers::local_io::{ReadLimits, ScopedReader};
use std::path::Path;

#[test]
fn jsonl_reports_partial_tail_and_never_reads_parent_or_absolute_paths() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("events.jsonl"),
        b"{\"ok\":true}\n{\"type\":",
    )
    .unwrap();
    let reader = ScopedReader::new(root.path().to_path_buf(), ReadLimits::default()).unwrap();
    let mut count = 0;
    let report = reader
        .for_each_jsonl(Path::new("events.jsonl"), |_, _| {
            count += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(count, 1);
    assert!(!report.complete);
    assert!(report.partial_tail);
    assert_eq!(report.malformed_lines, 0);
    assert!(reader.checked_path(Path::new("../events.jsonl")).is_err());
    assert!(reader
        .checked_path(&root.path().join("events.jsonl"))
        .is_err());
}

#[test]
fn malformed_middle_and_line_budget_are_explicit() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("events.jsonl"), b"bad\n{\"ok\":true}\n").unwrap();
    let reader = ScopedReader::new(root.path().to_path_buf(), ReadLimits::default()).unwrap();
    let report = reader
        .for_each_jsonl(Path::new("events.jsonl"), |_, _| Ok(()))
        .unwrap();
    assert!(!report.complete);
    assert!(!report.partial_tail);
    assert_eq!(report.malformed_lines, 1);
    let limits = ReadLimits {
        max_line_bytes: 4,
        ..ReadLimits::default()
    };
    let reader = ScopedReader::new(root.path().to_path_buf(), limits).unwrap();
    assert!(reader
        .for_each_jsonl(Path::new("events.jsonl"), |_, _| Ok(()))
        .is_err());
}

#[test]
fn valid_last_line_without_newline_is_complete_and_utf8_bom_is_supported() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("events.jsonl"),
        "\u{feff}{\"ok\":\"中文\"}",
    )
    .unwrap();
    let reader = ScopedReader::new(root.path().to_path_buf(), ReadLimits::default()).unwrap();
    let report = reader
        .for_each_jsonl(Path::new("events.jsonl"), |_, _| Ok(()))
        .unwrap();
    assert!(report.complete);
}

#[cfg(unix)]
#[test]
fn symlinked_root_leaf_and_parent_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    std::fs::write(other.path().join("private.json"), "{}").unwrap();
    std::os::unix::fs::symlink(other.path(), root.path().join("linked")).unwrap();
    std::os::unix::fs::symlink(
        other.path().join("private.json"),
        root.path().join("leaf.json"),
    )
    .unwrap();
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
    let value: String = conn
        .query_row("SELECT value FROM data", [], |row| row.get(0))
        .unwrap();
    assert_eq!(value, "visible");
    assert!(conn.execute("DELETE FROM data", []).is_err());
}

#[test]
fn repeat_scans_of_large_jsonl_are_readonly_and_report_baseline() {
    use std::io::Write;
    use std::time::Instant;

    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("large.jsonl");
    let mut output = std::fs::File::create(&path).unwrap();
    let count = 12_000usize;
    for i in 0..count {
        writeln!(
            output,
            "{{\"index\":{i},\"message\":\"AIKS provider fixture\"}}"
        )
        .unwrap();
    }
    output.sync_all().unwrap();
    drop(output);
    let bytes_before = std::fs::read(&path).unwrap();
    let reader = ScopedReader::new(root.path().to_path_buf(), ReadLimits::default()).unwrap();

    let start = Instant::now();
    let mut first_count = 0usize;
    let first = reader
        .for_each_jsonl(Path::new("large.jsonl"), |_, event| {
            assert!(event["index"].is_number());
            first_count += 1;
            Ok(())
        })
        .unwrap();
    let initial_ms = start.elapsed().as_millis();

    let repeat_start = Instant::now();
    let mut repeat_count = 0usize;
    let second = reader
        .for_each_jsonl(Path::new("large.jsonl"), |_, _| {
            repeat_count += 1;
            Ok(())
        })
        .unwrap();
    let repeat_ms = repeat_start.elapsed().as_millis();

    assert!(first.complete && second.complete);
    assert_eq!(first_count, count);
    assert_eq!(repeat_count, count);
    assert_eq!(std::fs::read(&path).unwrap(), bytes_before);
    eprintln!(
        "AIKS_PROVIDER_BASELINE jsonl_entries={count} bytes={} first_scan_ms={initial_ms} repeat_scan_ms={repeat_ms}",
        bytes_before.len()
    );
}

#[test]
fn incomplete_append_is_not_a_successful_provider_transcript() {
    use std::io::Write;

    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("stream.jsonl");
    std::fs::write(&path, b"{\"message\":\"first\"}\n{\"message\":").unwrap();
    let reader = ScopedReader::new(root.path().to_path_buf(), ReadLimits::default()).unwrap();
    let first = reader
        .for_each_jsonl(Path::new("stream.jsonl"), |_, _| Ok(()))
        .unwrap();
    assert!(first.partial_tail);
    assert!(!first.complete);
    assert!(reader.jsonl(Path::new("stream.jsonl")).is_err());

    let mut output = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    output.write_all(b"\"second\"}\n").unwrap();
    output.sync_all().unwrap();
    drop(output);

    let events = reader.jsonl(Path::new("stream.jsonl")).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].1["message"], "first");
    assert_eq!(events[1].1["message"], "second");
}

#[test]
fn oversized_jsonl_line_fails_before_parsing_or_unbounded_allocation() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("huge.jsonl"),
        b"{\"message\":\"1234567890\"}\n",
    )
    .unwrap();
    let reader = ScopedReader::new(
        root.path().to_path_buf(),
        ReadLimits {
            max_line_bytes: 12,
            ..ReadLimits::default()
        },
    )
    .unwrap();
    let error = reader
        .for_each_jsonl(Path::new("huge.jsonl"), |_, _| Ok(()))
        .unwrap_err();
    assert!(error.to_string().contains("byte budget exceeded"));
}
