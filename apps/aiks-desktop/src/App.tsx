import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import Sidebar from "./components/Sidebar";
import OverviewPage from "./pages/OverviewPage";
import SessionsPage from "./pages/SessionsPage";
import SessionDetailPage from "./pages/SessionDetailPage";
import KnowledgeBasePageV3 from "./pages/KnowledgeBasePageV3";
import KnowledgeDetailPage from "./pages/KnowledgeDetailPage";
import ProcessingPage from "./pages/ProcessingPage";
import ProcessingDetailPage from "./pages/ProcessingDetailPage";
import SearchPage from "./pages/SearchPage";
import SourcesPage from "./pages/SourcesPage";
import SettingsPage from "./pages/SettingsPage";
import DiagnosticsPage from "./pages/DiagnosticsPage";
import StartupScreen from "./components/StartupScreen";
import { getApi, shouldUseMock } from "./api/client";
import type { FullStatus, AiStatus } from "./api/types";

export type Page = "overview" | "sessions" | "knowledge" | "processing" | "search" | "sources" | "settings" | "diagnostics";

interface NavState {
  page: Page;
  sessionDetailId?: number;
  knowledgeDetailId?: string;
  pipelineDetailRunId?: string;
}

export default function App() {
  const [nav, setNav] = useState<NavState>({ page: "overview" });
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
      setIsReady(true);
      refreshStatus();
      const interval = setInterval(refreshStatus, 30000);
      return () => clearInterval(interval);
    }

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

  const navigate = (page: Page) => setNav({ page });
  const viewSessionDetail = (id: number) => setNav({ page: "sessions", sessionDetailId: id });
  const viewKnowledgeDetail = (id: string) => setNav({ page: "knowledge", knowledgeDetailId: id });
  const viewPipelineDetail = (runId: string) => setNav({ page: "processing", pipelineDetailRunId: runId });

  const renderMain = () => {
    if (nav.page === "sessions" && nav.sessionDetailId != null) {
      return <SessionDetailPage
        sessionId={nav.sessionDetailId}
        onBack={() => setNav({ page: "sessions" })}
        onViewKnowledge={viewKnowledgeDetail}
        onViewPipeline={viewPipelineDetail}
      />;
    }
    if (nav.page === "knowledge" && nav.knowledgeDetailId) {
      return <KnowledgeDetailPage
        knowledgeId={nav.knowledgeDetailId}
        onBack={() => setNav({ page: "knowledge" })}
        onViewSession={viewSessionDetail}
      />;
    }
    if (nav.page === "processing" && nav.pipelineDetailRunId) {
      return <ProcessingDetailPage
        runId={nav.pipelineDetailRunId}
        onBack={() => setNav({ page: "processing" })}
      />;
    }

    switch (nav.page) {
      case "overview": return <OverviewPage fullStatus={fullStatus} aiStatus={aiStatus} syncInProgress={syncInProgress} onRefresh={refreshStatus} />;
      case "sessions": return <SessionsPage onViewDetail={viewSessionDetail} />;
      case "knowledge": return <KnowledgeBasePageV3 onViewDetail={viewKnowledgeDetail} />;
      case "processing": return <ProcessingPage onViewDetail={viewPipelineDetail} />;
      case "search": return <SearchPage onViewKnowledge={viewKnowledgeDetail} />;
      case "sources": return <SourcesPage fullStatus={fullStatus} />;
      case "settings": return <SettingsPage />;
      case "diagnostics": return <DiagnosticsPage />;
    }
  };

  return (
    <div className="flex flex-col h-screen bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-gray-100 overflow-hidden">
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
          {fullStatus && !syncInProgress && <span>{fullStatus.scan_total} 条工作记录</span>}
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        <Sidebar
          page={nav.page}
          onNavigate={navigate}
          sessionCount={sessionCount}
          knowledgeCount={knowledgeCount}
          aiHealthy={aiStatus?.healthy ?? false}
        />
        <main className="flex-1 overflow-auto bg-gray-50 dark:bg-gray-900">
          {renderMain()}
        </main>
      </div>

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
