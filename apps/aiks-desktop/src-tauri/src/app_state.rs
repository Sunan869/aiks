/// Shared application state for the Tauri app.
use std::path::PathBuf;
use std::sync::Arc;

use aiks_core::AiksEngine;
use aiks_core::watcher::WatcherHandle;
use tokio::sync::Mutex;

/// State shared across all Tauri commands.
/// All fields must be Send + Sync since Tauri's State<T> requires T: Send + Sync.
pub struct AppState {
    /// None if SiYuan failed to start (control center still works)
    pub engine: Option<Arc<AiksEngine>>,
    /// The SiYuan base URL (None if not started)
    pub siyuan_url: Arc<Mutex<Option<String>>>,
    /// User data directory
    pub data_dir: PathBuf,
    /// B08: Watcher handle — wrapped in Mutex to satisfy Sync bound for Tauri State.
    /// Only needs to stay alive; never actually accessed after creation.
    pub _watcher_handle: Mutex<Option<WatcherHandle>>,
}

impl AppState {
    pub async fn siyuan_url(&self) -> Option<String> {
        self.siyuan_url.lock().await.clone()
    }

    pub fn engine(&self) -> Option<Arc<AiksEngine>> {
        self.engine.clone()
    }
}

/// Resolve the user data directory.
///
/// Delegates to `aiks_core`'s data-root resolution so the desktop app, the
/// CLI, and the state DB all agree on one root (default
/// `%LOCALAPPDATA%\AIKnowledgeSync`, relocatable via `AIKS_DATA_DIR` or a
/// `data-root.txt` pointer file).
pub fn data_dir() -> PathBuf {
    aiks_core::config::data_root()
}

/// Resolve the app config file path.
pub fn config_file_path() -> PathBuf {
    data_dir().join("config").join("aiks.toml")
}
