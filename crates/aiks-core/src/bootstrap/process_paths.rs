//! Keep Rust's canonical paths internally, but do not pass a verbatim Windows
//! workspace to SiYuan's SQLite DSN parser (the prefix contains a query '?').
use std::path::{Path, PathBuf};

/// Simplify only existing paths whose ordinary spelling resolves to the same
/// target. Never globally strip a prefix, rewrite device paths, or change the
/// authoritative data root / its ownership and reparse-point checks.
pub(super) fn for_kernel(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(candidate) = path.to_str().and_then(windows_candidate) {
            if let (Ok(original), Ok(normal)) = (path.canonicalize(), candidate.canonicalize()) {
                if original == normal {
                    return candidate;
                }
            }
        }
    }
    // An unresolved or non-equivalent path must retain its original semantics.
    path.to_path_buf()
}

#[cfg(any(windows, test))]
fn windows_candidate(value: &str) -> Option<PathBuf> {
    let tail = value.strip_prefix(r"\\?\")?;
    if let Some(unc) = tail.strip_prefix(r"UNC\") {
        let mut parts = unc.split('\\');
        if parts.next()?.is_empty() || parts.next()?.is_empty() {
            return None;
        }
        return Some(PathBuf::from(format!(r"\\{unc}")));
    }
    let bytes = tail.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1..3] == *b":\\" {
        Some(PathBuf::from(tail))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_preserve_drive_unc_unicode_and_spaces() {
        for (input, expected) in [
            (
                r"\\?\E:\AIKS data\个人\workspace",
                r"E:\AIKS data\个人\workspace",
            ),
            (
                r"\\?\UNC\server\share\workspace",
                r"\\server\share\workspace",
            ),
        ] {
            assert_eq!(windows_candidate(input), Some(PathBuf::from(expected)));
        }
    }

    #[test]
    fn device_relative_and_ordinary_paths_are_not_rewritten() {
        for input in [
            r"E:\ordinary\workspace",
            r"relative\workspace",
            r"\\.\PhysicalDrive0",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1",
            r"\\?\Volume{synthetic}\workspace",
            r"\\?\E:relative",
            r"\\?\UNC\\share",
            r"\\?\UNC\server\",
        ] {
            assert_eq!(windows_candidate(input), None, "{input}");
        }
    }

    #[test]
    fn unknown_paths_are_not_retargeted_or_created() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().canonicalize().unwrap().join("not-created");
        assert_eq!(for_kernel(&missing), missing);
        assert!(!missing.exists());
    }

    #[cfg(windows)]
    #[test]
    fn canonical_existing_workspace_uses_the_same_directory_without_verbatim_prefix() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("profile 测试");
        let workspace = profile.join("siyuan/workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let profile = profile.canonicalize().unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config =
            crate::bootstrap::BootstrapConfig::new(profile.clone(), profile.clone(), "Test");
        let runtime = config.runtime_config();
        assert!(!runtime.workspace.to_str().unwrap().starts_with(r"\\?\"));
        assert!(!runtime.runtime_root.to_str().unwrap().starts_with(r"\\?\"));
        assert_eq!(runtime.workspace.canonicalize().unwrap(), workspace);
        assert_eq!(runtime.runtime_root.canonicalize().unwrap(), profile);
        assert_eq!(config.data_dir, profile);
        assert_eq!(runtime.runtime_info_path, profile.join("runtime.json"));
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_paths_are_preserved_even_when_their_text_looks_like_a_prefix() {
        let input = Path::new(r"\\?\E:\not-a-windows-path-on-this-platform");
        assert_eq!(for_kernel(input), input);
    }
}
