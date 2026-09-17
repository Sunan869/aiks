import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import Sidebar from "./components/Sidebar";
import OverviewPage from "./pages/OverviewPage";
import SessionsPage from "./pages/SessionsPage";
import SessionDetailPage from "./pages/SessionDetailPage";
import KnowledgeWorkspacePage from "./pages/KnowledgeWorkspacePage";
import ProcessingPage from "./pages/ProcessingPage";
import ProcessingDetailPage from "./pages/ProcessingDetailPage";
import SourcesPage from "./pages/SourcesPage";
import SettingsPage from "./pages/SettingsPage";
import DiagnosticsPage from "./pages/DiagnosticsPage";
import StartupScreen from "./components/StartupScreen";
import { getApi, shouldUseMock } from "./api/client";
import { shouldKeepWorkbenchMounted } from "./api/workbench";
import type { FullStatus, AiStatus } from "./api/types";
import {
  rawConversationNavState,
  type NavState,
  type Page,
} from "./navigation";

export default function App() {
  const [nav, setNav] = useState<NavState>({ page: "overview" });
  const [fullStatus, setFullStatus] = useState<FullStatus | null>(null);
  const [aiStatus, setAiStatus] = useState<AiStatus | null>(null);
  const [knowledgeCount, setKnowledgeCount] = useState(0);
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
    try {
      const page = await getApi().getKnowledge({ limit: 1, offset: 0 });
      setKnowledgeCount(page.total);
    } catch {}
  }, []);

  useEffect(() => {
    if (isMock) {
      setIsReady(true);
      void refreshStatus();
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
    void refreshStatus();

    const interval = setInterval(refreshStatus, 10000);
    return () => {
      unlisten1.then(f => f()); unlisten2.then(f => f());
      unlisten3.then(f => f()); unlisten4.then(f => f());
      unlisten5.then(f => f());
      clearInterval(interval);
    };
  }, [refreshStatus, isMock]);

  useEffect(() => {
    const keepMounted = shouldKeepWorkbenchMounted(
      nav.page,
      nav.page === "sessions" && nav.sessionDetailId != null,
    );
    if (!keepMounted) {
      void getApi().hideWorkbench().catch(() => {});
    }
  }, [nav.page, nav.sessionDetailId]);

  if (!isReady) return <StartupScreen step={startupStep} error={startupError} />;

  const sessionCount = fullStatus?.scan_total ?? 0;
  const isHealthy = fullStatus ? fullStatus.db_failed === 0 && fullStatus.db_conflict === 0 : true;

  const navigate = (page: Page) => setNav({ page });
  const viewSessionDetail = (id: number) => setNav({ page: "sessions", sessionDetailId: id });
  const viewKnowledgeDetail = (id: string) => setNav({ page: "knowledge", knowledgeDetailId: id });
  const viewPipelineDetail = (runId: string) => setNav({ page: "processing", pipelineDetailRunId: runId });
  const viewRawConversation = (docId: string | null) => setNav(rawConversationNavState(docId));

  const renderMain = () => {
    if (nav.page === "sessions" && nav.sessionDetailId != null) {
      return (
        <SessionDetailPage
          sessionId={nav.sessionDetailId}
          onBack={() => setNav({ page: "sessions" })}
          onViewKnowledge={viewKnowledgeDetail}
          onViewPipeline={viewPipelineDetail}
          onViewRawConversation={viewRawConversation}
        />
      );
    }
    if (nav.page === "knowledge" && nav.knowledgeDetailId) {
      return <KnowledgeWorkspacePage knowledgeId={nav.knowledgeDetailId} />;
    }
    if (nav.page === "processing" && nav.pipelineDetailRunId) {
      return <ProcessingDetailPage runId={nav.pipelineDetailRunId} onBack={() => setNav({ page: "processing" })} />;
    }

    switch (nav.page) {
      case "overview": return <OverviewPage fullStatus={fullStatus} aiStatus={aiStatus} syncInProgress={syncInProgress} onRefresh={refreshStatus} />;
      case "sessions": return <SessionsPage onViewDetail={viewSessionDetail} />;
      case "knowledge": return <KnowledgeWorkspacePage workspaceMode={nav.workbenchMode} workbenchDocId={nav.workbenchDocId} />;
      case "processing": return <ProcessingPage onViewDetail={viewPipelineDetail} />;
      case "sources": return <SourcesPage fullStatus={fullStatus} />;
      case "settings": return <SettingsPage />;
      case "diagnostics": return <DiagnosticsPage />;
    }
  };

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-gray-50 text-gray-900 dark:bg-gray-900 dark:text-gray-100">
      <div className="flex h-12 flex-shrink-0 items-center justify-between border-b border-gray-200 bg-white px-4 dark:border-gray-700 dark:bg-gray-800">
        <div className="flex items-center gap-2">
          <span className="text-sm font-semibold text-blue-600 dark:text-blue-400">AIKS</span>
          <span className="rounded bg-blue-100 px-1.5 py-0.5 text-[10px] font-semibold text-blue-700 dark:bg-blue-900/30 dark:text-blue-300">V4.2 Workbench</span>
          {isMock && <span className="rounded bg-yellow-100 px-1.5 py-0.5 text-[10px] font-medium text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400">MOCK</span>}
        </div>
        <div className="flex items-center gap-4 text-xs text-gray-400">
          {aiStatus && <div className="flex items-center gap-1"><span className={`h-1.5 w-1.5 rounded-full ${aiStatus.healthy ? "bg-green-500" : "bg-yellow-400"}`} /><span>{aiStatus.healthy ? "AI 正常" : "AI 不可用"}</span></div>}
          {syncInProgress && <span className="text-blue-500">扫描中...</span>}
          {fullStatus && !syncInProgress && <span>{fullStatus.scan_total} 条工作记录</span>}
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        <Sidebar page={nav.page} onNavigate={navigate} sessionCount={sessionCount} knowledgeCount={knowledgeCount} aiHealthy={aiStatus?.healthy ?? false} />
        <main className="flex-1 overflow-auto bg-gray-50 dark:bg-gray-900">{renderMain()}</main>
      </div>

      <div className="flex h-7 flex-shrink-0 items-center gap-3 border-t border-gray-200 bg-white px-4 text-xs text-gray-400 dark:border-gray-700 dark:bg-gray-800">
        {fullStatus && <><span>{Object.values(fullStatus.scan_by_source).filter(v => v > 0).length} 个数据源</span><span>·</span><span>{fullStatus.last_sync_at ? `最近扫描 ${new Date(fullStatus.last_sync_at).toLocaleTimeString("zh-CN")}` : "尚未扫描"}</span><span>·</span><span className={isHealthy ? "text-green-500" : "text-yellow-500"}>{isHealthy ? "状态正常" : "有待处理项"}</span></>}
        {isMock && <span className="ml-auto text-yellow-500">Mock 模式 — 仅用于开发</span>}
      </div>
    </div>
  );
}
