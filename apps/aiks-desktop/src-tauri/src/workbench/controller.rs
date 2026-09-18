use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, MutexGuard,
};

use serde::Serialize;
use tauri::Url;
use uuid::Uuid;

use super::protocol::{validate_loopback_origin, WorkspaceMode, BRIDGE_PROTOCOL_VERSION};

#[derive(Debug, Clone, Serialize)]
pub struct WorkbenchStatus {
    pub available: bool,
    pub mounted: bool,
    pub ready: bool,
    pub mode: WorkspaceMode,
    pub origin: Option<String>,
    pub protocol_version: u16,
}

pub struct WorkbenchController {
    origin: Mutex<Option<Url>>,
    nonce: String,
    mounted: AtomicBool,
    ready: AtomicBool,
    mode: Mutex<WorkspaceMode>,
}

impl Default for WorkbenchController {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkbenchController {
    pub fn new() -> Self {
        Self {
            origin: Mutex::new(None),
            nonce: Uuid::new_v4().to_string(),
            mounted: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            mode: Mutex::new(WorkspaceMode::Knowledge),
        }
    }

    pub fn nonce(&self) -> &str {
        &self.nonce
    }

    pub fn origin(&self) -> Option<Url> {
        lock_recover(&self.origin).clone()
    }

    pub fn set_origin(&self, origin: &str) -> anyhow::Result<()> {
        let origin = validate_loopback_origin(origin)?;
        let mut current = lock_recover(&self.origin);
        if current.as_ref() != Some(&origin) {
            *current = Some(origin);
            self.mounted.store(false, Ordering::Release);
            self.ready.store(false, Ordering::Release);
        }
        Ok(())
    }

    pub fn clear_origin(&self) {
        *lock_recover(&self.origin) = None;
        self.mounted.store(false, Ordering::Release);
        self.ready.store(false, Ordering::Release);
    }

    pub fn set_mounted(&self, mounted: bool) {
        self.mounted.store(mounted, Ordering::Release);
        if !mounted {
            self.ready.store(false, Ordering::Release);
        }
    }

    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::Release);
    }

    pub fn set_mode(&self, mode: WorkspaceMode) {
        *lock_recover(&self.mode) = mode;
    }

    pub fn status(&self) -> WorkbenchStatus {
        let origin = self.origin();
        WorkbenchStatus {
            available: origin.is_some(),
            mounted: self.mounted.load(Ordering::Acquire),
            ready: self.ready.load(Ordering::Acquire),
            mode: *lock_recover(&self.mode),
            origin: origin.map(|url| url.to_string()),
            protocol_version: BRIDGE_PROTOCOL_VERSION,
        }
    }
}

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::protocol::WorkspaceMode;

    #[test]
    fn controller_starts_unavailable_and_not_ready() {
        let controller = WorkbenchController::new();
        let status = controller.status();
        assert!(!status.available);
        assert!(!status.mounted);
        assert!(!status.ready);
        assert_eq!(status.mode, WorkspaceMode::Knowledge);
        assert!(status.origin.is_none());
        assert!(!controller.nonce().is_empty());
    }

    #[test]
    fn controller_accepts_only_loopback_origin() {
        let controller = WorkbenchController::new();
        controller.set_origin("http://127.0.0.1:6806").unwrap();
        assert_eq!(
            controller.status().origin.as_deref(),
            Some("http://127.0.0.1:6806/")
        );
        assert!(controller.set_origin("https://example.com").is_err());
    }

    #[test]
    fn controller_tracks_ready_and_workspace_mode() {
        let controller = WorkbenchController::new();
        controller.set_origin("http://localhost:6806").unwrap();
        controller.set_mounted(true);
        controller.set_ready(true);
        controller.set_mode(WorkspaceMode::Session);

        let status = controller.status();
        assert!(status.available);
        assert!(status.mounted);
        assert!(status.ready);
        assert_eq!(status.mode, WorkspaceMode::Session);
    }
}
