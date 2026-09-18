from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{path}: expected exactly one anchor, found {count}: {old[:120]!r}")
    write(path, text.replace(old, new, 1))


def regex_once(path: str, pattern: str, replacement: str) -> None:
    text = read(path)
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.S)
    if count != 1:
        raise RuntimeError(f"{path}: regex anchor not found: {pattern[:160]}")
    write(path, updated)


# ---------------------------------------------------------------------------
# Repo-hygiene-safe documentation wording
# ---------------------------------------------------------------------------
for doc in [
    "docs/superpowers/specs/2026-09-18-aiks-v4-2-live-pipeline-settings-diagnostics-design.md",
    "docs/superpowers/plans/2026-09-18-aiks-v4-2-live-pipeline-settings-diagnostics.md",
]:
    text = read(doc)
    text = text.replace(
        "`http://10.10.23.16:18000/v1` and model `Qwen3.8-27B`",
        "the managed private endpoint defined by `AiModelConfig::default()` and model `Qwen3.8-27B`",
    )
    write(doc, text)


# ---------------------------------------------------------------------------
# Core config: desktop-only settings live in the same typed TOML source
# ---------------------------------------------------------------------------
replace_once(
    "crates/aiks-core/src/config/mod.rs",
    "    pub embedding: EmbeddingConfig,\n}",
    "    pub embedding: EmbeddingConfig,\n    pub desktop: DesktopConfig,\n}",
)
replace_once(
    "crates/aiks-core/src/config/mod.rs",
    "            embedding: EmbeddingConfig::default(),\n        }",
    "            embedding: EmbeddingConfig::default(),\n            desktop: DesktopConfig::default(),\n        }",
)
replace_once(
    "crates/aiks-core/src/config/mod.rs",
    "#[derive(Debug, Clone, Serialize, Deserialize)]\n#[serde(default)]\npub struct SyncConfig {",
    "#[derive(Debug, Clone, Serialize, Deserialize)]\n#[serde(default)]\npub struct DesktopConfig {\n    pub startup: bool,\n    pub close_to_tray: bool,\n}\n\nimpl Default for DesktopConfig {\n    fn default() -> Self {\n        Self {\n            startup: true,\n            close_to_tray: true,\n        }\n    }\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\n#[serde(default)]\npub struct SyncConfig {",
)


# ---------------------------------------------------------------------------
# SyncEngine: surface each candidate immediately after its raw write
# ---------------------------------------------------------------------------
regex_once(
    "crates/aiks-core/src/sync/engine.rs",
    r"    fn record_extraction_candidate\(.*?\n    \}\n\n    /// Run a full sync cycle\.",
    '''    fn record_extraction_candidate(
        &self,
        db: &StateDb,
        summary: &SessionSummary,
        opts: &SyncOptions,
        stats: &mut SyncStats,
    ) -> Option<ExtractionCandidate> {
        // A dry-run must never create executable follow-up work. New dry-run
        // sessions do not even have a canonical source_session row yet.
        if opts.dry_run {
            return None;
        }

        match SourceSessionRepo::new(db)
            .find_by_source_and_id(summary.source.as_str(), &summary.external_session_id)
        {
            Ok(Some(session)) => {
                let candidate = ExtractionCandidate {
                    session_id: session.id,
                    source: session.source,
                    external_session_id: session.external_session_id,
                };
                stats.extraction_candidates.push(candidate.clone());
                Some(candidate)
            }
            Ok(None) => {
                warn!(
                    source = %summary.source.as_str(),
                    external_session_id = %summary.external_session_id,
                    "[PIPELINE] Synced session missing canonical row; not enqueueing extraction"
                );
                None
            }
            Err(e) => {
                warn!(
                    source = %summary.source.as_str(),
                    external_session_id = %summary.external_session_id,
                    error = %e,
                    "[PIPELINE] Failed to resolve canonical extraction candidate"
                );
                None
            }
        }
    }

    /// Run a full sync cycle.''',
)
replace_once(
    "crates/aiks-core/src/sync/engine.rs",
    '''    pub async fn run_sync(
        &self,
        db: &StateDb,
        registry: &ProviderRegistry,
        sink: &SiYuanSink,
        opts: &SyncOptions,
    ) -> anyhow::Result<SyncStats> {
        let run_repo = SyncRunRepo::new(db);''',
    '''    pub async fn run_sync(
        &self,
        db: &StateDb,
        registry: &ProviderRegistry,
        sink: &SiYuanSink,
        opts: &SyncOptions,
    ) -> anyhow::Result<SyncStats> {
        self.run_sync_with_candidate_handler(db, registry, sink, opts, |_| {})
            .await
    }

    /// Run a full sync cycle and synchronously notify the caller as soon as
    /// each Created/Updated session becomes eligible for the knowledge pipeline.
    pub async fn run_sync_with_candidate_handler<F>(
        &self,
        db: &StateDb,
        registry: &ProviderRegistry,
        sink: &SiYuanSink,
        opts: &SyncOptions,
        mut on_candidate: F,
    ) -> anyhow::Result<SyncStats>
    where
        F: FnMut(&ExtractionCandidate),
    {
        let run_repo = SyncRunRepo::new(db);''',
)
replace_once(
    "crates/aiks-core/src/sync/engine.rs",
    "                    self.record_extraction_candidate(db, summary, opts, &mut stats);",
    "                    if let Some(candidate) =\n                        self.record_extraction_candidate(db, summary, opts, &mut stats)\n                    {\n                        on_candidate(&candidate);\n                    }",
)
# second occurrence (Updated)
replace_once(
    "crates/aiks-core/src/sync/engine.rs",
    "                    self.record_extraction_candidate(db, summary, opts, &mut stats);",
    "                    if let Some(candidate) =\n                        self.record_extraction_candidate(db, summary, opts, &mut stats)\n                    {\n                        on_candidate(&candidate);\n                    }",
)


# ---------------------------------------------------------------------------
# AiksEngine: use live callback only for sync_and_enqueue_extraction
# ---------------------------------------------------------------------------
replace_once(
    "crates/aiks-core/src/engine/mod.rs",
    "use crate::sync::{SyncEngine, SyncOptions, SyncStats};",
    "use crate::sync::{ExtractionCandidate, SyncEngine, SyncOptions, SyncStats};",
)
replace_once(
    "crates/aiks-core/src/engine/mod.rs",
    '''    /// Sync implementation — caller must hold `sync_lock`.
    async fn sync_unlocked(&self, opts: SyncOptions) -> anyhow::Result<SyncStats> {
        let sink = if let Some(token) = &self.siyuan_token {
            // External mode: use token from CLI config
            let mut cfg = self.config.siyuan.clone();
            cfg.base_url = self.siyuan_base_url.clone();
            cfg.token = token.clone();
            SiYuanSink::new(cfg)?
        } else {
            // Embedded mode: no token required
            SiYuanSink::embedded(&self.siyuan_base_url, &self.config.siyuan.notebook_name)?
        };
        self.sync_engine
            .run_sync(&self.db, &self.registry, &sink, &opts)
            .await
    }''',
    '''    /// Sync implementation — caller must hold `sync_lock`.
    async fn sync_unlocked(&self, opts: SyncOptions) -> anyhow::Result<SyncStats> {
        self.sync_unlocked_with_candidate_handler(opts, |_| {}).await
    }

    /// Sync implementation with a live per-session candidate callback.
    /// The callback is synchronous on purpose: it may persist/submit a durable
    /// PipelineJob, but it must not block raw synchronization on AI inference.
    async fn sync_unlocked_with_candidate_handler<F>(
        &self,
        opts: SyncOptions,
        on_candidate: F,
    ) -> anyhow::Result<SyncStats>
    where
        F: FnMut(&ExtractionCandidate),
    {
        let sink = if let Some(token) = &self.siyuan_token {
            // External mode: use token from CLI config
            let mut cfg = self.config.siyuan.clone();
            cfg.base_url = self.siyuan_base_url.clone();
            cfg.token = token.clone();
            SiYuanSink::new(cfg)?
        } else {
            // Embedded mode: no token required
            SiYuanSink::embedded(&self.siyuan_base_url, &self.config.siyuan.notebook_name)?
        };
        self.sync_engine
            .run_sync_with_candidate_handler(
                &self.db,
                &self.registry,
                &sink,
                &opts,
                on_candidate,
            )
            .await
    }''',
)
insert_anchor = '''    /// Search distilled knowledge while preserving degraded-state/error semantics.
    pub async fn search_knowledge('''
helper = '''    fn submit_pipeline_candidate(
        db: &Arc<StateDb>,
        pipeline_worker: &Arc<PipelineWorker>,
        candidate: &ExtractionCandidate,
    ) {
        // Resolve by canonical DB identity and verify the redundant source
        // identity. This prevents cross-provider external-ID collisions.
        let session_data: Option<(
            i64,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = {
            let conn = db.conn();
            conn.query_row(
                "SELECT id, source, external_session_id, title, project_name, content_hash
                 FROM source_session
                 WHERE id = ?1 AND source = ?2 AND external_session_id = ?3",
                rusqlite::params![
                    candidate.session_id,
                    candidate.source,
                    candidate.external_session_id
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .ok()
        };

        let Some((db_id, source, session_ext_id, title, project_name, content_hash)) = session_data
        else {
            tracing::warn!(
                session_id = candidate.session_id,
                "[PIPELINE] Live candidate no longer resolves to canonical session"
            );
            return;
        };

        let orchestrator = PipelineOrchestrator::new(Arc::clone(db));
        match orchestrator.enqueue(db_id, content_hash.as_deref()) {
            Ok(run_id) => {
                if let Err(e) = pipeline_worker.submit(PipelineJob {
                    pipeline_run_id: run_id,
                    session_id: db_id,
                    session_external_id: session_ext_id,
                    source,
                    session_title: title,
                    project_name,
                }) {
                    tracing::warn!(
                        session_id = db_id,
                        error = %e,
                        "[PIPELINE] Durable live enqueue failed"
                    );
                }
            }
            Err(e) => tracing::warn!(
                session_id = db_id,
                error = %e,
                "[PIPELINE] Could not create live pipeline run"
            ),
        }
    }

'''
replace_once("crates/aiks-core/src/engine/mod.rs", insert_anchor, helper + insert_anchor)
replace_once(
    "crates/aiks-core/src/engine/mod.rs",
    "            let stats = self.sync_unlocked(opts.clone()).await?;",
    '''            let enqueue_live = !opts.dry_run
                && self.config.ai.enabled
                && self.config.ai.auto_extract;
            let db = Arc::clone(&self.db);
            let pipeline_worker = Arc::clone(&self.pipeline_worker);
            let stats = self
                .sync_unlocked_with_candidate_handler(opts.clone(), move |candidate| {
                    if enqueue_live {
                        Self::submit_pipeline_candidate(&db, &pipeline_worker, candidate);
                    }
                })
                .await?;''',
)
regex_once(
    "crates/aiks-core/src/engine/mod.rs",
    r"\n            // B09/R09: Enqueue new/updated sessions into V3 pipeline — respecting.*?\n            stats\n",
    "\n            stats\n",
)


# ---------------------------------------------------------------------------
# Workbench state: origin availability != actual mounting != bridge readiness
# ---------------------------------------------------------------------------
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/controller.rs",
    "    pub available: bool,\n    pub ready: bool,",
    "    pub available: bool,\n    pub mounted: bool,\n    pub ready: bool,",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/controller.rs",
    "    ready: AtomicBool,\n    mode: Mutex<WorkspaceMode>,",
    "    mounted: AtomicBool,\n    ready: AtomicBool,\n    mode: Mutex<WorkspaceMode>,",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/controller.rs",
    "            nonce: Uuid::new_v4().to_string(),\n            ready: AtomicBool::new(false),",
    "            nonce: Uuid::new_v4().to_string(),\n            mounted: AtomicBool::new(false),\n            ready: AtomicBool::new(false),",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/controller.rs",
    "            *current = Some(origin);\n            self.ready.store(false, Ordering::Release);",
    "            *current = Some(origin);\n            self.mounted.store(false, Ordering::Release);\n            self.ready.store(false, Ordering::Release);",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/controller.rs",
    "        *lock_recover(&self.origin) = None;\n        self.ready.store(false, Ordering::Release);",
    "        *lock_recover(&self.origin) = None;\n        self.mounted.store(false, Ordering::Release);\n        self.ready.store(false, Ordering::Release);",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/controller.rs",
    '''    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::Release);
    }
''',
    '''    pub fn set_mounted(&self, mounted: bool) {
        self.mounted.store(mounted, Ordering::Release);
        if !mounted {
            self.ready.store(false, Ordering::Release);
        }
    }

    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::Release);
    }
''',
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/controller.rs",
    "            available: origin.is_some(),\n            ready: self.ready.load(Ordering::Acquire),",
    "            available: origin.is_some(),\n            mounted: self.mounted.load(Ordering::Acquire),\n            ready: self.ready.load(Ordering::Acquire),",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/controller.rs",
    "        assert!(!status.available);\n        assert!(!status.ready);",
    "        assert!(!status.available);\n        assert!(!status.mounted);\n        assert!(!status.ready);",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/controller.rs",
    "        controller.set_ready(true);\n        controller.set_mode(WorkspaceMode::Session);",
    "        controller.set_mounted(true);\n        controller.set_ready(true);\n        controller.set_mode(WorkspaceMode::Session);",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/controller.rs",
    "        assert!(status.available);\n        assert!(status.ready);",
    "        assert!(status.available);\n        assert!(status.mounted);\n        assert!(status.ready);",
)

# Existing child webview mount path
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/commands.rs",
    '''        webview.show().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let parent = app''',
    '''        webview.show().map_err(|e| e.to_string())?;
        controller.set_mounted(true);
        return Ok(());
    }

    let parent = app''',
)
# Newly-created child mount path
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/commands.rs",
    '''    webview.show().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn show_workbench(''',
    '''    webview.show().map_err(|e| e.to_string())?;
    controller.set_mounted(true);
    Ok(())
}

#[tauri::command]
pub async fn show_workbench(''',
)
# Existing child show path
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/commands.rs",
    '''        webview.show().map_err(|e| e.to_string())?;
        return webview.set_focus().map_err(|e| e.to_string());
    }
''',
    '''        webview.show().map_err(|e| e.to_string())?;
        controller.set_mounted(true);
        return webview.set_focus().map_err(|e| e.to_string());
    }
''',
)
# Fallback show path
replace_once(
    "apps/aiks-desktop/src-tauri/src/workbench/commands.rs",
    '''    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}''',
    '''    window.show().map_err(|e| e.to_string())?;
    controller.set_mounted(true);
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}''',
)

# Diagnostics test fixture now includes mounted state
replace_once(
    "apps/aiks-desktop/src-tauri/src/diagnostics.rs",
    "            available: true,\n            ready: true,",
    "            available: true,\n            mounted: true,\n            ready: true,",
)

# TS transport type + mock
replace_once(
    "apps/aiks-desktop/src/api/types.ts",
    "  available: boolean;\n  ready: boolean;",
    "  available: boolean;\n  mounted: boolean;\n  ready: boolean;",
)
replace_once(
    "apps/aiks-desktop/src/api/mock.ts",
    "      available: true,\n      ready: true,",
    "      available: true,\n      mounted: true,\n      ready: true,",
)


# ---------------------------------------------------------------------------
# Diagnostics UI: V4.2 and three truthful layers
# ---------------------------------------------------------------------------
replace_once(
    "apps/aiks-desktop/src/pages/DiagnosticsPage.tsx",
    '''  const workbenchStatus = !v41?.workbench.available
    ? "不可用"
    : v41.workbench.ready
      ? "Ready"
      : "等待 Bridge";
  const workbenchHealthy = Boolean(v41?.workbench.available && v41.workbench.ready);
  const workspaceMode = v41?.workbench.mode === "session" ? "原始会话" : "知识";''',
    '''  const workbenchMounted = Boolean(v41?.workbench.mounted);
  const workbenchStatus = !v41?.siyuan_ready
    ? "不可用"
    : workbenchMounted
      ? "已挂载"
      : "未挂载";
  const bridgeStatus = !v41?.siyuan_ready
    ? "不可用"
    : !workbenchMounted
      ? "未挂载"
      : v41.workbench.ready
        ? "已连接"
        : "等待 Bridge";
  const workbenchHealthy = Boolean(v41?.siyuan_ready && workbenchMounted);
  const bridgeHealthy = Boolean(workbenchHealthy && v41?.workbench.ready);
  const workspaceMode = v41?.workbench.mode === "session" ? "原始会话" : "知识";''',
)
replace_once(
    "apps/aiks-desktop/src/pages/DiagnosticsPage.tsx",
    "<h2 className=\"text-sm font-semibold\">V4.1 知识工作台</h2>",
    "<h2 className=\"text-sm font-semibold\">V4.2 知识工作台</h2>",
)
replace_once(
    "apps/aiks-desktop/src/pages/DiagnosticsPage.tsx",
    '''            <div className={`text-sm font-medium mt-1 ${workbenchHealthy ? "text-green-600" : "text-gray-500"}`}>
              {v41 ? `Protocol v${v41.workbench.protocol_version}` : "检测中"}
            </div>''',
    '''            <div className={`text-sm font-medium mt-1 ${bridgeHealthy ? "text-green-600" : v41 ? "text-yellow-500" : "text-gray-400"}`}>
              {v41 ? `${bridgeStatus}${workbenchMounted ? ` · Protocol v${v41.workbench.protocol_version}` : ""}` : "检测中"}
            </div>''',
)


# ---------------------------------------------------------------------------
# Processing Center: live polling while visible, no loading flicker on polls
# ---------------------------------------------------------------------------
replace_once(
    "apps/aiks-desktop/src/pages/ProcessingPage.tsx",
    '''  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [r, s] = await Promise.all([
        getApi().getPipelineRuns(300),
        getApi().getPipelineStats(),
      ]);
      setRuns(r);
      setStats(s);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);''',
    '''  const load = useCallback(async (showLoading = false) => {
    if (showLoading) setLoading(true);
    try {
      const [r, s] = await Promise.all([
        getApi().getPipelineRuns(300),
        getApi().getPipelineStats(),
      ]);
      setRuns(r);
      setStats(s);
    } finally {
      if (showLoading) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load(true);
    const poll = () => {
      if (document.visibilityState === "visible") void load(false);
    };
    const interval = window.setInterval(poll, 2000);
    const onVisibilityChange = () => {
      if (document.visibilityState === "visible") void load(false);
    };
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      window.clearInterval(interval);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, [load]);''',
)
replace_once(
    "apps/aiks-desktop/src/pages/ProcessingPage.tsx",
    "      await load();",
    "      await load(false);",
)
replace_once(
    "apps/aiks-desktop/src/pages/ProcessingPage.tsx",
    '''          <button onClick={load} className="text-xs px-3 py-1.5 border border-gray-200 rounded hover:bg-gray-50 dark:border-gray-700 dark:hover:bg-gray-700">''',
    '''          <button onClick={() => void load(true)} className="text-xs px-3 py-1.5 border border-gray-200 rounded hover:bg-gray-50 dark:border-gray-700 dark:hover:bg-gray-700">''',
)


# ---------------------------------------------------------------------------
# AppState + lifecycle: real close-to-tray state loaded from Config
# ---------------------------------------------------------------------------
replace_once(
    "apps/aiks-desktop/src-tauri/src/app_state.rs",
    "use std::path::PathBuf;\nuse std::sync::Arc;",
    "use std::path::PathBuf;\nuse std::sync::{atomic::AtomicBool, Arc};",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/app_state.rs",
    "    /// User data directory\n    pub data_dir: PathBuf,",
    "    /// User data directory\n    pub data_dir: PathBuf,\n    /// Whether closing the control window should keep AIKS in the tray.\n    pub close_to_tray: AtomicBool,",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/app_state.rs",
    '''pub fn config_file_path() -> PathBuf {
    data_dir().join("config").join("aiks.toml")
}
''',
    '''pub fn config_file_path() -> PathBuf {
    data_dir().join("config").join("aiks.toml")
}

/// Resolve the persisted desktop close behavior before AppState exists.
pub fn close_to_tray_setting() -> bool {
    let path = config_file_path();
    if path.exists() {
        aiks_core::Config::from_file(&path)
            .map(|config| config.desktop.close_to_tray)
            .unwrap_or(true)
    } else {
        aiks_core::Config::default().desktop.close_to_tray
    }
}
''',
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/lifecycle.rs",
    "use crate::app_state::{config_file_path, data_dir, AppState};",
    "use crate::app_state::{close_to_tray_setting, config_file_path, data_dir, AppState};",
)
# Add close flag to every AppState literal in lifecycle.
lifecycle = read("apps/aiks-desktop/src-tauri/src/lifecycle.rs")
needle = "        data_dir,\n        _watcher_handle:"
count = lifecycle.count(needle)
if count != 4:
    raise RuntimeError(f"lifecycle AppState anchor count changed: {count}")
lifecycle = lifecycle.replace(
    needle,
    "        data_dir,\n        close_to_tray: std::sync::atomic::AtomicBool::new(close_to_tray_setting()),\n        _watcher_handle:",
)
write("apps/aiks-desktop/src-tauri/src/lifecycle.rs", lifecycle)


# ---------------------------------------------------------------------------
# Settings commands: real typed Config source + real desktop controls
# ---------------------------------------------------------------------------
replace_once(
    "apps/aiks-desktop/src-tauri/src/commands.rs",
    "use std::sync::Arc;",
    "use std::sync::{atomic::Ordering, Arc};",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/commands.rs",
    "use tauri::State;",
    "use tauri::{AppHandle, State};\nuse tauri_plugin_autostart::ManagerExt;",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/commands.rs",
    "use crate::app_state::AppState;",
    "use crate::app_state::{config_file_path, AppState};",
)
regex_once(
    "apps/aiks-desktop/src-tauri/src/commands.rs",
    r"#\[derive\(Serialize, Deserialize, Clone\)\]\npub struct AppSettings \{.*?\n\}\n\nimpl Default for AppSettings \{.*?\n\}\n",
    '''#[derive(Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub startup: bool,
    pub close_to_tray: bool,
    pub sync_enabled: bool,
    pub scan_interval_seconds: u64,
    pub include_thinking: bool,
    pub include_tool_calls: bool,
    pub max_tool_result_chars: usize,
    pub redact_secrets: bool,
    pub ai_enabled: bool,
    pub ai_auto_extract: bool,
    pub ai_base_url: String,
    pub ai_model: String,
}

impl AppSettings {
    fn from_config(config: &aiks_core::Config, startup: bool, close_to_tray: bool) -> Self {
        Self {
            startup,
            close_to_tray,
            sync_enabled: config.sync.watch_enabled,
            scan_interval_seconds: config.sync.scan_interval_seconds,
            include_thinking: config.content.include_thinking,
            include_tool_calls: config.content.include_tool_calls,
            max_tool_result_chars: config.content.max_tool_result_chars,
            redact_secrets: config.security.redact_secrets,
            ai_enabled: config.ai.enabled,
            ai_auto_extract: config.ai.auto_extract,
            ai_base_url: config.ai.base_url.clone(),
            ai_model: config.ai.model.clone(),
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        let config = aiks_core::Config::default();
        Self::from_config(
            &config,
            config.desktop.startup,
            config.desktop.close_to_tray,
        )
    }
}
''',
)
regex_once(
    "apps/aiks-desktop/src-tauri/src/commands.rs",
    r"#\[tauri::command\]\npub async fn get_settings\(.*?\n\}\n\n/// B10 \+ R07: save_settings.*?\n#\[tauri::command\]\npub async fn open_data_folder",
    '''fn load_settings_config() -> Result<aiks_core::Config, String> {
    let path = config_file_path();
    if path.exists() {
        aiks_core::Config::from_file(&path)
            .map_err(|e| format!("Failed to load aiks.toml: {e}"))
    } else {
        Ok(aiks_core::Config::default())
    }
}

fn apply_settings_to_config(config: &mut aiks_core::Config, settings: &AppSettings) {
    config.desktop.startup = settings.startup;
    config.desktop.close_to_tray = settings.close_to_tray;
    config.ai.enabled = settings.ai_enabled;
    config.ai.auto_extract = settings.ai_auto_extract;
    config.ai.base_url = settings.ai_base_url.trim().trim_end_matches('/').to_string();
    config.ai.model = settings.ai_model.trim().to_string();
    config.sync.scan_interval_seconds = settings.scan_interval_seconds;
    config.sync.watch_enabled = settings.sync_enabled;
    config.security.redact_secrets = settings.redact_secrets;
    config.content.include_thinking = settings.include_thinking;
    config.content.include_tool_calls = settings.include_tool_calls;
    config.content.max_tool_result_chars = settings.max_tool_result_chars;
}

fn persist_settings_config(config: &aiks_core::Config) -> Result<(), String> {
    let path = config_file_path();
    let parent = path.parent().ok_or("Invalid config path")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let content = toml::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    let tmp = parent.join("aiks.toml.tmp");
    std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
    if let Err(first_error) = std::fs::rename(&tmp, &path) {
        // Windows does not replace an existing destination with rename().
        // Retry with a short replace fallback so repeated Settings saves work.
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| {
                format!("Failed to replace existing config after {first_error}: {e}")
            })?;
            std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        } else {
            return Err(first_error.to_string());
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_settings(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    let config = load_settings_config()?;
    let startup = app
        .autolaunch()
        .is_enabled()
        .unwrap_or(config.desktop.startup);
    let close_to_tray = state.close_to_tray.load(Ordering::Acquire);
    Ok(AppSettings::from_config(&config, startup, close_to_tray))
}

/// Persist the complete typed config while mutating only fields exposed by Settings.
#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if settings.ai_base_url.trim().is_empty() {
        return Err("AI 服务地址不能为空".to_string());
    }
    if settings.ai_model.trim().is_empty() {
        return Err("AI 模型不能为空".to_string());
    }

    let mut config = load_settings_config()?;
    apply_settings_to_config(&mut config, &settings);
    persist_settings_config(&config)?;

    let autostart = app.autolaunch();
    if settings.startup {
        autostart.enable().map_err(|e| e.to_string())?;
    } else {
        autostart.disable().map_err(|e| e.to_string())?;
    }
    state
        .close_to_tray
        .store(settings.close_to_tray, Ordering::Release);
    Ok(())
}

#[cfg(test)]
mod settings_mapping_tests {
    use super::*;

    #[test]
    fn settings_follow_the_real_ai_config_and_preserve_unexposed_fields() {
        let mut config = aiks_core::Config::default();
        config.embedding.enabled = true;
        config.ai.api_key = Some("preserve-me".to_string());
        let defaults = AppSettings::from_config(
            &config,
            config.desktop.startup,
            config.desktop.close_to_tray,
        );
        assert_eq!(defaults.ai_base_url, config.ai.base_url);
        assert_eq!(defaults.ai_model, config.ai.model);

        let mut edited = defaults;
        edited.ai_base_url = "http://example.invalid/v1".to_string();
        edited.ai_model = "model-from-settings".to_string();
        edited.close_to_tray = false;
        apply_settings_to_config(&mut config, &edited);

        assert_eq!(config.ai.base_url, "http://example.invalid/v1");
        assert_eq!(config.ai.model, "model-from-settings");
        assert!(!config.desktop.close_to_tray);
        assert!(config.embedding.enabled);
        assert_eq!(config.ai.api_key.as_deref(), Some("preserve-me"));
    }
}

#[tauri::command]
pub async fn open_data_folder''',
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/commands.rs",
    '''#[tauri::command]
pub async fn test_ai_connection(state: State<'_, AppState>) -> Result<bool, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    Ok(engine.ai_health_check().await)
}
''',
    '''#[tauri::command]
pub async fn test_ai_connection(state: State<'_, AppState>) -> Result<bool, String> {
    let engine = state.engine().ok_or("Engine not initialized")?;
    Ok(engine.ai_health_check().await)
}

/// Test the endpoint/model currently typed in Settings, not the engine's
/// in-memory configuration from before the last save.
#[tauri::command]
pub async fn test_ai_connection_with_settings(
    base_url: String,
    model: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    if base_url.trim().is_empty() || model.trim().is_empty() {
        return Ok(false);
    }
    let mut config = state
        .engine()
        .map(|engine| engine.ai_config().clone())
        .unwrap_or_default();
    config.enabled = true;
    config.base_url = base_url.trim().trim_end_matches('/').to_string();
    config.model = model.trim().to_string();
    let client = aiks_core::ai::AiClient::new(config).map_err(|e| e.to_string())?;
    Ok(client.health_check().await)
}
''',
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/commands.rs",
    '''#[tauri::command]
pub async fn restart_siyuan(''',
    '''#[tauri::command]
pub async fn restart_app(app: AppHandle) -> Result<(), String> {
    crate::lifecycle::shutdown(&app).await;
    app.restart()
}

#[tauri::command]
pub async fn restart_siyuan(''',
)


# ---------------------------------------------------------------------------
# Tauri lib: register new commands and obey close_to_tray
# ---------------------------------------------------------------------------
replace_once(
    "apps/aiks-desktop/src-tauri/src/lib.rs",
    "use std::sync::Arc;",
    "use std::sync::{atomic::Ordering, Arc};",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/lib.rs",
    "            commands::test_ai_connection,",
    "            commands::test_ai_connection,\n            commands::test_ai_connection_with_settings,",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/lib.rs",
    "            commands::restart_siyuan,",
    "            commands::restart_app,\n            commands::restart_siyuan,",
)
replace_once(
    "apps/aiks-desktop/src-tauri/src/lib.rs",
    '''        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                window.hide().ok();
                api.prevent_close();
            }
        })''',
    '''        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let close_to_tray = window
                    .app_handle()
                    .try_state::<app_state::AppState>()
                    .map(|state| state.close_to_tray.load(Ordering::Acquire))
                    .unwrap_or(true);

                if close_to_tray || window.label() != "control" {
                    window.hide().ok();
                    api.prevent_close();
                } else {
                    api.prevent_close();
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        lifecycle::shutdown(&app).await;
                        app.exit(0);
                    });
                }
            }
        })''',
)


# ---------------------------------------------------------------------------
# Settings UI: one backend DTO, no fake toggles, real test/save/restart semantics
# ---------------------------------------------------------------------------
settings_page = r'''import { useEffect, useState } from "react";
import { CheckCircle } from "lucide-react";
import { shouldUseMock } from "../api/client";

interface Settings {
  startup: boolean;
  close_to_tray: boolean;
  sync_enabled: boolean;
  scan_interval_seconds: number;
  include_thinking: boolean;
  include_tool_calls: boolean;
  max_tool_result_chars: number;
  redact_secrets: boolean;
  ai_enabled: boolean;
  ai_auto_extract: boolean;
  ai_base_url: string;
  ai_model: string;
}

const MOCK_SETTINGS: Settings = {
  startup: true,
  close_to_tray: true,
  sync_enabled: true,
  scan_interval_seconds: 300,
  include_thinking: false,
  include_tool_calls: true,
  max_tool_result_chars: 10000,
  redact_secrets: true,
  ai_enabled: true,
  ai_auto_extract: true,
  ai_base_url: "http://localhost:11434/v1",
  ai_model: "Qwen3.8-27B",
};

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saved, setSaved] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [aiHealthy, setAiHealthy] = useState<boolean | null>(null);
  const [aiTesting, setAiTesting] = useState(false);
  const isMock = shouldUseMock();

  useEffect(() => {
    if (isMock) {
      setSettings(MOCK_SETTINGS);
      return;
    }
    import("@tauri-apps/api/core").then(({ invoke }) => {
      invoke<Settings>("get_settings").then(setSettings).catch(error => setSaveError(String(error)));
    });
  }, [isMock]);

  const testAiConnection = async () => {
    if (!settings) return;
    setAiHealthy(null);
    setAiTesting(true);
    try {
      if (isMock) {
        setAiHealthy(true);
        return;
      }
      const { invoke } = await import("@tauri-apps/api/core");
      const ok = await invoke<boolean>("test_ai_connection_with_settings", {
        baseUrl: settings.ai_base_url,
        model: settings.ai_model,
      });
      setAiHealthy(ok);
    } catch {
      setAiHealthy(false);
    } finally {
      setAiTesting(false);
    }
  };

  const save = async (restart: boolean) => {
    if (!settings) return;
    setSaveError("");
    try {
      if (!isMock) {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("save_settings", { settings });
        if (restart) {
          await invoke("restart_app");
          return;
        }
      }
      setSaved(true);
      window.setTimeout(() => setSaved(false), 2000);
    } catch (error) {
      setSaveError(String(error));
    }
  };

  const update = (key: keyof Settings, value: boolean | number | string) => {
    setSettings(current => current ? { ...current, [key]: value } : current);
  };

  if (!settings) return <div className="p-6 text-gray-400 text-sm">加载中...</div>;

  return (
    <div className="p-6 max-w-xl">
      <div className="mb-6">
        <h1 className="text-xl font-semibold">设置</h1>
        <p className="mt-1 text-xs text-gray-400">桌面行为保存后立即生效；AI、同步和内容配置将在重启 AIKS 后生效。</p>
      </div>

      <Section title="常规">
        <Toggle label="开机自动启动" desc="登录后自动在后台运行" value={settings.startup} onChange={value => update("startup", value)} />
        <Toggle label="关闭窗口后驻留后台" desc="关闭主窗口时保留托盘与后台同步" value={settings.close_to_tray} onChange={value => update("close_to_tray", value)} />
      </Section>

      <Section title="同步">
        <Toggle label="实时自动同步" desc="监测文件变化并自动同步" value={settings.sync_enabled} onChange={value => update("sync_enabled", value)} />
        <div className="flex items-center justify-between py-3">
          <div>
            <div className="text-sm">定时扫描间隔</div>
            <div className="text-xs text-gray-400">重启 AIKS 后生效</div>
          </div>
          <select
            className="text-sm border border-gray-300 dark:border-gray-600 rounded px-2 py-1 bg-white dark:bg-gray-700"
            value={settings.scan_interval_seconds / 60}
            onChange={event => update("scan_interval_seconds", +event.target.value * 60)}
          >
            {[1, 5, 10, 15, 30].map(minutes => <option key={minutes} value={minutes}>{minutes} 分钟</option>)}
          </select>
        </div>
      </Section>

      <Section title="AI 智能整理">
        <Toggle label="启用智能整理" desc="使用配置的 AI 服务自动提炼知识" value={settings.ai_enabled} onChange={value => update("ai_enabled", value)} />
        <Toggle label="自动整理新会话" desc="Raw Session 同步成功后自动进入处理队列" value={settings.ai_auto_extract} onChange={value => update("ai_auto_extract", value)} />
        <div className="py-3">
          <div className="text-xs text-gray-400">当前模型</div>
          <div className="mt-1 text-sm font-medium text-gray-700 dark:text-gray-300">{settings.ai_model}</div>
        </div>
      </Section>

      <Section title="内容">
        <Toggle label="保存思考过程" desc="同步源数据中的 thinking 内容" value={settings.include_thinking} onChange={value => update("include_thinking", value)} />
        <Toggle label="保存 Tool Call" desc="记录 AI 执行的工具调用" value={settings.include_tool_calls} onChange={value => update("include_tool_calls", value)} />
        <Toggle label="Secret 脱敏" desc="自动过滤 API Key、Token 等敏感信息" value={settings.redact_secrets} onChange={value => update("redact_secrets", value)} />
      </Section>

      <div className="mb-4">
        <button
          onClick={() => setShowAdvanced(!showAdvanced)}
          className="text-xs text-gray-400 hover:text-gray-600 flex items-center gap-1"
        >
          {showAdvanced ? "▾" : "▸"} 高级设置
        </button>
        {showAdvanced && (
          <div className="mt-3 bg-gray-50 dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4 space-y-3 text-sm">
            <div>
              <div className="text-xs text-gray-400 mb-1">知识引擎</div>
              <div className="text-gray-600 dark:text-gray-300">SiYuan 3.8.3 (内置)</div>
            </div>
            <div>
              <div className="text-xs text-gray-400 mb-1">AI 服务地址</div>
              <input
                value={settings.ai_base_url}
                onChange={event => update("ai_base_url", event.target.value)}
                className="w-full text-xs border border-gray-300 dark:border-gray-600 rounded px-2 py-1 bg-white dark:bg-gray-700"
              />
            </div>
            <div>
              <div className="text-xs text-gray-400 mb-1">AI 模型</div>
              <input
                value={settings.ai_model}
                onChange={event => update("ai_model", event.target.value)}
                className="w-full text-xs border border-gray-300 dark:border-gray-600 rounded px-2 py-1 bg-white dark:bg-gray-700"
              />
            </div>
            <div>
              <div className="text-xs text-gray-400 mb-1">Tool Result 最大字符数</div>
              <input
                type="number"
                min={1000}
                step={1000}
                value={settings.max_tool_result_chars}
                onChange={event => update("max_tool_result_chars", Number(event.target.value))}
                className="w-full text-xs border border-gray-300 dark:border-gray-600 rounded px-2 py-1 bg-white dark:bg-gray-700"
              />
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={testAiConnection}
                disabled={aiTesting}
                className="text-xs px-3 py-1 border border-gray-300 dark:border-gray-600 rounded hover:bg-gray-50 dark:hover:bg-gray-700 disabled:opacity-50"
              >
                {aiTesting ? "测试中..." : "测试当前 AI 配置"}
              </button>
              {aiHealthy !== null && (
                <span className={`text-xs ${aiHealthy ? "text-green-600" : "text-red-500"}`}>
                  {aiHealthy ? "✓ 连接正常" : "✕ 连接失败"}
                </span>
              )}
            </div>
          </div>
        )}
      </div>

      <div className="rounded-lg bg-blue-50 px-3 py-2 text-xs text-blue-600 dark:bg-blue-900/20 dark:text-blue-300">
        AI 服务、模型、同步周期和内容规则保存后需重启 AIKS 后生效；开机启动和关闭驻留设置立即生效。
      </div>
      {saveError && <div className="mt-3 text-xs text-red-500">{saveError}</div>}

      <div className="mt-4 flex items-center gap-2">
        <button
          onClick={() => void save(false)}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-sm transition-colors"
        >
          {saved ? <span className="flex items-center gap-1"><CheckCircle className="w-4 h-4" />已保存</span> : "保存设置"}
        </button>
        {!isMock && (
          <button
            onClick={() => void save(true)}
            className="px-4 py-2 border border-blue-300 text-blue-600 rounded-lg text-sm hover:bg-blue-50 dark:border-blue-700 dark:text-blue-300 dark:hover:bg-blue-900/20"
          >
            保存并重启
          </button>
        )}
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mb-5">
      <div className="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-2">{title}</div>
      <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 divide-y divide-gray-100 dark:divide-gray-700 px-4">
        {children}
      </div>
    </div>
  );
}

function Toggle({ label, desc, value, onChange }: { label: string; desc: string; value: boolean; onChange: (value: boolean) => void }) {
  return (
    <div className="flex items-center justify-between py-3">
      <div>
        <div className="text-sm">{label}</div>
        {desc && <div className="text-xs text-gray-400">{desc}</div>}
      </div>
      <button
        role="switch"
        aria-checked={value}
        onClick={() => onChange(!value)}
        className={`relative shrink-0 w-10 h-5 rounded-full transition-colors ${value ? "bg-blue-600" : "bg-gray-300 dark:bg-gray-600"}`}
      >
        <span className={`absolute left-0.5 top-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform ${value ? "translate-x-5" : "translate-x-0"}`} />
      </button>
    </div>
  );
}
'''
write("apps/aiks-desktop/src/pages/SettingsPage.tsx", settings_page)


# ---------------------------------------------------------------------------
# Keep the RED test itself formatted before regular CI reaches compile/tests.
# ---------------------------------------------------------------------------
# cargo fmt in the workflow will normalize Rust files, including the test.

print("V4.2 follow-up patch applied")
