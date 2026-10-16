//! Size-bounded logging; avoid unbounded aiks.log growth across long-lived sessions.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const ARCHIVES: usize = 5;

struct State {
    path: PathBuf,
    file: Option<File>,
    bytes: u64,
}

impl State {
    fn rotate(&mut self) -> io::Result<()> {
        self.file.take();
        for index in (1..ARCHIVES).rev() {
            let from = self.path.with_extension(format!("log.{index}"));
            let to = self.path.with_extension(format!("log.{}", index + 1));
            if from.exists() {
                if to.exists() {
                    fs::remove_file(&to)?;
                }
                fs::rename(from, to)?;
            }
        }
        let first = self.path.with_extension("log.1");
        if first.exists() {
            fs::remove_file(&first)?;
        }
        if self.path.exists() {
            fs::rename(&self.path, first)?;
        }
        self.file = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?,
        );
        self.bytes = 0;
        Ok(())
    }
}

#[derive(Clone)]
pub struct RotatingWriter(Arc<Mutex<State>>);

impl Write for RotatingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| io::Error::other("log lock poisoned"))?;
        if state.bytes.saturating_add(buf.len() as u64) > MAX_BYTES && state.bytes > 0 {
            state.rotate()?;
        }
        let n = state
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("log file closed"))?
            .write(buf)?;
        state.bytes += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| io::Error::other("log lock poisoned"))?;
        state
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("log file closed"))?
            .flush()
    }
}

pub fn make_writer(
    dir: &Path,
) -> io::Result<impl Fn() -> RotatingWriter + Clone + Send + Sync + 'static> {
    fs::create_dir_all(dir)?;
    let path = dir.join("aiks.log");
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    let bytes = file.metadata()?.len();
    let state = Arc::new(Mutex::new(State {
        path,
        file: Some(file),
        bytes,
    }));
    if bytes >= MAX_BYTES {
        state
            .lock()
            .map_err(|_| io::Error::other("log lock poisoned"))?
            .rotate()?;
    }
    Ok(move || RotatingWriter(Arc::clone(&state)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_oversized_log_is_archived_on_initialization() {
        let dir = std::env::temp_dir().join(format!("aiks-log-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("aiks.log");
        let file = File::create(&path).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        drop(file);
        let factory = make_writer(&dir).unwrap();
        let mut w = factory();
        w.write_all(b"hello").unwrap();
        assert!(path.metadata().unwrap().len() < MAX_BYTES);
        assert!(dir.join("aiks.log.1").exists());
        drop(w);
        drop(factory);
        fs::remove_dir_all(dir).unwrap();
    }
}
