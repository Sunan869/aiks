//! Filesystem access shared by the external providers. No source writes.
use std::fs::{self, File, Metadata};
use std::io::{BufRead, BufReader, Read};
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub struct ReadLimits {
    pub max_line_bytes: usize,
    pub max_file_bytes: u64,
    pub max_entries: usize,
    pub max_depth: usize,
}
impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            max_line_bytes: 8 * 1024 * 1024,
            max_file_bytes: 256 * 1024 * 1024,
            max_entries: 100_000,
            max_depth: 16,
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct ReadReport {
    pub complete: bool,
    pub malformed_lines: usize,
    pub partial_tail: bool,
}
#[derive(Debug, Clone)]
pub struct ScopedReader {
    root: PathBuf,
    limits: ReadLimits,
}

fn is_link(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_type().is_symlink() || meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if a.dev() != b.dev() || a.ino() != b.ino() {
            return false;
        }
    }
    a.len() == b.len() && a.modified().ok() == b.modified().ok()
}

impl ScopedReader {
    pub fn new(root: PathBuf, limits: ReadLimits) -> Result<Self> {
        let metadata = fs::symlink_metadata(&root).context("provider root is not accessible")?;
        ensure!(
            metadata.is_dir() && !is_link(&metadata),
            "provider root must be a real directory"
        );
        Ok(Self {
            root: root.canonicalize()?,
            limits,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn limits(&self) -> ReadLimits {
        self.limits
    }

    /// Resolve a relative path, rejecting traversal, links and reparse points.
    /// This is containment validation, not OS-level isolation from a hostile writer.
    pub fn checked_path(&self, relative: &Path) -> Result<PathBuf> {
        ensure!(
            !relative.is_absolute(),
            "absolute provider paths are not allowed here"
        );
        let root_meta = fs::symlink_metadata(&self.root)?;
        ensure!(
            root_meta.is_dir() && !is_link(&root_meta),
            "provider root was replaced"
        );
        let mut path = self.root.clone();
        for component in relative.components() {
            match component {
                Component::Normal(name) => path.push(name),
                Component::CurDir => continue,
                _ => bail!("provider path traversal is not allowed"),
            }
            let meta = fs::symlink_metadata(&path)?;
            ensure!(
                !is_link(&meta),
                "provider links and reparse points are not followed"
            );
        }
        let path = path.canonicalize()?;
        ensure!(
            path.starts_with(&self.root),
            "provider path escaped its root"
        );
        Ok(path)
    }
    pub fn relative(&self, absolute: &Path) -> Result<PathBuf> {
        let relative = absolute
            .strip_prefix(&self.root)
            .context("source path outside provider root")?;
        self.checked_path(relative)?;
        Ok(relative.to_path_buf())
    }
    pub fn exists(&self, relative: &Path) -> Result<bool> {
        match self.checked_path(relative) {
            Ok(_) => Ok(true),
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
    fn open_file(&self, relative: &Path) -> Result<(File, Metadata)> {
        let path = self.checked_path(relative)?;
        let before = fs::symlink_metadata(&path)?;
        ensure!(
            before.is_file() && !is_link(&before),
            "provider input must be a regular file"
        );
        ensure!(
            before.len() <= self.limits.max_file_bytes,
            "provider file byte budget exceeded"
        );
        let file = File::open(path)?;
        ensure!(
            same_file(&before, &file.metadata()?),
            "provider input changed while opening"
        );
        Ok((file, before))
    }
    fn unchanged(&self, relative: &Path, before: &Metadata) -> Result<bool> {
        let path = self.checked_path(relative)?;
        Ok(same_file(before, &fs::metadata(path)?))
    }
    pub fn read_text(&self, relative: &Path) -> Result<String> {
        let (file, before) = self.open_file(relative)?;
        let mut bytes = Vec::new();
        file.take(self.limits.max_file_bytes + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= self.limits.max_file_bytes,
            "provider file byte budget exceeded"
        );
        ensure!(
            self.unchanged(relative, &before)?,
            "provider input changed during read; retry later"
        );
        String::from_utf8(bytes).context("provider input is not UTF-8")
    }
    pub fn read_json(&self, relative: &Path) -> Result<Value> {
        let text = self.read_text(relative)?;
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| {
            anyhow::anyhow!(
                "invalid provider JSON at line {} column {}",
                e.line(),
                e.column()
            )
        })
    }
    pub fn for_each_jsonl(
        &self,
        relative: &Path,
        mut visit: impl FnMut(usize, Value) -> Result<()>,
    ) -> Result<ReadReport> {
        let (file, before) = self.open_file(relative)?;
        let mut reader = BufReader::new(file);
        let mut report = ReadReport {
            complete: true,
            ..Default::default()
        };
        let mut total = 0_u64;
        let mut index = 0;
        loop {
            // Take limits allocation before read_until, not after an unbounded read_line.
            let mut bytes = Vec::new();
            let count = reader
                .by_ref()
                .take(self.limits.max_line_bytes as u64 + 1)
                .read_until(b'\n', &mut bytes)?;
            if count == 0 {
                break;
            }
            ensure!(
                count <= self.limits.max_line_bytes,
                "provider JSONL line byte budget exceeded"
            );
            total += count as u64;
            ensure!(
                total <= self.limits.max_file_bytes,
                "provider JSONL file byte budget exceeded"
            );
            let newline = bytes.last() == Some(&b'\n');
            let line = std::str::from_utf8(&bytes).ok().map(|line| {
                if index == 0 {
                    line.trim_start_matches('\u{feff}').trim()
                } else {
                    line.trim()
                }
            });
            if let Some("") = line {
                index += 1;
                continue;
            }
            match line.and_then(|line| serde_json::from_str::<Value>(line).ok()) {
                Some(value) => visit(index, value)?,
                None => {
                    report.complete = false;
                    if !newline && reader.fill_buf()?.is_empty() {
                        report.partial_tail = true;
                    } else {
                        report.malformed_lines += 1;
                    }
                }
            }
            index += 1;
        }
        report.complete &= self.unchanged(relative, &before)?;
        Ok(report)
    }
    pub fn jsonl(&self, relative: &Path) -> Result<Vec<(usize, Value)>> {
        let mut events = Vec::new();
        let report = self.for_each_jsonl(relative, |index, event| {
            events.push((index, event));
            Ok(())
        })?;
        ensure!(report.complete, "incomplete provider transcript (malformed={}, partial_tail={}); retry after source write completes", report.malformed_lines, report.partial_tail);
        Ok(events)
    }
    pub fn open_readonly(&self, relative: &Path) -> Result<Connection> {
        let path = self.checked_path(relative)?;
        ensure!(
            fs::metadata(&path)?.is_file(),
            "provider database is not a regular file"
        );
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        // A short read transaction includes WAL state; immutable mode would not.
        conn.execute_batch("BEGIN DEFERRED TRANSACTION")?;
        Ok(conn)
    }
    /// List only this directory; no implicit traversal of user's home.
    pub fn children(&self, relative: &Path) -> Result<Vec<PathBuf>> {
        if !self.exists(relative)? {
            return Ok(Vec::new());
        }
        let path = self.checked_path(relative)?;
        ensure!(path.is_dir(), "provider directory expected");
        let mut children = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            ensure!(
                children.len() < self.limits.max_entries,
                "provider entry budget exceeded"
            );
            let rel = relative.join(entry.file_name());
            self.checked_path(&rel)?;
            children.push(rel);
        }
        children.sort();
        Ok(children)
    }
    pub fn walk_files(&self, relative: &Path, depth: usize) -> Result<Vec<PathBuf>> {
        ensure!(
            depth <= self.limits.max_depth,
            "provider traversal depth exceeded"
        );
        let mut pending = vec![(relative.to_path_buf(), 0_usize)];
        let mut files = Vec::new();
        let mut visited = 0;
        while let Some((dir, level)) = pending.pop() {
            for path in self.children(&dir)? {
                visited += 1;
                ensure!(
                    visited <= self.limits.max_entries,
                    "provider traversal entry budget exceeded"
                );
                let meta = fs::metadata(self.checked_path(&path)?)?;
                if meta.is_file() {
                    files.push(path);
                } else if meta.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if matches!(
                        name,
                        ".git" | "node_modules" | "target" | ".venv" | "__pycache__"
                    ) {
                        continue;
                    }
                    ensure!(
                        level < depth,
                        "provider traversal incomplete at depth limit"
                    );
                    pending.push((path, level + 1));
                }
            }
        }
        files.sort();
        Ok(files)
    }
}
