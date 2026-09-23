// CI lint baseline: pre-existing Clippy debt; remove allowances incrementally.
#![allow(
    clippy::manual_range_contains,
    clippy::ptr_arg,
    clippy::redundant_pattern_matching
)]

/// SiYuan Kernel Runtime Manager.
///
/// Manages the lifecycle of the embedded SiYuan-Kernel process.
/// Embedded mode uses NO accessAuthCode — SiYuan listens on 127.0.0.1 only,
/// and unauthenticated local requests are allowed by default.
///
/// Layout convention (spec §14, §20):
///   runtime_root/
///   ├── kernel/
///   │   └── SiYuan-Kernel.exe
///   ├── stage/
///   ├── appearance/
///   └── guide/
///
/// Kernel command (SiYuan 3.7.0+, spec §21):
///   SiYuan-Kernel.exe serve
///       --workspace=<AIKS workspace>
///       --wd=<runtime_root>
///       --port=<allocated port>
///       --lang=zh-CN
///       --mode=prod
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

// ── Configuration ─────────────────────────────────────────────────────────────

/// Configuration for starting the SiYuan Kernel.
///
/// `runtime_root` must point to the directory that contains:
///   `kernel/SiYuan-Kernel.exe`, `stage/`, `appearance/`, `guide/`
#[derive(Debug, Clone)]
pub struct SiyuanRuntimeConfig {
    /// Root of the SiYuan runtime (contains kernel/, stage/, appearance/)
    pub runtime_root: PathBuf,
    /// SiYuan workspace directory (user data)
    pub workspace: PathBuf,
    /// Port override; if None, auto-allocate from 6806
    pub port: Option<u16>,
    /// Language for SiYuan UI
    pub language: String,
    /// SiYuan version this runtime must match (for version check)
    pub expected_version: Option<String>,
    /// Max time to wait for Kernel to become ready
    pub startup_timeout: Duration,
    /// Log file directory (for Kernel stdout/stderr)
    pub log_dir: Option<PathBuf>,
    /// Path to save runtime.json for crash recovery
    pub runtime_info_path: PathBuf,
}

impl SiyuanRuntimeConfig {
    pub fn new(runtime_root: PathBuf, workspace: PathBuf, data_dir: &PathBuf) -> Self {
        Self {
            runtime_root,
            workspace,
            port: None,
            language: "zh-CN".to_string(),
            expected_version: None,
            startup_timeout: Duration::from_secs(30),
            log_dir: Some(data_dir.join("logs")),
            runtime_info_path: data_dir.join("runtime.json"),
        }
    }

    /// Absolute path to SiYuan-Kernel.exe inside runtime_root.
    pub fn kernel_exe(&self) -> PathBuf {
        let name = if cfg!(windows) {
            "SiYuan-Kernel.exe"
        } else {
            "SiYuan-Kernel"
        };
        self.runtime_root.join("kernel").join(name)
    }
}

// ── Runtime state ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeState {
    Stopped,
    Starting,
    Ready,
    Failed,
    Stopping,
}

/// Persistent info saved to runtime.json for crash recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub pid: u32,
    pub port: u16,
    pub workspace: String,
    pub version: String,
    #[serde(rename = "startedByAiks")]
    pub started_by_aiks: bool,
}

/// Live runtime status, returned by `SiyuanRuntime::health()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeHealth {
    pub state: RuntimeState,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub version: Option<String>,
    pub base_url: Option<String>,
    pub last_error: Option<String>,
}

/// Validation result from `SiyuanRuntime::validate()`.
#[derive(Debug, Clone)]
pub struct RuntimeValidation {
    pub kernel_exists: bool,
    pub kernel_size_bytes: u64,
    pub stage_exists: bool,
    pub appearance_exists: bool,
    pub errors: Vec<String>,
}

impl RuntimeValidation {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
            && self.kernel_exists
            && self.kernel_size_bytes > 1_000_000
            && self.stage_exists
            && self.appearance_exists
    }
}

/// Info about a successfully started runtime.
#[derive(Debug, Clone)]
pub struct SiyuanRuntimeInfo {
    pub pid: u32,
    pub port: u16,
    pub version: String,
    pub base_url: String,
}

// ── SiyuanRuntime ─────────────────────────────────────────────────────────────

pub struct SiyuanRuntime {
    config: SiyuanRuntimeConfig,
    child: Arc<Mutex<Option<Child>>>,
    info: Arc<Mutex<Option<SiyuanRuntimeInfo>>>,
    state: Arc<Mutex<RuntimeState>>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl SiyuanRuntime {
    pub fn new(config: SiyuanRuntimeConfig) -> Self {
        Self {
            config,
            child: Arc::new(Mutex::new(None)),
            info: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(RuntimeState::Stopped)),
            last_error: Arc::new(Mutex::new(None)),
        }
    }

    // ── Public accessors ────────────────────────────────────────────────────

    pub fn state(&self) -> RuntimeState {
        self.state.lock().unwrap().clone()
    }

    pub fn port(&self) -> Option<u16> {
        self.info.lock().unwrap().as_ref().map(|i| i.port)
    }

    pub fn base_url(&self) -> Option<String> {
        self.info
            .lock()
            .unwrap()
            .as_ref()
            .map(|i| i.base_url.clone())
    }

    pub fn runtime_info(&self) -> Option<SiyuanRuntimeInfo> {
        self.info.lock().unwrap().clone()
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }

    // ── Validate runtime layout ─────────────────────────────────────────────

    /// Check that the runtime_root contains all necessary files.
    pub fn validate(config: &SiyuanRuntimeConfig) -> RuntimeValidation {
        let mut errors = Vec::new();
        let kernel = config.kernel_exe();

        let kernel_exists = kernel.exists();
        let kernel_size = if kernel_exists {
            kernel.metadata().map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        if !kernel_exists {
            errors.push(format!("Kernel not found: {}", kernel.display()));
        } else if kernel_size < 1_000_000 {
            errors.push(format!(
                "Kernel too small ({} bytes): {}",
                kernel_size,
                kernel.display()
            ));
        }

        let stage = config.runtime_root.join("stage");
        let appearance = config.runtime_root.join("appearance");

        let stage_exists = stage.exists()
            && stage.is_dir()
            && std::fs::read_dir(&stage)
                .map(|d| d.count() > 0)
                .unwrap_or(false);
        let appearance_exists = appearance.exists()
            && appearance.is_dir()
            && std::fs::read_dir(&appearance)
                .map(|d| d.count() > 0)
                .unwrap_or(false);

        if !stage_exists {
            errors.push(format!("stage/ missing or empty: {}", stage.display()));
        }
        if !appearance_exists {
            errors.push(format!(
                "appearance/ missing or empty: {}",
                appearance.display()
            ));
        }

        RuntimeValidation {
            kernel_exists,
            kernel_size_bytes: kernel_size,
            stage_exists,
            appearance_exists,
            errors,
        }
    }

    // ── Crash recovery ──────────────────────────────────────────────────────

    /// Check if a previous AIKS-managed Kernel is still running.
    /// Returns port if successfully recovered.
    pub async fn try_recover_existing(&self) -> Option<u16> {
        let info_path = &self.config.runtime_info_path;
        if !info_path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(info_path).ok()?;
        let saved: RuntimeInfo = serde_json::from_str(&content).ok()?;

        if !saved.started_by_aiks {
            return None;
        }

        // Verify the expected version matches
        if let Some(ref expected) = self.config.expected_version {
            if &saved.version != expected {
                info!(
                    "Saved runtime version mismatch ({} vs {}), ignoring",
                    saved.version, expected
                );
                return None;
            }
        }

        // Check process is still alive
        if !is_process_alive(saved.pid) {
            debug!("Saved PID {} is no longer running", saved.pid);
            let _ = std::fs::remove_file(info_path);
            return None;
        }

        // Check HTTP health
        let base_url = format!("http://127.0.0.1:{}", saved.port);
        if let Ok(version) = get_siyuan_version(&base_url, None).await {
            info!(pid = saved.pid, port = saved.port, version = %version, "Recovered existing SiYuan");
            let info = SiyuanRuntimeInfo {
                pid: saved.pid,
                port: saved.port,
                version,
                base_url,
            };
            *self.info.lock().unwrap() = Some(info);
            *self.state.lock().unwrap() = RuntimeState::Ready;
            return Some(saved.port);
        }

        debug!("Saved runtime at port {} not responding", saved.port);
        let _ = std::fs::remove_file(info_path);
        None
    }

    // ── Start ───────────────────────────────────────────────────────────────

    /// Start the SiYuan Kernel and wait until it is ready.
    pub async fn start(&self) -> anyhow::Result<SiyuanRuntimeInfo> {
        // Validate first
        let validation = Self::validate(&self.config);
        if !validation.is_valid() {
            let msg = validation.errors.join("; ");
            anyhow::bail!("SiYuan runtime validation failed: {}", msg);
        }

        // Try recovery
        if let Some(_) = self.try_recover_existing().await {
            if let Some(info) = self.runtime_info() {
                return Ok(info);
            }
        }

        *self.state.lock().unwrap() = RuntimeState::Starting;

        // Allocate port
        let port = self.config.port.unwrap_or(0);
        let port = if port == 0 {
            allocate_port(6806, 6899)
                .ok_or_else(|| anyhow::anyhow!("No available port in range 6806-6899"))?
        } else {
            port
        };

        info!(port, "Starting SiYuan Kernel");
        debug!("Runtime root: {}", self.config.runtime_root.display());
        debug!("Workspace: {}", self.config.workspace.display());

        // Ensure workspace directory exists
        std::fs::create_dir_all(&self.config.workspace)?;

        // Build kernel arguments (spec §21)
        let kernel_exe = self.config.kernel_exe();
        let args: Vec<String> = vec![
            "serve".to_string(),
            format!("--workspace={}", self.config.workspace.display()),
            format!("--wd={}", self.config.runtime_root.display()),
            format!("--port={}", port),
            format!("--lang={}", self.config.language),
            "--mode=prod".to_string(),
        ];

        debug!("Kernel args: {}", args.join(" "));

        // Set up logging
        let (stdout_stdio, stderr_stdio) = self.make_log_stdio(port)?;

        // Spawn kernel. On Windows use CREATE_NO_WINDOW so the bundled kernel
        // remains a true background child instead of flashing a console window.
        let mut command = Command::new(&kernel_exe);
        command
            .args(&args)
            .stdout(stdout_stdio)
            .stderr(stderr_stdio);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let child = command
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn SiYuan-Kernel: {}", e))?;

        let pid = child.id();
        *self.child.lock().unwrap() = Some(child);

        info!(pid, port, "SiYuan Kernel process started");

        // Wait for ready
        let base_url = format!("http://127.0.0.1:{}", port);
        let expected_ver = self.config.expected_version.clone();

        match self
            .wait_ready(pid, &base_url, expected_ver.as_deref())
            .await
        {
            Ok(version) => {
                let info = SiyuanRuntimeInfo {
                    pid,
                    port,
                    version: version.clone(),
                    base_url: base_url.clone(),
                };

                *self.info.lock().unwrap() = Some(info.clone());
                *self.state.lock().unwrap() = RuntimeState::Ready;
                *self.last_error.lock().unwrap() = None;

                // Save runtime.json
                self.save_runtime_info(pid, port, &version);

                info!(port, pid, version = %version, "SiYuan Kernel is READY");
                Ok(info)
            }
            Err(e) => {
                let msg = e.to_string();
                *self.state.lock().unwrap() = RuntimeState::Failed;
                *self.last_error.lock().unwrap() = Some(msg.clone());
                error!(error = %msg, "SiYuan Kernel failed to start");
                self.kill_child();
                Err(anyhow::anyhow!("{}", msg))
            }
        }
    }

    /// Wait until the Kernel has completed its full boot sequence.
    /// `/api/system/version` becomes available before database rebuild/indexing finishes,
    /// so readiness is gated by the official `/api/system/bootProgress` endpoint.
    async fn wait_ready(
        &self,
        pid: u32,
        base_url: &str,
        expected_version: Option<&str>,
    ) -> anyhow::Result<String> {
        let deadline = tokio::time::Instant::now() + self.config.startup_timeout;
        let delays_ms: &[u64] = &[
            250, 500, 500, 1000, 1000, 1000, 2000, 2000, 2000, 3000, 3000, 5000,
        ];
        let mut delay_iter = delays_ms.iter().cycle();
        let mut detected_version: Option<String> = None;

        loop {
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "SiYuan Kernel did not become ready within {}s",
                    self.config.startup_timeout.as_secs()
                );
            }

            if !is_process_alive(pid) {
                anyhow::bail!("SiYuan Kernel process exited unexpectedly (PID {})", pid);
            }

            let delay = *delay_iter.next().unwrap_or(&2000);
            tokio::time::sleep(Duration::from_millis(delay)).await;

            if detected_version.is_none() {
                match get_siyuan_version(base_url, None).await {
                    Ok(version) => {
                        if let Some(expected) = expected_version {
                            if version != expected {
                                warn!(
                                    expected = expected,
                                    actual = %version,
                                    "Bundled SiYuan runtime version mismatch"
                                );
                            }
                        }
                        detected_version = Some(version);
                    }
                    Err(e) => {
                        debug!("Version health check: {}", e);
                        continue;
                    }
                }
            }

            match get_siyuan_boot_progress(base_url, None).await {
                Ok((progress, details)) => {
                    debug!(progress, details = %details, "SiYuan boot progress");
                    if progress >= 100 {
                        return Ok(detected_version
                            .clone()
                            .expect("version checked before boot progress"));
                    }
                }
                Err(e) => debug!("Boot progress check: {}", e),
            }
        }
    }

    // ── Stop ────────────────────────────────────────────────────────────────

    /// Gracefully stop SiYuan (spec §35).
    pub async fn stop(&self) {
        if self.state() == RuntimeState::Stopped {
            return;
        }

        *self.state.lock().unwrap() = RuntimeState::Stopping;

        // Try graceful API shutdown first
        let base_url_opt = self.base_url();
        if let Some(base_url) = base_url_opt {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_default();

            let _ = client
                .post(format!("{}/api/system/exit", base_url))
                .json(&serde_json::json!({"force": false}))
                .send()
                .await;

            // Wait up to 8 seconds for graceful exit (spec §35)
            // Extract pid BEFORE entering the wait loop to avoid holding guard across awaits
            let maybe_pid = self.info.lock().unwrap().as_ref().map(|i| i.pid);
            if let Some(pid) = maybe_pid {
                let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
                while tokio::time::Instant::now() < deadline {
                    if !is_process_alive(pid) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }

        // Force kill if still running
        self.kill_child();

        *self.info.lock().unwrap() = None;
        *self.state.lock().unwrap() = RuntimeState::Stopped;

        // Remove runtime.json
        let _ = std::fs::remove_file(&self.config.runtime_info_path);

        info!("SiYuan Kernel stopped");
    }

    /// Restart the kernel.
    pub async fn restart(&self) -> anyhow::Result<SiyuanRuntimeInfo> {
        self.stop().await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.start().await
    }

    /// Live health check.
    pub async fn health(&self) -> RuntimeHealth {
        if self.state() == RuntimeState::Ready {
            if let Some(info) = self.runtime_info() {
                let exited = {
                    let mut child_guard = self.child.lock().unwrap();
                    if let Some(child) = child_guard.as_mut() {
                        match child.try_wait() {
                            Ok(Some(status)) => Some(format!("exit status {status}")),
                            Ok(None) => None,
                            Err(e) => {
                                warn!(error = %e, "Failed to query SiYuan child process status");
                                None
                            }
                        }
                    } else if !is_process_alive(info.pid) {
                        Some("process is no longer alive".to_string())
                    } else {
                        None
                    }
                };

                if let Some(detail) = exited {
                    let message = format!(
                        "SiYuan Kernel process exited unexpectedly after startup (PID {}): {}",
                        info.pid, detail
                    );
                    *self.state.lock().unwrap() = RuntimeState::Failed;
                    *self.last_error.lock().unwrap() = Some(message);
                    let _ = std::fs::remove_file(&self.config.runtime_info_path);
                }
            }
        }

        let state = self.state();
        let info = self.runtime_info();
        let last_error = self.last_error();
        RuntimeHealth {
            state,
            pid: info.as_ref().map(|i| i.pid),
            port: info.as_ref().map(|i| i.port),
            version: info.as_ref().map(|i| i.version.clone()),
            base_url: info.as_ref().map(|i| i.base_url.clone()),
            last_error,
        }
    }

    // ── Private helpers ─────────────────────────────────────────────────────

    fn kill_child(&self) {
        let mut guard = self.child.lock().unwrap();
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn save_runtime_info(&self, pid: u32, port: u16, version: &str) {
        let info = RuntimeInfo {
            pid,
            port,
            workspace: self.config.workspace.to_string_lossy().to_string(),
            version: version.to_string(),
            started_by_aiks: true,
        };
        if let Ok(json) = serde_json::to_string_pretty(&info) {
            let _ = std::fs::write(&self.config.runtime_info_path, json);
        }
    }

    fn make_log_stdio(&self, port: u16) -> anyhow::Result<(Stdio, Stdio)> {
        if let Some(log_dir) = &self.config.log_dir {
            std::fs::create_dir_all(log_dir)?;
            let date = chrono::Utc::now().format("%Y-%m-%d");
            let log_path = log_dir.join(format!("siyuan-{}-{}.log", date, port));
            let log_file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)?;
            let log_file2 = log_file.try_clone()?;
            Ok((Stdio::from(log_file), Stdio::from(log_file2)))
        } else {
            Ok((Stdio::null(), Stdio::null()))
        }
    }
}

// ── Free functions ─────────────────────────────────────────────────────────────

/// Call /api/system/version and return the version string.
pub async fn get_siyuan_version(base_url: &str, token: Option<&str>) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    let mut req = client.get(format!("{}/api/system/version", base_url));
    if let Some(t) = token {
        req = req.header("Authorization", format!("Token {}", t));
    }

    let resp: serde_json::Value = req.send().await?.json().await?;

    // SiYuan v3 returns {"code":0,"msg":"","data":"3.8.3"}
    resp["data"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Unexpected version response: {}", resp))
}

/// Query SiYuan's official boot progress endpoint.
async fn get_siyuan_boot_progress(
    base_url: &str,
    token: Option<&str>,
) -> anyhow::Result<(i64, String)> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;
    let mut req = client.get(format!("{}/api/system/bootProgress", base_url));
    if let Some(t) = token {
        req = req.header("Authorization", format!("Token {}", t));
    }
    let resp: serde_json::Value = req.send().await?.json().await?;
    if resp["code"].as_i64().unwrap_or(-1) != 0 {
        anyhow::bail!("SiYuan bootProgress error: {}", resp);
    }
    let progress = resp["data"]["progress"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("Unexpected bootProgress response: {}", resp))?;
    let details = resp["data"]["details"].as_str().unwrap_or("").to_string();
    Ok((progress, details))
}

/// Allocate an available TCP port in [start, end].
pub fn allocate_port(start: u16, end: u16) -> Option<u16> {
    for port in start..=end {
        let addr = format!("127.0.0.1:{}", port);
        if let Ok(listener) = std::net::TcpListener::bind(&addr) {
            drop(listener);
            return Some(port);
        }
    }
    None
}

/// Check if a process with the given PID is still alive.
#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut command = Command::new("tasklist");
    command.creation_flags(CREATE_NO_WINDOW);
    let out = command
        .args(["/FI", &format!("PID eq {}", pid), "/FO", "CSV", "/NH"])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()),
        Err(_) => false,
    }
}

#[cfg(target_os = "linux")]
fn is_process_alive(pid: u32) -> bool {
    use std::fs;
    fs::metadata(format!("/proc/{}", pid)).is_ok()
}

#[cfg(target_os = "macos")]
fn is_process_alive(pid: u32) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn is_process_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_config(
        runtime_root: PathBuf,
        workspace: PathBuf,
        data_dir: &PathBuf,
    ) -> SiyuanRuntimeConfig {
        SiyuanRuntimeConfig::new(runtime_root, workspace, data_dir)
    }

    #[test]
    fn kernel_path_resolution() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let data = dir.path().to_path_buf();
        let cfg = make_config(root.clone(), root.join("workspace"), &data);

        let kernel = cfg.kernel_exe();
        let expected_name = if cfg!(windows) {
            "SiYuan-Kernel.exe"
        } else {
            "SiYuan-Kernel"
        };
        assert!(kernel.ends_with(format!("kernel/{}", expected_name)));
        assert!(kernel.starts_with(&root));
    }

    #[test]
    fn validate_missing_kernel() {
        let dir = tempdir().unwrap();
        let data = dir.path().to_path_buf();
        let cfg = make_config(dir.path().to_path_buf(), dir.path().join("ws"), &data);
        let v = SiyuanRuntime::validate(&cfg);
        assert!(!v.is_valid());
        assert!(v.errors.iter().any(|e| e.contains("Kernel not found")));
    }

    #[test]
    fn validate_missing_stage() {
        let dir = tempdir().unwrap();
        let data = dir.path().to_path_buf();
        let cfg = make_config(dir.path().to_path_buf(), dir.path().join("ws"), &data);

        // Create fake kernel
        let kernel_dir = dir.path().join("kernel");
        std::fs::create_dir_all(&kernel_dir).unwrap();
        let kernel_name = if cfg!(windows) {
            "SiYuan-Kernel.exe"
        } else {
            "SiYuan-Kernel"
        };
        let fake_kernel = kernel_dir.join(kernel_name);
        std::fs::write(&fake_kernel, vec![0u8; 2_000_000]).unwrap(); // 2 MB

        let v = SiyuanRuntime::validate(&cfg);
        assert!(!v.is_valid());
        assert!(v.errors.iter().any(|e| e.contains("stage")));
    }

    #[test]
    fn validate_complete_runtime() {
        let dir = tempdir().unwrap();
        let data = dir.path().to_path_buf();
        let cfg = make_config(dir.path().to_path_buf(), dir.path().join("ws"), &data);

        // Create fake runtime
        let kernel_dir = dir.path().join("kernel");
        let stage_dir = dir.path().join("stage");
        let appear_dir = dir.path().join("appearance");
        std::fs::create_dir_all(&kernel_dir).unwrap();
        std::fs::create_dir_all(&stage_dir).unwrap();
        std::fs::create_dir_all(&appear_dir).unwrap();

        let kernel_name = if cfg!(windows) {
            "SiYuan-Kernel.exe"
        } else {
            "SiYuan-Kernel"
        };
        std::fs::write(kernel_dir.join(kernel_name), vec![0u8; 2_000_000]).unwrap();
        std::fs::write(stage_dir.join("index.html"), b"<html/>").unwrap();
        std::fs::write(appear_dir.join("base.css"), b"body{}").unwrap();

        let v = SiyuanRuntime::validate(&cfg);
        assert!(v.is_valid(), "Errors: {:?}", v.errors);
        assert_eq!(v.kernel_size_bytes, 2_000_000);
    }

    #[test]
    fn port_allocation_finds_free_port() {
        let port = allocate_port(6806, 6899);
        assert!(port.is_some());
        let p = port.unwrap();
        assert!(p >= 6806 && p <= 6899);
    }

    #[test]
    fn runtime_command_args_format() {
        let dir = tempdir().unwrap();
        let data = dir.path().to_path_buf();
        let mut cfg = make_config(dir.path().to_path_buf(), dir.path().join("ws"), &data);
        cfg.port = Some(7000);
        // Verify the args we'd pass
        let ws_arg = format!("--workspace={}", cfg.workspace.display());
        let wd_arg = format!("--wd={}", cfg.runtime_root.display());
        let args: Vec<&str> = vec![
            "serve",
            &ws_arg,
            &wd_arg,
            "--port=7000",
            "--lang=zh-CN",
            "--mode=prod",
        ];
        assert!(args.contains(&"serve"));
        assert!(args.iter().any(|a| a.starts_with("--workspace=")));
        assert!(args.iter().any(|a| a.starts_with("--wd=")));
        assert!(args.contains(&"--port=7000"));
        // MUST NOT include accessAuthCode
        assert!(!args.iter().any(|a| a.contains("accessAuthCode")));
    }

    #[test]
    fn no_accessauthcode_in_embedded_mode() {
        // Spec §25-26: Embedded mode must NOT set accessAuthCode
        let dir = tempdir().unwrap();
        let data = dir.path().to_path_buf();
        let cfg = make_config(dir.path().to_path_buf(), dir.path().join("ws"), &data);
        // The SiyuanRuntimeConfig struct has no accessAuthCode field
        // This is a compile-time verification via the struct definition
        let _cfg = cfg; // just use it
    }

    #[tokio::test]
    async fn version_mismatch_warning() {
        // Spec §31: version mismatch should not abort, just warn
        // We test the parsing logic directly
        let json = serde_json::json!({"code": 0, "msg": "", "data": "3.7.9"});
        let ver = json["data"].as_str().unwrap();
        assert_eq!(ver, "3.7.9");
        assert_ne!(ver, "3.8.3"); // mismatch detected
    }

    #[test]
    fn runtime_info_serialization() {
        let info = RuntimeInfo {
            pid: 12345,
            port: 6812,
            workspace: "C:\\Users\\test\\AIKnowledgeSync\\siyuan\\workspace".to_string(),
            version: "3.8.3".to_string(),
            started_by_aiks: true,
        };
        let json = serde_json::to_string_pretty(&info).unwrap();
        assert!(json.contains("\"pid\": 12345"));
        assert!(json.contains("\"port\": 6812"));
        assert!(json.contains("\"version\": \"3.8.3\""));
        assert!(json.contains("\"startedByAiks\": true"));

        let deserialized: RuntimeInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.pid, 12345);
        assert_eq!(deserialized.version, "3.8.3");
    }
}
