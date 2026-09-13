import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import Sidebar from "./components/Sidebar";
import OverviewPage from "./pages/OverviewPage";
import SessionsPage from "./pages/SessionsPage";
import KnowledgeBasePageV3 from "./pages/KnowledgeBasePageV3";
import ProcessingPage from "./pages/ProcessingPage";
import SearchPage from "./pages/SearchPage";
import SourcesPage from "./pages/SourcesPage";
import SettingsPage from "./pages/SettingsPage";
import DiagnosticsPage from "./pages/DiagnosticsPage";
import StartupScreen from "./components/StartupScreen";
import { getApi, shouldUseMock } from "./api/client";
import type { FullStatus, AiStatus } from "./api/types";

export type Page = "overview" | "sessions" | "knowledge" | "processing" | "search" | "sources" | "settings" | "diagnostics";

export default function App() {
  const [page, setPage] = useState<Page>("overview");
  const [fullStatus, setFullStatus] = useState<FullStatus | null>(null);
  const [aiStatus, setAiStatus] = useState<AiStatus | null>(null);
  const [startupStep, setStartupStep] = useState<string>("正在初始化 AIKS...");
  const [isReady, setIsReady] = useState(false);
  const [startupError, setStartupError] = useState<string | null>(null);
  const [syncInProgress, setSyncInProgress] = useState(false);

  const isMock = shouldUseMock();

  const refreshStatus = useCallback(async () => {
    try {
      const s = await getApi().getFullStatus();
      setFullStatus(s);
    } catch {}
    try {
      const ai = await getApi().getAiStatus();
      setAiStatus(ai);
    } catch {}
  }, []);

  useEffect(() => {
    if (isMock) {
      // Mock mode: skip Tauri event listeners, show immediately
      setIsReady(true);
      refreshStatus();
      const interval = setInterval(refreshStatus, 30000);
      return () => clearInterval(interval);
    }

    // Tauri mode: listen for startup events
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

    // Try to get status immediately
    getApi().getFullStatus().then(s => { setFullStatus(s); setIsReady(true); }).catch(() => {});
    refreshStatus();

    const interval = setInterval(refreshStatus, 10000);
    return () => {
      unlisten1.then(f => f()); unlisten2.then(f => f());
      unlisten3.then(f => f()); unlisten4.then(f => f());
      unlisten5.then(f => f());
      clearInterval(interval);
    };
  }, [refreshStatus, isMock]);

  if (!isReady) return <StartupScreen step={startupStep} error={startupError} />;

  const sessionCount = fullStatus?.scan_total ?? 0;
  const knowledgeCount = fullStatus?.extraction_success ?? 0;
  const isHealthy = fullStatus ? fullStatus.db_failed === 0 && fullStatus.db_conflict === 0 : true;

  return (
    <div className="flex flex-col h-screen bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-gray-100 overflow-hidden">
      {/* Top bar */}
      <div className="flex items-center justify-between px-4 h-12 bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700 flex-shrink-0">
        <div className="flex items-center gap-2">
          <span className="font-semibold text-blue-600 dark:text-blue-400 text-sm">AIKS</span>
          {isMock && (
            <span className="text-[10px] px-1.5 py-0.5 bg-yellow-100 dark:bg-yellow-900/30 text-yellow-700 dark:text-yellow-400 rounded font-medium">MOCK</span>
          )}
        </div>
        <div className="flex items-center gap-4 text-xs text-gray-400">
          {aiStatus && (
            <div className="flex items-center gap-1">
              <span className={`w-1.5 h-1.5 rounded-full ${aiStatus.healthy ? "bg-green-500" : "bg-yellow-400"}`} />
              <span>{aiStatus.healthy ? "AI 正常" : "AI 不可用"}</span>
            </div>
          )}
          {syncInProgress && (
            <div className="flex items-center gap-1 text-blue-500">
              <svg className="w-3 h-3 animate-spin" fill="none" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"/>
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"/>
              </svg>
              <span>扫描中...</span>
            </div>
          )}
          {fullStatus && !syncInProgress && (
            <span>{fullStatus.scan_total} 条工作记录</span>
          )}
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        <Sidebar
          page={page}
          onNavigate={setPage}
          sessionCount={sessionCount}
          knowledgeCount={knowledgeCount}
          aiHealthy={aiStatus?.healthy ?? false}
        />
        <main className="flex-1 overflow-auto bg-gray-50 dark:bg-gray-900">
          {page === "overview" && <OverviewPage fullStatus={fullStatus} aiStatus={aiStatus} syncInProgress={syncInProgress} onRefresh={refreshStatus} />}
          {page === "sessions" && <SessionsPage />}
          {page === "knowledge" && <KnowledgeBasePageV3 />}
          {page === "processing" && <ProcessingPage />}
          {page === "search" && <SearchPage />}
          {page === "sources" && <SourcesPage fullStatus={fullStatus} />}
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
              ? `最近扫描 ${new Date(fullStatus.last_sync_at).toLocaleTimeString("zh-CN")}`
              : "尚未扫描"}</span>
            <span>·</span>
            <span className={isHealthy ? "text-green-500" : "text-yellow-500"}>
              {isHealthy ? "状态正常" : "有待处理项"}
            </span>
          </>
        )}
        {isMock && <span className="ml-auto text-yellow-500">Mock 模式 — 仅用于开发</span>}
      </div>
    </div>
  );
}
