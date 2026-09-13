import { useState, useEffect, useCallback } from "react";
import { getApi } from "../api/client";
import type { SessionItem, SessionPage } from "../api/types";

const SOURCE_LABELS: Record<string, string> = {
  opencode: "OpenCode",
  claude_code: "Claude Code",
  codex: "Codex",
  gemini_cli: "Gemini CLI",
};

const PIPELINE_STATUS_COLORS: Record<string, string> = {
  READY: "text-green-600 bg-green-50",
  PROCESSING: "text-blue-600 bg-blue-50",
  FAILED: "text-red-600 bg-red-50",
  RAW_ONLY: "text-gray-500 bg-gray-50",
  DISCOVERED: "text-yellow-600 bg-yellow-50",
};

const PIPELINE_STATUS_LABELS: Record<string, string> = {
  READY: "完成", PROCESSING: "处理中", FAILED: "失败",
  RAW_ONLY: "原始", DISCOVERED: "已发现",
};

interface Props { onViewDetail?: (sessionId: number) => void; }

export default function SessionsPage({ onViewDetail }: Props) {
  const [data, setData] = useState<SessionPage | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [source, setSource] = useState<string>("");
  const [page, setPage] = useState(0);
  const PAGE_SIZE = 50;

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await getApi().getSessions({ source: source || undefined, limit: PAGE_SIZE, offset: page * PAGE_SIZE });
      setData(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [source, page]);

  useEffect(() => { load(); }, [load]);

  const sourceOptions = ["", "opencode", "claude_code", "codex", "gemini_cli"];

  return (
    <div className="p-6">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h1 className="text-lg font-semibold text-gray-900 dark:text-gray-100">工作记录</h1>
          {data && <p className="text-sm text-gray-500 mt-0.5">共 {data.total} 条</p>}
        </div>
        <select
          value={source}
          onChange={e => { setSource(e.target.value); setPage(0); }}
          className="text-sm border border-gray-200 dark:border-gray-700 rounded px-2 py-1 bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-300"
        >
          {sourceOptions.map(s => (
            <option key={s} value={s}>{s ? SOURCE_LABELS[s] ?? s : "全部来源"}</option>
          ))}
        </select>
      </div>

      {error && (
        <div className="mb-4 px-4 py-3 bg-red-50 dark:bg-red-900/20 border border-red-200 rounded text-sm text-red-700 dark:text-red-400">
          {error}
        </div>
      )}

      {loading ? (
        <div className="flex items-center justify-center h-48 text-gray-400 text-sm">加载中...</div>
      ) : data?.items.length === 0 ? (
        <div className="flex flex-col items-center justify-center h-48 text-gray-400">
          <p className="text-sm">暂无工作记录</p>
          <p className="text-xs mt-1">请先扫描数据源</p>
        </div>
      ) : (
        <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 overflow-hidden">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-gray-100 dark:border-gray-700 bg-gray-50 dark:bg-gray-900/50">
                <th className="text-left px-4 py-2.5 text-xs font-medium text-gray-500 uppercase tracking-wide">标题</th>
                <th className="text-left px-4 py-2.5 text-xs font-medium text-gray-500 uppercase tracking-wide">来源</th>
                <th className="text-left px-4 py-2.5 text-xs font-medium text-gray-500 uppercase tracking-wide">项目</th>
                <th className="text-left px-4 py-2.5 text-xs font-medium text-gray-500 uppercase tracking-wide">处理状态</th>
                <th className="text-left px-4 py-2.5 text-xs font-medium text-gray-500 uppercase tracking-wide">更新时间</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-100 dark:divide-gray-700">
              {data?.items.map((item: SessionItem) => (
                <tr key={item.id}
                  className={`hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors ${onViewDetail ? "cursor-pointer" : ""}`}
                  onClick={() => onViewDetail?.(item.id)}
                >
                  <td className="px-4 py-3">
                    <div className="font-medium text-gray-900 dark:text-gray-100 truncate max-w-xs">
                      {item.title || item.session_id}
                    </div>
                    <div className="text-xs text-gray-400 font-mono mt-0.5">{item.session_id}</div>
                  </td>
                  <td className="px-4 py-3 text-gray-600 dark:text-gray-400">
                    {SOURCE_LABELS[item.source] ?? item.source}
                  </td>
                  <td className="px-4 py-3 text-gray-600 dark:text-gray-400 truncate max-w-[120px]">
                    {item.project_name || "—"}
                  </td>
                  <td className="px-4 py-3">
                    {item.pipeline_status ? (
                      <span className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${PIPELINE_STATUS_COLORS[item.pipeline_status] || "text-gray-600 bg-gray-100"}`}>
                        {PIPELINE_STATUS_LABELS[item.pipeline_status] ?? item.pipeline_status}
                        {item.current_stage && item.pipeline_status === "PROCESSING" && (
                          <span className="ml-1 text-[10px] opacity-75">{item.current_stage}</span>
                        )}
                      </span>
                    ) : (
                      <span className="text-gray-400 text-xs">—</span>
                    )}
                  </td>
                  <td className="px-4 py-3 text-gray-400 text-xs">
                    {item.updated_at ? new Date(item.updated_at).toLocaleString("zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }) : "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Pagination */}
      {data && data.total > PAGE_SIZE && (
        <div className="flex items-center justify-between mt-4">
          <span className="text-xs text-gray-500">
            第 {page * PAGE_SIZE + 1}–{Math.min((page + 1) * PAGE_SIZE, data.total)} 条，共 {data.total} 条
          </span>
          <div className="flex gap-2">
            <button
              onClick={() => setPage(p => p - 1)}
              disabled={page === 0}
              className="px-3 py-1 text-xs border border-gray-200 rounded disabled:opacity-40 hover:bg-gray-50"
            >上一页</button>
            <button
              onClick={() => setPage(p => p + 1)}
              disabled={(page + 1) * PAGE_SIZE >= data.total}
              className="px-3 py-1 text-xs border border-gray-200 rounded disabled:opacity-40 hover:bg-gray-50"
            >下一页</button>
          </div>
        </div>
      )}
    </div>
  );
}
