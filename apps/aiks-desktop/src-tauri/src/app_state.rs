/// Shared application state for the Tauri app.
use std::path::PathBuf;
use std::sync::Arc;

use aiks_core::AiksEngine;
use tokio::sync::Mutex;

/// State shared across all Tauri commands.
pub struct AppState {
    /// None if SiYuan failed to start (control center still works)
    pub engine: Option<Arc<AiksEngine>>,
    /// The SiYuan base URL (None if not started)
    pub siyuan_url: Arc<Mutex<Option<String>>>,
    /// User data directory
    pub data_dir: PathBuf,
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
/// Spec §10, §23: %LOCALAPPDATA%\AIKnowledgeSync on Windows.
pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("AIKnowledgeSync")
}
