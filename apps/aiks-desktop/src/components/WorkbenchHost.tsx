import { useEffect, useState } from "react";
import { ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { getApi } from "../api/client";
import type { WorkbenchStatus, WorkspaceMode } from "../api/types";

interface Props {
  mode: WorkspaceMode;
  docId?: string | null;
}

export default function WorkbenchHost({ mode, docId }: Props) {
  const [status, setStatus] = useState<WorkbenchStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const open = async () => {
    setLoading(true);
    setError(null);
    try {
      if (docId) {
        await getApi().openSiyuanDocument(docId, mode);
      } else {
        await getApi().showWorkbench(mode);
      }
      setStatus(await getApi().getWorkbenchStatus());
    } catch (e) {
      setError(String(e));
      try { setStatus(await getApi().getWorkbenchStatus()); } catch {}
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void open();
    return () => { void getApi().hideWorkbench(); };
  }, [mode, docId]);

  return (
    <div className="flex min-h-0 flex-1 items-center justify-center rounded-xl border border-gray-200 bg-white p-8 dark:border-gray-700 dark:bg-gray-800">
      <div className="max-w-lg text-center">
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
            ? "正文、文档树、标签、反链、历史和原生编辑能力由持久 SiYuan Workbench 提供。"
            : "原始 Session 以只读模式打开，保留搜索、复制、折叠、反链、图谱和来源导航能力。"}
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
        {error && <p className="mt-4 rounded-lg bg-red-50 px-3 py-2 text-xs text-red-600 dark:bg-red-950/30 dark:text-red-300">{error}</p>}
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
