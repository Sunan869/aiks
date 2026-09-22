use crate::app_state::data_dir;
use crate::workbench::plugin::install_bridge_plugin;
/// Desktop bootstrap: locates the embedded SiYuan runtime.
use std::path::PathBuf;
use tauri::Manager;

/// Legacy mode prepares its bridge in the existing personal workspace.
pub fn find_runtime_root(app_handle: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    locate_runtime_root(app_handle).map(prepare_embedded_runtime)
}

/// Read-only location for the independent Service; never edits the legacy workspace.
pub fn locate_runtime_root(app_handle: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        let candidate = resource_dir.join("resources").join("siyuan");
        if candidate.join("kernel").exists() {
            tracing::debug!("Runtime found via resource_dir: {}", candidate.display());
            return Ok(candidate);
        }
        let candidate2 = resource_dir.join("siyuan");
        if candidate2.join("kernel").exists() {
            tracing::debug!(
                "Runtime found via resource_dir/siyuan: {}",
                candidate2.display()
            );
            return Ok(candidate2);
        }
    }
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("siyuan");
    if dev_path.join("kernel").exists() {
        tracing::debug!(
            "Runtime found via CARGO_MANIFEST_DIR: {}",
            dev_path.display()
        );
        return Ok(dev_path);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            for rel in &["resources/siyuan", "siyuan"] {
                let candidate = exe_dir.join(rel);
                if candidate.join("kernel").exists() {
                    tracing::debug!("Runtime found relative to exe: {}", candidate.display());
                    return Ok(candidate);
                }
            }
        }
    }
    anyhow::bail!(
        "SiYuan runtime not found. Prepare it with scripts/setup-siyuan.ps1 on Windows \
         or scripts/setup-siyuan.sh on macOS/Linux.\n\
         Checked:\n  \
         - Tauri resource_dir/resources/siyuan\n  \
         - {}/resources/siyuan\n  \
         - <exe_dir>/resources/siyuan",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn prepare_embedded_runtime(runtime_root: PathBuf) -> PathBuf {
    let source = runtime_root
        .join("data")
        .join("plugins")
        .join("aiks-bridge");
    let workspace = data_dir().join("siyuan").join("workspace");
    match install_bridge_plugin(&source, &workspace) {
        Ok(target) => tracing::debug!(
            "AIKS bridge plugin prepared: {} -> {}",
            source.display(),
            target.display()
        ),
        Err(error) => tracing::warn!(
            "AIKS bridge plugin could not be prepared from {}: {}",
            source.display(),
            error
        ),
    }
    runtime_root
}
