//! Read-only backup inspection for the desktop. Never start SiYuan, read
//! Provider directories, or restore into the running application's data root.
use std::path::{Path, PathBuf};

use serde::Serialize;

use aiks_core::storage::app_backup::{
    preflight_offline_restore, preflight_offline_restore_with_capacity,
    restore_offline_backup_with_capacity_and_confirmation, verify_offline_backup,
    OfflineRestorePreview,
};

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
