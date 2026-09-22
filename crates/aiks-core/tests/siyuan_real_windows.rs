//! Opt-in smoke tests against the exact packaged SiYuan kernel, not a mock.
//! Run on Windows with AIKS_TEST_SIYUAN_RUNTIME_ROOT set by the CI fetcher.
#![cfg(windows)]

use aiks_core::{bootstrap::BootstrapConfig, runtime::SiyuanRuntime};
use std::path::PathBuf;

async fn boot_fresh_workspace(canonical: bool) {
    let runtime_root = PathBuf::from(
        std::env::var_os("AIKS_TEST_SIYUAN_RUNTIME_ROOT")
            .expect("Prepare the pinned runtime before running this opt-in test"),
    )
    .canonicalize()
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("AIKS profile 测试");
    std::fs::create_dir_all(profile.join("siyuan/workspace")).unwrap();
    let profile = profile.canonicalize().unwrap();
    assert!(profile.to_str().unwrap().starts_with(r"\\?\"));
    let profile = if canonical {
        profile
    } else {
        PathBuf::from(profile.to_str().unwrap().strip_prefix(r"\\?\").unwrap())
    };
    let config = BootstrapConfig::new(runtime_root, profile.clone(), "Smoke test");
    let runtime = SiyuanRuntime::new(config.runtime_config());
    let result = runtime.start().await;
    // Always clean up the child before assertions; these are temporary profiles.
    runtime.stop().await;
    if let Err(error) = &result {
        let logs = profile.join("logs");
        for entry in std::fs::read_dir(logs).unwrap().flatten() {
            if let Ok(text) = std::fs::read_to_string(entry.path()) {
                let tail: Vec<_> = text.lines().rev().take(60).collect();
                eprintln!("{}", tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
            }
        }
        panic!("Fresh workspace failed (canonical={canonical}): {error:#}");
    }
    assert!(result.unwrap().version.starts_with("3."));
    assert!(profile.join("siyuan/workspace/conf/conf.json").is_file());
}

#[tokio::test]
#[ignore = "Requires the pinned real Windows SiYuan release"]
async fn fresh_plain_windows_workspace_reaches_ready() {
    boot_fresh_workspace(false).await;
}

#[tokio::test]
#[ignore = "Requires the pinned real Windows SiYuan release"]
async fn fresh_canonical_windows_workspace_reaches_ready() {
    boot_fresh_workspace(true).await;
}
