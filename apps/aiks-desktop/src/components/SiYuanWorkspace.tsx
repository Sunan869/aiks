import { useCallback, useEffect, useState } from "react";
import { AlertCircle, ExternalLink, Loader2, RefreshCw } from "lucide-react";
import {
  getApi,
  normalizeSiyuanWorkspaceUrl,
  shouldUseMock,
} from "../api/client";

interface Props {
  title?: string;
  description?: string;
}

const NATIVE_CAPABILITIES = ["创建", "编辑", "收藏", "归档", "搜索"];

export default function SiYuanWorkspace({
  title = "SiYuan 工作区",
  description = "直接使用 SiYuan 原生能力管理笔记，不在 AIKS 中重复实现编辑与知识组织。",
}: Props) {
  const mock = shouldUseMock();
  const [workspaceUrl, setWorkspaceUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [frameLoading, setFrameLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [frameRevision, setFrameRevision] = useState(0);
  const [openingWindow, setOpeningWindow] = useState(false);

  const loadWorkspaceUrl = useCallback(async () => {
    if (mock) {
      setWorkspaceUrl(null);
      setError(null);
      setLoading(false);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const rawUrl = await getApi().getSiyuanUrl();
      const normalized = normalizeSiyuanWorkspaceUrl(rawUrl);
      setWorkspaceUrl(normalized);
      if (rawUrl && !normalized) {
        setError("SiYuan 返回了非本机工作区地址，AIKS 已阻止嵌入。请检查运行时配置。");
      }
    } catch (e) {
      setWorkspaceUrl(null);
      setError(`无法获取 SiYuan 工作区地址：${String(e)}`);
    } finally {
      setLoading(false);
    }
  }, [mock]);

  useEffect(() => {
    void loadWorkspaceUrl();
  }, [loadWorkspaceUrl]);

  const reloadFrame = () => {
    setFrameLoading(true);
    setFrameRevision(value => value + 1);
  };

  const openIndependentWindow = async () => {
    if (mock) return;
    setOpeningWindow(true);
    setError(null);
    try {
      await getApi().openSiyuanWorkspace();
    } catch (e) {
      setError(`无法打开 SiYuan 独立窗口：${String(e)}`);
    } finally {
      setOpeningWindow(false);
    }
  };

  if (loading) {
    return (
      <div className="flex min-h-[420px] flex-1 items-center justify-center rounded-lg border border-gray-200 bg-white text-sm text-gray-400 dark:border-gray-700 dark:bg-gray-800">
        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
        正在连接 SiYuan 工作区...
      </div>
    );
  }

  if (mock) {
    return (
      <div className="flex min-h-[420px] flex-1 flex-col items-center justify-center rounded-lg border border-dashed border-gray-300 bg-white px-8 text-center dark:border-gray-700 dark:bg-gray-800">
        <div className="text-sm font-medium text-gray-800 dark:text-gray-200">{title}</div>
        <p className="mt-2 max-w-xl text-xs leading-relaxed text-gray-500 dark:text-gray-400">
          浏览器 Mock 模式不会启动本机 SiYuan。运行 AIKS Desktop 后，这里会直接载入 embedded SiYuan 原生工作区。
        </p>
        <div className="mt-4 flex flex-wrap justify-center gap-2">
          {NATIVE_CAPABILITIES.map(item => (
            <span key={item} className="rounded-full bg-gray-100 px-2.5 py-1 text-xs text-gray-500 dark:bg-gray-700 dark:text-gray-300">
              {item}
            </span>
          ))}
        </div>
      </div>
    );
  }

  if (!workspaceUrl) {
    return (
      <div className="flex min-h-[420px] flex-1 flex-col items-center justify-center rounded-lg border border-gray-200 bg-white px-8 text-center dark:border-gray-700 dark:bg-gray-800">
        <AlertCircle className="h-7 w-7 text-amber-500" />
        <div className="mt-3 text-sm font-medium text-gray-800 dark:text-gray-200">
          {error ? "SiYuan 工作区暂不可用" : "SiYuan 正在启动"}
        </div>
        <p className="mt-2 max-w-xl text-xs leading-relaxed text-gray-500 dark:text-gray-400">
          {error ?? "AIKS 已启动 embedded SiYuan runtime，但暂时还没有可用地址。稍后重试即可，不需要重新创建任何知识数据。"}
        </p>
        <button
          type="button"
          onClick={() => void loadWorkspaceUrl()}
          className="mt-4 inline-flex items-center gap-1.5 rounded-md border border-gray-200 px-3 py-1.5 text-xs text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-700"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          重新连接
        </button>
      </div>
    );
  }

  return (
    <div className="flex min-h-[520px] flex-1 flex-col overflow-hidden rounded-lg border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800">
      <div className="flex flex-shrink-0 items-center justify-between gap-4 border-b border-gray-200 px-3 py-2 dark:border-gray-700">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium text-gray-800 dark:text-gray-200">{title}</span>
            <span className="rounded bg-green-50 px-1.5 py-0.5 text-[10px] font-medium text-green-700 dark:bg-green-900/30 dark:text-green-400">
              原生能力
            </span>
          </div>
          <p className="mt-0.5 truncate text-[11px] text-gray-400">{description}</p>
        </div>
        <div className="flex flex-shrink-0 items-center gap-1.5">
          <button
            type="button"
            onClick={reloadFrame}
            className="inline-flex items-center gap-1 rounded-md border border-gray-200 px-2.5 py-1.5 text-xs text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-700"
            title="重新加载 SiYuan 工作区"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${frameLoading ? "animate-spin" : ""}`} />
            刷新
          </button>
          <button
            type="button"
            onClick={() => void openIndependentWindow()}
            disabled={openingWindow}
            className="inline-flex items-center gap-1 rounded-md border border-gray-200 px-2.5 py-1.5 text-xs text-gray-600 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-700"
            title="如果当前 WebView 环境不适合嵌入，可使用现有 SiYuan 独立窗口"
          >
            {openingWindow ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ExternalLink className="h-3.5 w-3.5" />}
            独立窗口
          </button>
        </div>
      </div>

      {error && (
        <div className="flex items-center gap-2 border-b border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-700 dark:border-amber-900/50 dark:bg-amber-900/20 dark:text-amber-300">
          <AlertCircle className="h-3.5 w-3.5 flex-shrink-0" />
          {error}
        </div>
      )}

      <div className="relative min-h-[460px] flex-1 bg-gray-50 dark:bg-gray-900">
        {frameLoading && (
          <div className="absolute inset-x-0 top-0 z-10 flex items-center justify-center bg-white/90 py-2 text-xs text-gray-500 backdrop-blur-sm dark:bg-gray-800/90 dark:text-gray-300">
            <Loader2 className="mr-1.5 h-3.5 w-3.5 animate-spin" />
            正在重新加载 SiYuan...
          </div>
        )}
        <iframe
          key={frameRevision}
          src={workspaceUrl}
          title={title}
          className="h-full min-h-[460px] w-full border-0 bg-white"
          allow="clipboard-read; clipboard-write; fullscreen"
          referrerPolicy="no-referrer"
          onLoad={() => setFrameLoading(false)}
        />
      </div>
    </div>
  );
}
