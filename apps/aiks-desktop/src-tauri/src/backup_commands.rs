//! Read-only backup inspection for the desktop. Never start SiYuan, read
//! Provider directories, or restore into the running application's data root.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use aiks_core::runtime::{RuntimeState, SiyuanRuntime};
use aiks_core::storage::backup_quiescence::BackupQuiescence;
use tokio::sync::Mutex;

use serde::Serialize;

use aiks_core::storage::app_backup::{
    create_offline_backup_from_quiesced_workspace, preflight_offline_restore,
    preflight_offline_restore_with_capacity, restore_offline_backup_with_capacity_and_confirmation,
    verify_offline_backup, OfflineRestorePreview,
};

/// Bind internal lifecycle backup strictly to this process's managed data
/// directory, never to arbitrary paths supplied by a future UI command.
fn validate_managed_backup_paths(
    data_root: &Path,
    db_path: &Path,
    workspace: &Path,
    destination: &Path,
) -> Result<(), String> {
    let root = std::fs::canonicalize(data_root).map_err(|e| e.to_string())?;
    let database = std::fs::canonicalize(db_path).map_err(|e| e.to_string())?;
    let actual_workspace = std::fs::canonicalize(workspace).map_err(|e| e.to_string())?;
    if database != root.join("aiks.db") || actual_workspace != root.join("siyuan").join("workspace")
    {
        return Err("备份来源与当前 AIKS 数据目录不一致".to_string());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "备份目标没有父目录".to_string())?;
    let parent = std::fs::canonicalize(parent).map_err(|e| e.to_string())?;
    let name = destination
        .file_name()
        .ok_or_else(|| "备份目标没有目录名".to_string())?;
    let output = parent.join(name);
    if output.starts_with(&root) || root.starts_with(&output) {
        return Err("备份目标不能与当前 AIKS 数据目录重叠".to_string());
    }
    if std::fs::symlink_metadata(&output).is_ok() {
        return Err("备份目标必须是不存在的新目录".to_string());
    }
    Ok(())
}

/// Internal-only safety orchestration. Not exposed through Tauri until all
/// writing paths and external SiYuan clients are covered by an audit.
#[allow(dead_code)]
async fn create_managed_quiesced_backup(
    runtime: Arc<Mutex<Option<SiyuanRuntime>>>,
    gate: Arc<BackupQuiescence>,
    db_path: PathBuf,
    workspace: PathBuf,
    destination: PathBuf,
) -> Result<usize, String> {
    if aiks_core::bootstrap::DevOverride::from_env().is_active() {
        return Err("外部 SiYuan 无法由 AIKS 安全停写".to_string());
    }
    validate_managed_backup_paths(
        &aiks_core::config::data_root(),
        &db_path,
        &workspace,
        &destination,
    )?;
    let paused = tauri::async_runtime::spawn_blocking(move || {
        BackupQuiescence::pause_and_drain_owned_with_timeout(&gate, Duration::from_secs(120))
            .map_err(str::to_string)
    })
    .await
    .map_err(|e| e.to_string())??;
    let runtime_guard = runtime.lock().await;
    let Some(kernel) = runtime_guard.as_ref() else {
        return Err("未找到由 AIKS 管理的 SiYuan 内核".to_string());
    };
    if kernel.state() != RuntimeState::Ready {
        return Err("SiYuan 内核未就绪，禁止完整备份".to_string());
    }
    kernel
        .stop_for_backup()
        .await
        .map_err(|error| format!("未确认 SiYuan 子进程退出，禁止备份：{error}"))?;
    let snapshot = if kernel.state() == RuntimeState::Stopped {
        tauri::async_runtime::spawn_blocking(move || {
            let metadata = std::fs::symlink_metadata(&db_path).map_err(|e| e.to_string())?;
            if !metadata.file_type().is_file() {
                return Err("AIKS 数据库不是普通文件".to_string());
            }
            let db = aiks_core::storage::StateDb::open(&db_path).map_err(|e| e.to_string())?;
            create_offline_backup_from_quiesced_workspace(&db, &workspace, &destination)
                .map(|manifest| manifest.entries.len())
                .map_err(|e| e.to_string())
        })
        .await
        .unwrap_or_else(|error| Err(format!("备份工作线程异常退出：{error}")))
    } else {
        Err("SiYuan 未确认停止，禁止读取工作空间".to_string())
    };
    // Always attempt restart, even when snapshot failed. Hold the paused
    // guard until the restart attempt has finished.
    let restarted = kernel.start().await.map_err(|e| e.to_string());
    if restarted.is_err() {
        paused.gate_recovery_required();
    }
    drop(runtime_guard);
    drop(paused);
    match (snapshot, restarted) {
        (Ok(count), Ok(_)) => Ok(count),
        (Err(error), Ok(_)) => Err(error),
        (Ok(_), Err(error)) => Err(format!("备份完成，但 SiYuan 重启失败：{error}")),
        (Err(snapshot_error), Err(restart_error)) => Err(format!(
            "备份失败：{snapshot_error}；SiYuan 重启也失败：{restart_error}"
        )),
    }
}

/// Verify the already-running managed SiYuan is healthy before unblocking
/// writes after a previously failed restart. Never launches external kernels.
#[tauri::command]
pub async fn verify_managed_siyuan_recovery(
    runtime: tauri::State<'_, Arc<Mutex<Option<SiyuanRuntime>>>>,
    gate: tauri::State<'_, Arc<BackupQuiescence>>,
) -> Result<bool, String> {
    if aiks_core::bootstrap::DevOverride::from_env().is_active() {
        return Err("外部 SiYuan 不受 AIKS 恢复门禁控制".to_string());
    }
    if !gate.is_recovery_required() {
        return Err("当前不存在需要核验的停写故障".to_string());
    }
    let guard = runtime.lock().await;
    let kernel = guard
        .as_ref()
        .ok_or_else(|| "未找到 AIKS 管理的 SiYuan 内核".to_string())?;
    let health = kernel.health().await;
    if health.state != RuntimeState::Ready || kernel.state() != RuntimeState::Ready {
        return Err("SiYuan 尚未恢复就绪，继续禁止后台写入".to_string());
    }
    gate.clear_after_verified_recovery().map_err(str::to_string)?;
    Ok(true)
}

fn validated_path(raw: &str) -> Result<PathBuf, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err("备份路径不能为空".to_string());
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err("备份路径必须是绝对路径".to_string());
    }
    Ok(path)
}

fn inspect(bundle_path: &str, destination_path: &str) -> Result<OfflineRestorePreview, String> {
    let bundle = validated_path(bundle_path)?;
    let destination = validated_path(destination_path)?;
    let active = aiks_core::config::data_root();
    let active = std::fs::canonicalize(&active).map_err(|e| e.to_string())?;
    let parent = destination
        .parent()
        .ok_or_else(|| "恢复目标必须有父目录".to_string())?;
    let parent = std::fs::canonicalize(parent).map_err(|e| e.to_string())?;
    let destination_name = destination
        .file_name()
        .ok_or_else(|| "恢复目标必须有目录名".to_string())?;
    let resolved_destination = parent.join(destination_name);
    if resolved_destination.starts_with(&active) || active.starts_with(&resolved_destination) {
        return Err("恢复目标不能与当前 AIKS 数据目录重叠".to_string());
    }
    preflight_offline_restore(&bundle, &destination).map_err(|e| e.to_string())
}

/// Windows free space for the volume containing the selected restore parent.
/// Query the operating system, not a user-supplied estimate.
#[cfg(windows)]
fn available_restore_bytes(destination: &Path) -> Result<u64, String> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            directory_name: *const u16,
            free_bytes_available: *mut u64,
            total_bytes: *mut u64,
            total_free_bytes: *mut u64,
        ) -> i32;
    }

    let parent = destination
        .parent()
        .ok_or_else(|| "恢复目标缺少父目录".to_string())?;
    let parent = std::fs::canonicalize(parent).map_err(|e| e.to_string())?;
    let wide: Vec<u16> = parent.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0u64;
    let status = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if status == 0 {
        return Err(format!(
            "无法查询恢复磁盘可用容量：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(available)
}

#[cfg(not(windows))]
fn available_restore_bytes(_destination: &Path) -> Result<u64, String> {
    Err("当前平台尚不支持安全的实时磁盘空间检查，已禁止执行恢复".to_string())
}

#[derive(Debug, Serialize)]
pub struct DesktopRestorePreview {
    #[serde(flatten)]
    pub backup: OfflineRestorePreview,
    pub available_bytes: u64,
    pub required_bytes: u64,
}

fn inspect_with_capacity(
    bundle_path: &str,
    destination_path: &str,
) -> Result<DesktopRestorePreview, String> {
    let summary = inspect(bundle_path, destination_path)?;
    let destination = validated_path(destination_path)?;
    let available = available_restore_bytes(&destination)?;
    let bundle = validated_path(bundle_path)?;
    preflight_offline_restore_with_capacity(&bundle, &destination, available)
        .map_err(|e| e.to_string())?;
    let reserve = (summary.total_bytes / 5).max(64 * 1024 * 1024);
    let required = summary
        .total_bytes
        .checked_add(reserve)
        .ok_or_else(|| "恢复空间预算溢出".to_string())?;
    Ok(DesktopRestorePreview {
        backup: summary,
        available_bytes: available,
        required_bytes: required,
    })
}

/// Manual offline backup is restricted to a separate, already-quiesced
/// workspace. The running application's SiYuan workspace is never sampled.
#[tauri::command]
pub async fn create_offline_backup_from_stopped_workspace(
    workspace_path: String,
    destination_path: String,
) -> Result<usize, String> {
    if aiks_core::bootstrap::DevOverride::from_env().is_active() {
        return Err("外部 SiYuan 开发模式不能保证停写，禁止创建完整备份".to_string());
    }
    let workspace = validated_path(&workspace_path)?;
    let destination = validated_path(&destination_path)?;
    let active =
        std::fs::canonicalize(aiks_core::config::data_root()).map_err(|error| error.to_string())?;
    let workspace = std::fs::canonicalize(workspace).map_err(|error| error.to_string())?;
    let parent = destination
        .parent()
        .ok_or_else(|| "备份目录缺少父目录".to_string())?;
    let parent = std::fs::canonicalize(parent).map_err(|error| error.to_string())?;
    let name = destination
        .file_name()
        .ok_or_else(|| "备份目录缺少名称".to_string())?;
    let output = parent.join(name);
    if workspace.starts_with(&active) || active.starts_with(&workspace) {
        return Err("不能备份正在运行的 AIKS 工作空间，请选择单独停写的工作空间".to_string());
    }
    if output.starts_with(&active) || active.starts_with(&output) {
        return Err("备份输出不能与当前 AIKS 数据目录重叠".to_string());
    }
    if output.starts_with(&workspace) || workspace.starts_with(&output) {
        return Err("备份输出不能与来源工作空间重叠".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let db_path = active.join("aiks.db");
        let metadata = std::fs::symlink_metadata(&db_path)
            .map_err(|error| format!("找不到已有 AIKS 状态数据库：{error}"))?;
        if !metadata.file_type().is_file() {
            return Err("AIKS 状态数据库必须是普通文件，不能是目录或符号链接".to_string());
        }
        let db = aiks_core::storage::StateDb::open(&db_path).map_err(|error| error.to_string())?;
        create_offline_backup_from_quiesced_workspace(&db, &workspace, &output)
            .map(|manifest| manifest.entries.len())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("离线备份任务异常退出：{error}"))?
}

#[tauri::command]
pub fn inspect_offline_backup_capacity(
    bundle_path: String,
    destination_path: String,
) -> Result<DesktopRestorePreview, String> {
    inspect_with_capacity(&bundle_path, &destination_path)
}

#[tauri::command]
pub fn inspect_offline_backup(
    bundle_path: String,
    destination_path: String,
) -> Result<OfflineRestorePreview, String> {
    inspect(&bundle_path, &destination_path)
}

#[tauri::command]
pub fn verify_offline_backup_manifest(bundle_path: String) -> Result<usize, String> {
    let bundle = validated_path(&bundle_path)?;
    verify_offline_backup(Path::new(&bundle))
        .map(|manifest| manifest.entries.len())
        .map_err(|e| e.to_string())
}

/// Isolated restore is opt-in and never replaces live AIKS or SiYuan data.
/// The user must enter the *same* absolute destination during confirmation.
#[tauri::command]
pub async fn restore_offline_backup_isolated(
    bundle_path: String,
    destination_path: String,
    confirmed_destination_path: String,
) -> Result<usize, String> {
    if destination_path != confirmed_destination_path {
        return Err("恢复目标与用户确认的路径不一致".to_string());
    }
    // Re-run all checks immediately before writing, not merely when the
    // preview was displayed. Large copies run off the UI event loop.
    tauri::async_runtime::spawn_blocking(move || {
        let preview = inspect_with_capacity(&bundle_path, &destination_path)?;
        let bundle = validated_path(&bundle_path)?;
        let destination = validated_path(&destination_path)?;
        let confirmed = validated_path(&confirmed_destination_path)?;
        restore_offline_backup_with_capacity_and_confirmation(
            &bundle,
            &destination,
            &confirmed,
            preview.available_bytes,
        )
        .map(|manifest| manifest.entries.len())
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("恢复任务未正常完成：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_data_destination_cannot_pass_preview() {
        let current = aiks_core::config::data_root();
        if let Ok(current) = std::fs::canonicalize(current) {
            let target = current.join("restore-preview");
            let result = inspect("relative-bundle", &target.to_string_lossy());
            assert!(result.is_err());
        }
    }

    #[test]
    fn only_absolute_paths_are_accepted() {
        assert!(validated_path("").is_err());
        assert!(validated_path("../workspace").is_err());
        assert!(validated_path("relative").is_err());
    }
}

#[cfg(test)]
mod managed_path_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn managed_backup_rejects_alien_sources_and_nested_output() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("aiks-backup-path-test-{nonce}"));
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.join("aiks");
        let workspace = root.join("siyuan").join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let database = root.join("aiks.db");
        std::fs::write(&database, b"fixture").unwrap();
        let output = dir.join("backup");
        assert!(validate_managed_backup_paths(&root, &database, &workspace, &output).is_ok());
        assert!(
            validate_managed_backup_paths(&root, &database, &workspace, &root.join("backup"),)
                .is_err()
        );
        assert!(
            validate_managed_backup_paths(&root, &database, &dir.join("other"), &output,).is_err()
        );
        std::fs::create_dir(&output).unwrap();
        assert!(validate_managed_backup_paths(&root, &database, &workspace, &output).is_err());
        assert!(output.is_dir());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
