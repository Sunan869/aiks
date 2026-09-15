/// Desktop bootstrap: locates the embedded SiYuan runtime.
///
/// Runtime layout (spec §14, §20):
///   runtime_root/
///   ├── kernel/SiYuan-Kernel.exe
///   ├── stage/
///   ├── appearance/
///   └── guide/
///
/// Path resolution priority (spec §19):
///   1. Tauri resource_dir / resources / siyuan  (release & dev via Tauri)
///   2. src-tauri/resources/siyuan               (cargo MANIFEST_DIR fallback for dev)
///   3. <exe_dir>/resources/siyuan               (portable fallback)
use std::path::PathBuf;
use tauri::Manager;

/// Locate the SiYuan runtime root.
pub fn find_runtime_root(app_handle: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    // In Tauri 2, when bundling with ["resources/siyuan/**"], files are placed
    // relative to resource_dir at the same path: resource_dir/resources/siyuan/...
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        // Release path (NSIS installed): resource_dir/resources/siyuan/
        let candidate = resource_dir.join("resources").join("siyuan");
        if candidate.join("kernel").exists() {
            tracing::debug!("Runtime found via resource_dir: {}", candidate.display());
            return Ok(candidate);
        }
        // Also try resource_dir/siyuan/ (some Tauri versions strip the prefix)
        let candidate2 = resource_dir.join("siyuan");
        if candidate2.join("kernel").exists() {
            tracing::debug!(
                "Runtime found via resource_dir/siyuan: {}",
                candidate2.display()
            );
            return Ok(candidate2);
        }
    }

    // Dev fallback: look relative to src-tauri/resources/siyuan (CARGO_MANIFEST_DIR)
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

    // Executable-relative fallback
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
        "SiYuan runtime not found. Run: powershell -File scripts\\setup-siyuan.ps1\n\
         Checked:\n  \
         - Tauri resource_dir/resources/siyuan\n  \
         - {}/resources/siyuan\n  \
         - <exe_dir>/resources/siyuan",
        env!("CARGO_MANIFEST_DIR")
    )
}
