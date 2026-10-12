//! Shared drain gate for coordinated offline backup.
//! Every mutating operation must acquire a writer permit; backups obtain
//! exclusive quiescence only after all active writers leave.
use std::sync::{Arc, Condvar, Mutex};

#[derive(Default)]
pub struct BackupQuiescence {
    state: Mutex<GateState>,
    idle: Condvar,
}

#[derive(Default)]
struct GateState {
    writers: usize,
    paused: bool,
}

pub struct WriterPermit<'a> {
    gate: &'a BackupQuiescence,
}

pub struct OwnedWriterPermit {
    gate: Arc<BackupQuiescence>,
}

pub struct OwnedQuiescedGuard {
    gate: Arc<BackupQuiescence>,
}

pub struct QuiescedGuard<'a> {
    gate: &'a BackupQuiescence,
}

impl BackupQuiescence {
    pub fn try_begin_owned(gate: &Arc<Self>) -> Option<OwnedWriterPermit> {
        let mut state = gate.state.lock().ok()?;
        if state.paused {
            return None;
        }
        state.writers += 1;
        Some(OwnedWriterPermit {
            gate: Arc::clone(gate),
        })
    }

    /// Owns the gate so the drain remains active across async operations.
    /// Always call from a blocking thread; the wait is synchronous.
    pub fn pause_and_drain_owned(gate: &Arc<Self>) -> Result<OwnedQuiescedGuard, &'static str> {
        let mut state = gate.state.lock().map_err(|_| "backup gate poisoned")?;
        if state.paused {
            return Err("backup already in progress");
        }
        state.paused = true;
        while state.writers != 0 {
            state = gate.idle.wait(state).map_err(|_| "backup gate poisoned")?;
        }
        Ok(OwnedQuiescedGuard {
            gate: Arc::clone(gate),
        })
    }

    /// Bounded drain for backup orchestration. A timeout always unpauses
    /// the gate, even if a worker never returns from an external operation.
    pub fn pause_and_drain_owned_with_timeout(
        gate: &Arc<Self>,
        timeout: std::time::Duration,
    ) -> Result<OwnedQuiescedGuard, &'static str> {
        let mut state = gate.state.lock().map_err(|_| "backup gate poisoned")?;
        if state.paused {
            return Err("backup already in progress");
        }
        state.paused = true;
        let deadline = std::time::Instant::now() + timeout;
        while state.writers != 0 {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                state.paused = false;
                gate.idle.notify_all();
                return Err("backup drain timed out");
            }
            let result = gate.idle.wait_timeout(state, remaining);
            match result {
                Ok((next, _)) => state = next,
                Err(poisoned) => {
                    let (mut next, _) = poisoned.into_inner();
                    next.paused = false;
                    gate.idle.notify_all();
                    return Err("backup gate poisoned");
                }
            }
        }
        Ok(OwnedQuiescedGuard {
            gate: Arc::clone(gate),
        })
    }

    pub fn try_begin_write(&self) -> Option<WriterPermit<'_>> {
        let mut state = self.state.lock().ok()?;
        if state.paused {
            return None;
        }
        state.writers += 1;
        Some(WriterPermit { gate: self })
    }

    /// Blocking drain must run outside async event loops. Callers must hold
    /// the returned guard for the entire stopped-runtime snapshot operation.
    pub fn pause_and_drain(&self) -> Result<QuiescedGuard<'_>, &'static str> {
        let mut state = self.state.lock().map_err(|_| "backup gate poisoned")?;
        if state.paused {
            return Err("backup already in progress");
        }
        state.paused = true;
        while state.writers != 0 {
            state = self.idle.wait(state).map_err(|_| "backup gate poisoned")?;
        }
        Ok(QuiescedGuard { gate: self })
    }
}

impl Drop for OwnedWriterPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.gate.state.lock() {
            state.writers -= 1;
            if state.writers == 0 {
                self.gate.idle.notify_all();
            }
        }
    }
}

impl Drop for WriterPermit<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.gate.state.lock() {
            state.writers -= 1;
            if state.writers == 0 {
                self.gate.idle.notify_all();
            }
        }
    }
}

impl Drop for OwnedQuiescedGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.gate.state.lock() {
            state.paused = false;
            self.gate.idle.notify_all();
        }
    }
}

impl Drop for QuiescedGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.gate.state.lock() {
            state.paused = false;
            self.gate.idle.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn pause_drains_running_writes_and_blocks_new_writes() {
        let gate = Arc::new(BackupQuiescence::default());
        let writer = gate.try_begin_write().unwrap();
        let other = Arc::clone(&gate);
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let guard = other.pause_and_drain().unwrap();
            tx.send(other.try_begin_write().is_none()).unwrap();
            drop(guard);
        });
        std::thread::sleep(Duration::from_millis(20));
        assert!(rx.try_recv().is_err());
        drop(writer);
        assert!(rx.recv_timeout(Duration::from_secs(2)).unwrap());
        handle.join().unwrap();
        assert!(gate.try_begin_write().is_some());
    }

    #[test]
    fn owned_worker_permit_prevents_snapshot_until_long_job_finishes() {
        let gate = Arc::new(BackupQuiescence::default());
        let worker = BackupQuiescence::try_begin_owned(&gate).unwrap();
        let other = Arc::clone(&gate);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            let guard = other.pause_and_drain().unwrap();
            done_tx.send(other.try_begin_write().is_none()).unwrap();
            drop(guard);
        });
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        assert!(done_rx.try_recv().is_err());
        drop(worker);
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        handle.join().unwrap();
        assert!(BackupQuiescence::try_begin_owned(&gate).is_some());
    }

    #[test]
    fn owned_pause_drains_workers_and_recovers_on_guard_drop() {
        let gate = Arc::new(BackupQuiescence::default());
        let worker = BackupQuiescence::try_begin_owned(&gate).unwrap();
        let other = Arc::clone(&gate);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            let paused = BackupQuiescence::pause_and_drain_owned(&other).unwrap();
            done_tx.send(other.try_begin_write().is_none()).unwrap();
            drop(paused);
        });
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        assert!(done_rx.try_recv().is_err());
        drop(worker);
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        thread.join().unwrap();
        assert!(gate.try_begin_write().is_some());
    }

    #[test]
    fn drain_timeout_unpauses_and_preserves_active_writer() {
        let gate = Arc::new(BackupQuiescence::default());
        let worker = BackupQuiescence::try_begin_owned(&gate).unwrap();
        assert!(BackupQuiescence::pause_and_drain_owned_with_timeout(
            &gate,
            Duration::from_millis(10),
        )
        .is_err());
        assert!(gate.try_begin_write().is_some());
        drop(worker);
        let guard =
            BackupQuiescence::pause_and_drain_owned_with_timeout(&gate, Duration::from_secs(1))
                .unwrap();
        assert!(gate.try_begin_write().is_none());
        drop(guard);
        assert!(gate.try_begin_write().is_some());
    }

    #[test]
    fn concurrent_pause_rejected_and_drop_resumes() {
        let gate = BackupQuiescence::default();
        let guard = gate.pause_and_drain().unwrap();
        assert!(gate.pause_and_drain().is_err());
        assert!(gate.try_begin_write().is_none());
        drop(guard);
        assert!(gate.try_begin_write().is_some());
    }
}
