import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Sidebar from "./components/Sidebar";
import OverviewPage from "./pages/OverviewPage";
import SourcesPage from "./pages/SourcesPage";
import SyncPage from "./pages/SyncPage";
import KnowledgePage from "./pages/KnowledgePage";
import SettingsPage from "./pages/SettingsPage";
import DiagnosticsPage from "./pages/DiagnosticsPage";
import StartupScreen from "./components/StartupScreen";

export type Page = "overview" | "knowledge" | "sources" | "sync" | "settings" | "diagnostics";

export interface RuntimeStatus {
  state: string; port: number | null; version: string | null; mode: string;
}

export interface AppStatus {
  total_sessions: number; synced: number; pending: number;
  conflict: number; failed: number; last_sync_at: string | null;
  runtime?: RuntimeStatus;
}

export interface AiStatus {
  enabled: boolean; healthy: boolean; model: string;
  display_name: string; base_url: string;
  extraction_stats: { total: number; success: number; skipped: number; failed: number; pending: number };
}

/// Unified full status — single source of truth (spec §8)
export interface FullStatus {
  scan_total: number;
  scan_by_source: Record<string, number>;
  db_total: number;
  db_synced: number;
  db_pending: number;
  db_conflict: number;
  db_failed: number;
  last_sync_at: string | null;
  last_sync_discovered: number;
  last_sync_new: number;
  last_sync_updated: number;
  last_sync_failed: number;
  extraction_total: number;
  extraction_success: number;
  extraction_skipped: number;
  extraction_failed: number;
  extraction_pending: number;
  siyuan_ready: boolean;
  ai_ready: boolean;
  ai_model: string;
}

export default function App() {
  const [page, setPage] = useState<Page>("overview");
  const [fullStatus, setFullStatus] = useState<FullStatus | null>(null);
  const [aiStatus, setAiStatus] = useState<AiStatus | null>(null);
  const [startupStep, setStartupStep] = useState<string>("正在初始化 AIKS...");
  const [isReady, setIsReady] = useState(false);
  const [startupError, setStartupError] = useState<string | null>(null);
  const [syncInProgress, setSyncInProgress] = useState(false);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await invoke<FullStatus>("get_full_status");
      setFullStatus(s);
    } catch {}
    try {
      const ai = await invoke<AiStatus>("get_ai_status");
      setAiStatus(ai);
    } catch {}
  }, []);

  useEffect(() => {
    const unlisten1 = listen<{ step: string; message: string }>("startup-progress", (e) => {
      setStartupStep(e.payload.message);
      if (e.payload.step === "ready") setTimeout(() => setIsReady(true), 400);
    });
    const unlisten2 = listen<{ error: string }>("startup-error", (e) => {
      setStartupError(e.payload.error);
      setIsReady(true);
    });
    const unlisten3 = listen("sync-status", () => { setSyncInProgress(true); });
    const unlisten4 = listen("sync-complete", () => {
      setSyncInProgress(false);
      setTimeout(refreshStatus, 500);
    });
    const unlisten5 = listen("sync-error", () => { setSyncInProgress(false); });

    // Try to get status immediately (app might already be running)
    invoke<FullStatus>("get_full_status").then(s => { setFullStatus(s); setIsReady(true); }).catch(() => {});
    refreshStatus();

    const interval = setInterval(refreshStatus, 10000);
    return () => {
      unlisten1.then(f => f()); unlisten2.then(f => f());
      unlisten3.then(f => f()); unlisten4.then(f => f());
      unlisten5.then(f => f());
      clearInterval(interval);
    };
  }, [refreshStatus]);

  if (!isReady) return <StartupScreen step={startupStep} error={startupError} />;

  const sessionCount = fullStatus?.scan_total ?? 0;
  const isHealthy = fullStatus ? fullStatus.db_failed === 0 && fullStatus.db_conflict === 0 : true;

  return (
    <div className="flex flex-col h-screen bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-gray-100 overflow-hidden">
      {/* Top bar */}
      <div className="flex items-center justify-between px-4 h-12 bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700 flex-shrink-0">
        <div className="font-semibold text-blue-600 dark:text-blue-400 text-sm">AIKS</div>
        <div className="flex items-center gap-4 text-xs text-gray-400">
          {aiStatus && (
            <div className="flex items-center gap-1">
              <span className={`w-1.5 h-1.5 rounded-full ${aiStatus.healthy ? "bg-green-500" : "bg-yellow-400"}`} />
              <span>{aiStatus.healthy ? "AI 整理正常" : "AI 暂时不可用"}</span>
            </div>
          )}
          {syncInProgress && (
            <div className="flex items-center gap-1 text-blue-500">
              <svg className="w-3 h-3 animate-spin" fill="none" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"/>
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"/>
              </svg>
              <span>同步中...</span>
            </div>
          )}
          {fullStatus && !syncInProgress && (
            <span>{fullStatus.scan_total} 条会话</span>
          )}
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        <Sidebar
          page={page}
          onNavigate={setPage}
          sessionCount={sessionCount}
          knowledgeCount={fullStatus?.extraction_success ?? 0}
          aiHealthy={aiStatus?.healthy ?? false}
        />
        <main className="flex-1 overflow-auto">
          {page === "overview" && <OverviewPage fullStatus={fullStatus} aiStatus={aiStatus} syncInProgress={syncInProgress} onRefresh={refreshStatus} />}
          {page === "knowledge" && <KnowledgePage />}
          {page === "sources" && <SourcesPage fullStatus={fullStatus} />}
          {page === "sync" && <SyncPage fullStatus={fullStatus} onRefresh={refreshStatus} />}
          {page === "settings" && <SettingsPage />}
          {page === "diagnostics" && <DiagnosticsPage />}
        </main>
      </div>

      {/* Status bar */}
      <div className="px-4 h-7 bg-white dark:bg-gray-800 border-t border-gray-200 dark:border-gray-700 flex items-center gap-3 text-xs text-gray-400 flex-shrink-0">
        {fullStatus && (
          <>
            <span>{Object.values(fullStatus.scan_by_source).filter(v => v > 0).length} 个数据源</span>
            <span>·</span>
            <span>{fullStatus.last_sync_at
              ? `最近同步 ${new Date(fullStatus.last_sync_at).toLocaleTimeString("zh-CN")}`
              : "尚未同步"}</span>
            <span>·</span>
            <span className={isHealthy ? "text-green-500" : "text-yellow-500"}>
              {isHealthy ? "状态正常" : "有待处理项"}
            </span>
          </>
        )}
      </div>
    </div>
  );
}
