use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{atomic::AtomicBool, Arc};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

use crate::app_state::AppState;

const POINTER_FILE: &str = "data-root.txt";
const PENDING_FILE: &str = ".aiks-data-migration.json";
const STATUS_FILE: &str = ".aiks-data-migration-status.txt";
const MIGRATION_MARKER: &str = ".aiks-migration-in-progress";

#[derive(Debug, Clone, Serialize)]
pub struct DataStorageSettings {
    pub current_root: String,
    pub default_root: String,
    pub setup_required: bool,
    pub custom: bool,
    pub env_override: bool,
    pub migration_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingMigration {
    source: String,
    target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TreeStats {
    files: u64,
    bytes: u64,
}

pub async fn prepare_storage_before_startup(app: &AppHandle) -> anyhow::Result<bool> {
    let default_root = default_data_root();
    fs::create_dir_all(&default_root)?;

    if pending_path().exists() {
        if let Some(window) = app.get_webview_window("control") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        let _ = app.emit(
            "startup-progress",
            serde_json::json!({
                "step": "data_migration",
                "message": "正在迁移 AIKS 数据目录，完成后将继续启动..."
            }),
        );

        let migration = tokio::task::spawn_blocking(apply_pending_migration)
            .await
            .map_err(|e| anyhow::anyhow!("Data migration task failed: {e}"))?;
        if let Err(error) = migration {
            tracing::error!(error = %error, "Data directory migration failed; keeping previous data root");
            let _ = write_status(&format!("数据目录迁移失败：{error}"));
            let _ = fs::remove_file(pending_path());
        }
    }

    if requires_initial_setup() {
        let state = AppState {
            engine: None,
            siyuan_url: Arc::new(Mutex::new(None)),
            data_dir: default_root,
            close_to_tray: AtomicBool::new(true),
            _watcher_handle: Mutex::new(None),
        };
        app.manage(state);
        if let Some(window) = app.get_webview_window("control") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        let _ = app.emit(
            "startup-progress",
            serde_json::json!({
                "step": "storage_setup",
                "message": "请选择 AIKS 数据存储位置"
            }),
        );
        return Ok(false);
    }

    // Existing users that already have data in the historical default directory
    // should not be interrupted after upgrading. Persist the default as their
    // explicit choice so future launches can distinguish them from a fresh install.
    if env_override().is_none() && !pointer_path().exists() && has_existing_aiks_data(&default_data_root()) {
        if let Err(error) = write_pointer(&default_data_root()) {
            tracing::warn!(error = %error, "Could not persist legacy default data root selection");
        }
    }

    Ok(true)
}

#[tauri::command]
pub fn get_data_storage_settings() -> Result<DataStorageSettings, String> {
    let current = aiks_core::config::data_root();
    let default = default_data_root();
    Ok(DataStorageSettings {
        current_root: current.to_string_lossy().to_string(),
        default_root: default.to_string_lossy().to_string(),
        setup_required: requires_initial_setup(),
        custom: !same_path(&current, &default),
        env_override: env_override().is_some(),
        migration_note: fs::read_to_string(status_path()).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
    })
}

#[tauri::command]
pub fn pick_data_directory() -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = '选择 AIKS 数据存储目录，例如 E:\AIKS-Data'
$dialog.ShowNewFolderButton = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    Write-Output $dialog.SelectedPath
}
"#;
        let mut command = Command::new("powershell.exe");
        command.creation_flags(CREATE_NO_WINDOW);
        let output = command
            .args(["-NoProfile", "-STA", "-Command", script])
            .output()
            .map_err(|e| format!("无法打开目录选择器：{e}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok((!selected.is_empty()).then_some(selected));
    }

    #[cfg(not(windows))]
    {
        Err("当前版本仅在 Windows 上提供图形化目录选择器".to_string())
    }
}

#[tauri::command]
pub async fn set_data_storage_root(app: AppHandle, target_path: String) -> Result<(), String> {
    if env_override().is_some() {
        return Err("当前数据目录由 AIKS_DATA_DIR 环境变量控制，请先移除该环境变量再修改。".to_string());
    }

    let target_text = target_path.trim();
    if target_text.is_empty() {
        return Err("数据目录不能为空".to_string());
    }
    let target = PathBuf::from(target_text);
    if !target.is_absolute() {
        return Err("请选择绝对路径作为数据目录".to_string());
    }
    fs::create_dir_all(&target).map_err(|e| format!("无法创建目标目录：{e}"))?;

    let current = aiks_core::config::data_root();
    validate_target(&current, &target)?;

    if same_path(&current, &target) {
        write_pointer(&target)?;
        let _ = fs::remove_file(status_path());
        crate::lifecycle::shutdown(&app).await;
        app.restart()
    }

    if requires_initial_setup() {
        ensure_effectively_empty(&target)?;
        write_pointer(&target)?;
        write_status(&format!("数据目录已设置为 {}", target.display()))?;
        crate::lifecycle::shutdown(&app).await;
        app.restart()
    }

    ensure_effectively_empty_or_recoverable(&target)?;
    let pending = PendingMigration {
        source: current.to_string_lossy().to_string(),
        target: target.to_string_lossy().to_string(),
    };
    let json = serde_json::to_string_pretty(&pending).map_err(|e| e.to_string())?;
    fs::write(pending_path(), json).map_err(|e| format!("无法登记数据迁移任务：{e}"))?;
    let _ = fs::remove_file(status_path());

    crate::lifecycle::shutdown(&app).await;
    app.restart()
}

fn apply_pending_migration() -> Result<(), String> {
    if env_override().is_some() {
        return Err("检测到 AIKS_DATA_DIR 环境变量，已取消待执行的数据目录迁移".to_string());
    }

    let raw = fs::read_to_string(pending_path()).map_err(|e| format!("读取迁移任务失败：{e}"))?;
    let pending: PendingMigration = serde_json::from_str(&raw).map_err(|e| format!("迁移任务格式错误：{e}"))?;
    let source = PathBuf::from(&pending.source);
    let target = PathBuf::from(&pending.target);

    if !source.exists() {
        return Err(format!("原数据目录不存在：{}", source.display()));
    }
    validate_target(&source, &target)?;
    prepare_destination(&target)?;

    let source_stats = measure_tree(&source)?;
    copy_tree(&source, &target)?;
    let target_stats = measure_tree(&target)?;
    if source_stats != target_stats {
        return Err(format!(
            "迁移校验失败：原目录 {} 个文件/{} 字节，目标目录 {} 个文件/{} 字节",
            source_stats.files, source_stats.bytes, target_stats.files, target_stats.bytes
        ));
    }
    validate_sqlite_copy(&target)?;

    write_pointer(&target)?;
    let _ = fs::remove_file(target.join(MIGRATION_MARKER));
    let _ = fs::remove_file(pending_path());
    write_status(&format!(
        "数据已迁移到 {}。原目录 {} 暂时保留作为安全备份；如果原目录是默认控制目录，请保留其中的 {}。",
        target.display(),
        source.display(),
        POINTER_FILE
    ))?;
    Ok(())
}

fn validate_sqlite_copy(target: &Path) -> Result<(), String> {
    let db_path = target.join("aiks.db");
    if !db_path.exists() {
        return Ok(());
    }
    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| format!("无法打开迁移后的数据库进行校验：{e}"))?;
    let check: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|e| format!("数据库 quick_check 失败：{e}"))?;
    if check.eq_ignore_ascii_case("ok") {
        Ok(())
    } else {
        Err(format!("迁移后的数据库校验未通过：{check}"))
    }
}

fn prepare_destination(target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|e| format!("无法创建目标目录：{e}"))?;
    let marker = target.join(MIGRATION_MARKER);
    if marker.exists() {
        clear_non_control_entries(target)?;
    }
    ensure_effectively_empty(target)?;
    fs::write(marker, b"AIKS data migration in progress")
        .map_err(|e| format!("无法创建迁移标记：{e}"))?;
    Ok(())
}

fn clear_non_control_entries(root: &Path) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if is_control_name(&entry.file_name()) {
            continue;
        }
        let path = entry.path();
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| format!("清理未完成迁移目录失败 {}：{e}", path.display()))?;
        } else {
            fs::remove_file(&path).map_err(|e| format!("清理未完成迁移文件失败 {}：{e}", path.display()))?;
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    copy_dir(source, target, true)
}

fn copy_dir(source: &Path, target: &Path, root_level: bool) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|e| format!("创建目录失败 {}：{e}", target.display()))?;
    for entry in fs::read_dir(source).map_err(|e| format!("读取目录失败 {}：{e}", source.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if root_level && is_control_name(&entry.file_name()) {
            continue;
        }
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        if file_type.is_symlink() {
            return Err(format!("数据目录包含符号链接，已停止迁移：{}", entry.path().display()));
        }
        let destination = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir(&entry.path(), &destination, false)?;
        } else if file_type.is_file() {
            let expected = entry.metadata().map_err(|e| e.to_string())?.len();
            let copied = fs::copy(entry.path(), &destination)
                .map_err(|e| format!("复制文件失败 {}：{e}", entry.path().display()))?;
            if copied != expected {
                return Err(format!("文件复制长度不一致：{}", entry.path().display()));
            }
        }
    }
    Ok(())
}

fn measure_tree(root: &Path) -> Result<TreeStats, String> {
    measure_dir(root, true)
}

fn measure_dir(root: &Path, root_level: bool) -> Result<TreeStats, String> {
    let mut stats = TreeStats { files: 0, bytes: 0 };
    for entry in fs::read_dir(root).map_err(|e| format!("读取目录失败 {}：{e}", root.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if root_level && is_control_name(&entry.file_name()) {
            continue;
        }
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        if file_type.is_symlink() {
            return Err(format!("数据目录包含符号链接：{}", entry.path().display()));
        }
        if file_type.is_dir() {
            let nested = measure_dir(&entry.path(), false)?;
            stats.files += nested.files;
            stats.bytes += nested.bytes;
        } else if file_type.is_file() {
            stats.files += 1;
            stats.bytes += entry.metadata().map_err(|e| e.to_string())?.len();
        }
    }
    Ok(stats)
}

fn validate_target(source: &Path, target: &Path) -> Result<(), String> {
    let source = canonical_or_original(source);
    let target = canonical_or_original(target);
    if same_path(&source, &target) {
        return Ok(());
    }
    if target.starts_with(&source) || source.starts_with(&target) {
        return Err("新旧数据目录不能互相包含，请选择独立目录（例如 E:\\AIKS-Data）".to_string());
    }
    let default = canonical_or_original(&default_data_root());
    if !same_path(&target, &default) && target.starts_with(&default) {
        return Err("自定义数据目录不能放在 AIKS 默认控制目录内部".to_string());
    }
    Ok(())
}

fn ensure_effectively_empty_or_recoverable(path: &Path) -> Result<(), String> {
    if path.join(MIGRATION_MARKER).exists() {
        return Ok(());
    }
    ensure_effectively_empty(path)
}

fn ensure_effectively_empty(path: &Path) -> Result<(), String> {
    for entry in fs::read_dir(path).map_err(|e| format!("无法读取目标目录：{e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !is_control_name(&entry.file_name()) {
            return Err(format!(
                "目标目录必须为空，检测到现有内容：{}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn is_control_name(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(POINTER_FILE | PENDING_FILE | STATUS_FILE | MIGRATION_MARKER)
    )
}

fn requires_initial_setup() -> bool {
    if env_override().is_some() || pointer_path().exists() {
        return false;
    }
    !has_existing_aiks_data(&default_data_root())
}

fn has_existing_aiks_data(root: &Path) -> bool {
    ["aiks.db", "config", "archive", "siyuan", "data"]
        .iter()
        .any(|name| root.join(name).exists())
}

fn env_override() -> Option<String> {
    std::env::var("AIKS_DATA_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn default_data_root() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("AIKnowledgeSync")
}

fn pointer_path() -> PathBuf {
    default_data_root().join(POINTER_FILE)
}

fn pending_path() -> PathBuf {
    default_data_root().join(PENDING_FILE)
}

fn status_path() -> PathBuf {
    default_data_root().join(STATUS_FILE)
}

fn write_pointer(root: &Path) -> Result<(), String> {
    fs::create_dir_all(default_data_root()).map_err(|e| e.to_string())?;
    fs::write(pointer_path(), format!("{}\n", root.display()))
        .map_err(|e| format!("无法保存数据目录设置：{e}"))
}

fn write_status(message: &str) -> Result<(), String> {
    fs::create_dir_all(default_data_root()).map_err(|e| e.to_string())?;
    fs::write(status_path(), format!("{}\n", message)).map_err(|e| e.to_string())
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn same_path(a: &Path, b: &Path) -> bool {
    let a = canonical_or_original(a).to_string_lossy().replace('\\', "/");
    let b = canonical_or_original(b).to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    {
        a.eq_ignore_ascii_case(&b)
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("aiks-storage-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn copy_tree_preserves_file_count_and_bytes() {
        let source = temp_root("source");
        let target = temp_root("target");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("a.txt"), b"hello").unwrap();
        fs::write(source.join("nested").join("b.bin"), [1u8, 2, 3, 4]).unwrap();
        fs::write(source.join(POINTER_FILE), b"control").unwrap();

        copy_tree(&source, &target).unwrap();
        assert_eq!(measure_tree(&source).unwrap(), measure_tree(&target).unwrap());
        assert!(!target.join(POINTER_FILE).exists());

        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(target);
    }

    #[test]
    fn migration_target_must_be_independent() {
        let source = temp_root("nesting");
        let nested = source.join("child");
        fs::create_dir_all(&nested).unwrap();
        assert!(validate_target(&source, &nested).is_err());
        let _ = fs::remove_dir_all(source);
    }

    #[test]
    fn non_control_content_makes_target_non_empty() {
        let target = temp_root("non-empty");
        fs::write(target.join("user.txt"), b"keep").unwrap();
        assert!(ensure_effectively_empty(&target).is_err());
        let _ = fs::remove_dir_all(target);
    }
}
