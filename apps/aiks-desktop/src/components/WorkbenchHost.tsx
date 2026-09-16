import { useCallback, useEffect, useRef, useState } from "react";
import { ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { getApi } from "../api/client";
import type { WorkbenchBounds, WorkbenchStatus, WorkspaceMode } from "../api/types";

interface Props {
  mode: WorkspaceMode;
  docId?: string | null;
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

export default function WorkbenchHost({ mode, docId }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [mounted, setMounted] = useState(false);
  const [status, setStatus] = useState<WorkbenchStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const open = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const readyStatus = await waitForBridgeReady();
      setStatus(readyStatus);
      if (!readyStatus.available) throw new Error("SiYuan Workbench 当前不可用");
      if (!readyStatus.ready) throw new Error("SiYuan Bridge 初始化超时");

      if (docId) {
        await getApi().openSiyuanDocument(docId, mode);
      } else {
        await getApi().showWorkbench(mode);
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
  }, [docId, mode]);

  useEffect(() => {
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
  }, []);

  useEffect(() => {
    if (mounted) void open();
  }, [mounted, open]);

  return (
    <div
      ref={hostRef}
      className="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden rounded-xl border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800"
    >
      <div className="max-w-lg p-8 text-center">
        {loading ? (
          <Loader2 className="mx-auto h-8 w-8 animate-spin text-blue-500" />
        ) : (
          <ExternalLink className="mx-auto h-8 w-8 text-blue-500" />
        )}
        <h2 className="mt-4 text-base font-semibold text-gray-900 dark:text-gray-100">
          {mode === "knowledge" ? "SiYuan 知识工作台" : "原始 Session 工作台"}
        </h2>
        <p className="mt-2 text-sm leading-6 text-gray-500 dark:text-gray-400">
          {mode === "knowledge"
            ? "正在将 SiYuan 文档树、编辑器、标签、反链、历史等能力嵌入当前区域。"
            : "正在以只读模式嵌入原始 Session，保留搜索、复制、折叠、反链和图谱能力。"}
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
