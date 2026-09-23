import { useCallback, useEffect, useRef, useState } from "react";
import { ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { getApi } from "../api/client";
import type { WorkbenchBounds, WorkbenchStatus } from "../api/types";
import { boundWorkbenchMode, type WorkspaceMode } from "../api/workbench";

interface Props {
  surface: WorkspaceMode;
  docId?: string | null;
  suspended?: boolean;
}

function readBounds(element: HTMLElement): WorkbenchBounds | null {
  const rect = element.getBoundingClientRect();
  if (rect.width < 1 || rect.height < 1) return null;
  return {
    x: Math.round(rect.x),
    y: Math.round(rect.y),
    width: Math.round(rect.width),
    height: Math.round(rect.height),
  };
}

async function waitForBridgeReady(): Promise<WorkbenchStatus> {
  let status = await getApi().getWorkbenchStatus();
  for (let attempt = 0; attempt < 30 && status.available && !status.ready; attempt += 1) {
    await new Promise(resolve => window.setTimeout(resolve, 100));
    status = await getApi().getWorkbenchStatus();
  }
  return status;
}

function surfaceCopy(surface: WorkspaceMode): { title: string; description: string } {
  if (surface === "session") {
    return {
      title: "原始 Session 工作台",
      description: "正在以只读模式嵌入原始 Session；搜索、图谱、反链、数据库等原生展示由 SiYuan Workbench 自身负责。",
    };
  }
  return {
    title: "SiYuan 知识工作台",
    description: "正在嵌入 Canonical SiYuan 文档工作台；原生搜索、图谱、反链、数据库等展示均在 Workbench 内完成。",
  };
}

export default function WorkbenchHost({ surface, docId, suspended = false }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [mounted, setMounted] = useState(false);
  const [status, setStatus] = useState<WorkbenchStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const copy = surfaceCopy(surface);

  const open = useCallback(async () => {
    if (suspended) return;

    setLoading(true);
    setError(null);
    try {
      const readyStatus = await waitForBridgeReady();
      setStatus(readyStatus);
      if (!readyStatus.available) throw new Error("SiYuan Workbench 当前不可用");
      if (!readyStatus.ready) throw new Error("SiYuan Bridge 初始化超时");

      const boundMode = boundWorkbenchMode(surface, docId);
      if (boundMode && docId) {
        await getApi().openSiyuanDocument(docId, boundMode);
      } else {
        await getApi().showWorkbench(surface);
      }
      setStatus(await getApi().getWorkbenchStatus());
    } catch (e) {
      setError(String(e));
      try {
        setStatus(await getApi().getWorkbenchStatus());
      } catch {}
    } finally {
      setLoading(false);
    }
  }, [docId, surface, suspended]);

  useEffect(() => {
    if (suspended) {
      setMounted(false);
      void getApi().hideWorkbench().catch(() => {});
      return;
    }

    const host = hostRef.current;
    if (!host) return;

    let cancelled = false;
    let frame = 0;

    const syncBounds = async () => {
      const bounds = readBounds(host);
      if (!bounds) return;
      try {
        await getApi().mountWorkbench(bounds);
        if (!cancelled) setMounted(true);
      } catch (e) {
        if (!cancelled) {
          setError(String(e));
          setLoading(false);
        }
      }
    };

    const scheduleSync = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => void syncBounds());
    };

    const observer = new ResizeObserver(scheduleSync);
    observer.observe(host);
    window.addEventListener("resize", scheduleSync);
    scheduleSync();

    return () => {
      cancelled = true;
      cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener("resize", scheduleSync);
      void getApi().hideWorkbench().catch(() => {});
    };
  }, [suspended]);

  useEffect(() => {
    if (mounted && !suspended) void open();
  }, [mounted, open, suspended]);

  return (
    <div
      ref={hostRef}
      className="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden bg-white dark:bg-gray-800"
    >
      <div className="max-w-lg p-8 text-center">
        {loading ? (
          <Loader2 className="mx-auto h-8 w-8 animate-spin text-blue-500" />
        ) : (
          <ExternalLink className="mx-auto h-8 w-8 text-blue-500" />
        )}
        <h2 className="mt-4 text-base font-semibold text-gray-900 dark:text-gray-100">
          {copy.title}
        </h2>
        <p className="mt-2 text-sm leading-6 text-gray-500 dark:text-gray-400">
          {copy.description}
        </p>
        {status && (
          <div className="mt-4 flex justify-center gap-3 text-xs text-gray-400">
            <span className={status.available ? "text-green-500" : "text-yellow-500"}>
              {status.available ? "Workbench 可用" : "Workbench 不可用"}
            </span>
            <span>·</span>
            <span>{status.ready ? "Bridge 已连接" : "Bridge 初始化中"}</span>
          </div>
        )}
        {error && (
          <p className="mt-4 rounded-lg bg-red-50 px-3 py-2 text-xs text-red-600 dark:bg-red-950/30 dark:text-red-300">
            {error}
          </p>
        )}
        <button
          type="button"
          onClick={() => void open()}
          className="mt-5 inline-flex items-center gap-1.5 rounded-lg border border-gray-200 px-3 py-2 text-xs font-medium text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-700"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          重新打开工作台
        </button>
      </div>
    </div>
  );
}
