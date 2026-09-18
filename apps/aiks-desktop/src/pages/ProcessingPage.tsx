import { useState, useEffect, useCallback, useRef } from "react";
import { getApi } from "../api/client";
import type { PipelineSummary, PipelineStats } from "../api/types";

const STATUS_CONFIG: Record<string, { color: string; label: string; icon: string }> = {
  READY: { color: "text-green-600", label: "完成", icon: "✓" },
  PROCESSING: { color: "text-blue-500", label: "处理中", icon: "…" },
  FAILED: { color: "text-red-500", label: "失败", icon: "✕" },
  RAW_ONLY: { color: "text-gray-400", label: "原始", icon: "·" },
  DISCOVERED: { color: "text-yellow-500", label: "已发现", icon: "!" },
};

const STAGE_LABELS: Record<string, string> = {
  DISCOVERED: "发现",
  PARSED: "解析",
  NORMALIZED: "标准化",
  CLEANED: "清洗",
  LLM_CHUNKED: "切片",
  AI_EXTRACTED: "AI提炼",
  KNOWLEDGE_SPLIT: "知识拆分",
  EMBED_CHUNKED: "向量切片",
  EMBEDDED: "向量化",
  INDEXED: "索引",
  READY: "完成",
};

const STAGE_ORDER = ["PARSED", "CLEANED", "LLM_CHUNKED", "AI_EXTRACTED", "EMBEDDED", "INDEXED"];

function StageCell({ stage, stageRuns }: { stage: string; stageRuns: { stage: string; status: string }[] }) {
  const run = stageRuns.find(s => s.stage === stage);
  if (!run) return <td className="px-2 py-2 text-center text-gray-200 dark:text-gray-700 text-xs">—</td>;
  const cfg = {
    SUCCESS: { icon: "✓", color: "text-green-500" },
    RUNNING: { icon: "…", color: "text-blue-500 animate-pulse" },
    FAILED: { icon: "✕", color: "text-red-500" },
    PENDING: { icon: "·", color: "text-gray-300" },
  }[run.status] ?? { icon: "·", color: "text-gray-300" };
  return (
    <td className={`px-2 py-2 text-center text-xs font-medium ${cfg.color}`}>{cfg.icon}</td>
  );
}

interface Props { onViewDetail?: (runId: string) => void; }

export default function ProcessingPage({ onViewDetail }: Props) {
  const [runs, setRuns] = useState<PipelineSummary[]>([]);
  const [stats, setStats] = useState<PipelineStats | null>(null);
  const [filter, setFilter] = useState<string>("all");
  const [loading, setLoading] = useState(true);
  const [backfilling, setBackfilling] = useState(false);
  const [backfillMsg, setBackfillMsg] = useState("");
  const inFlightRef = useRef(false);

  const load = useCallback(async (showLoading = false) => {
    if (inFlightRef.current) return;
    inFlightRef.current = true;
    if (showLoading) setLoading(true);
    try {
      const [r, s] = await Promise.all([
        getApi().getPipelineRuns(300),
        getApi().getPipelineStats(),
      ]);
      setRuns(r);
      setStats(s);
    } finally {
      inFlightRef.current = false;
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
  }, [load]);

  const handleBackfill = useCallback(async () => {
    setBackfilling(true);
    setBackfillMsg("");
    try {
      const { submitted } = await getApi().backfillExtractions();
      setBackfillMsg(`已重新提交 ${submitted} 个任务（失败 + 未处理队列）`);
      await load(false);
    } catch (e) {
      setBackfillMsg(`提交失败: ${String(e)}`);
    } finally {
      setBackfilling(false);
    }
  }, [load]);

  const filtered = filter === "all" ? runs
    : filter === "processing" ? runs.filter(r => r.status === "PROCESSING")
    : filter === "failed" ? runs.filter(r => r.status === "FAILED")
    : runs.filter(r => r.status === "READY");

  return (
    <div className="p-6">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h1 className="text-lg font-semibold text-gray-900 dark:text-gray-100">处理中心</h1>
          <p className="text-sm text-gray-500 mt-0.5">知识处理流水线状态</p>
        </div>
        <div className="flex items-center gap-3">
          {backfillMsg && <span className="text-xs text-blue-500">{backfillMsg}</span>}
          <button
            onClick={handleBackfill}
            disabled={backfilling}
            className="text-xs px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded disabled:opacity-50"
          >
            {backfilling ? "提交中..." : "重试失败并补跑队列"}
          </button>
          <button onClick={() => void load(true)} className="text-xs px-3 py-1.5 border border-gray-200 rounded hover:bg-gray-50 dark:border-gray-700 dark:hover:bg-gray-700">
            刷新
          </button>
        </div>
      </div>

      {/* Stats bar */}
      {stats && (
        <div className="grid grid-cols-4 gap-3 mb-5">
          {[
            { label: "全部", value: stats.total, key: "all", color: "text-gray-700" },
            { label: "处理中", value: stats.processing, key: "processing", color: "text-blue-600" },
            { label: "失败", value: stats.failed, key: "failed", color: "text-red-500" },
            { label: "已完成", value: stats.ready, key: "ready", color: "text-green-600" },
          ].map(item => (
            <button
              key={item.key}
              onClick={() => setFilter(item.key)}
              className={`text-left px-4 py-3 rounded border transition-colors ${filter === item.key
                ? "border-blue-300 bg-blue-50 dark:bg-blue-900/20 dark:border-blue-700"
                : "border-gray-200 bg-white dark:bg-gray-800 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-700"
              }`}
            >
              <div className={`text-xl font-bold ${item.color} dark:opacity-90`}>{item.value}</div>
              <div className="text-xs text-gray-500 mt-0.5">{item.label}</div>
            </button>
          ))}
        </div>
      )}

      {/* Knowledge & embedding stats */}
      {stats && (
        <div className="grid grid-cols-3 gap-3 mb-5 text-center">
          <div className="px-3 py-2 bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700">
            <div className="text-base font-semibold text-gray-800 dark:text-gray-200">{stats.knowledge_items}</div>
            <div className="text-xs text-gray-400">知识条目</div>
          </div>
          <div className="px-3 py-2 bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700">
            <div className="text-base font-semibold text-gray-800 dark:text-gray-200">{stats.knowledge_chunks}</div>
            <div className="text-xs text-gray-400">向量切片</div>
          </div>
          <div className="px-3 py-2 bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700">
            <div className={`text-base font-semibold ${stats.embeddings > 0 ? "text-green-600" : "text-gray-400"}`}>
              {stats.embeddings > 0 ? stats.embeddings : "未配置"}
            </div>
            <div className="text-xs text-gray-400">向量索引</div>
          </div>
        </div>
      )}

      {loading ? (
        <div className="flex items-center justify-center h-40 text-gray-400 text-sm">加载中...</div>
      ) : filtered.length === 0 ? (
        <div className="flex flex-col items-center justify-center h-40 text-gray-400">
          <p className="text-sm">暂无处理记录</p>
        </div>
      ) : (
        <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 overflow-hidden">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-gray-100 dark:border-gray-700 bg-gray-50 dark:bg-gray-900/50">
                <th className="text-left px-4 py-2.5 text-xs font-medium text-gray-500 uppercase">会话</th>
                <th className="text-left px-3 py-2.5 text-xs font-medium text-gray-500 uppercase">来源</th>
                {STAGE_ORDER.map(s => (
                  <th key={s} className="px-2 py-2.5 text-xs font-medium text-gray-500 uppercase text-center w-12">
                    {STAGE_LABELS[s] ?? s}
                  </th>
                ))}
                <th className="text-left px-4 py-2.5 text-xs font-medium text-gray-500 uppercase">知识</th>
                <th className="text-left px-4 py-2.5 text-xs font-medium text-gray-500 uppercase">状态</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-100 dark:divide-gray-700">
              {filtered.map((run: PipelineSummary) => {
                const cfg = STATUS_CONFIG[run.status] ?? { color: "text-gray-500", label: run.status, icon: "·" };
                return (
                  <tr key={run.run_id}
                    className={`hover:bg-gray-50 dark:hover:bg-gray-700/40 transition-colors ${onViewDetail ? "cursor-pointer" : ""}`}
                    onClick={() => onViewDetail?.(run.run_id)}
                  >
                    <td className="px-4 py-2.5">
                      <div className="text-gray-900 dark:text-gray-100 truncate max-w-[200px]">
                        {run.session_title || run.run_id}
                      </div>
                    </td>
                    <td className="px-3 py-2.5 text-gray-500 text-xs">{run.source}</td>
                    {STAGE_ORDER.map(s => (
                      <StageCell key={s} stage={s} stageRuns={run.stage_runs} />
                    ))}
                    <td className="px-4 py-2.5 text-gray-600 dark:text-gray-400 text-xs">
                      {run.knowledge_count > 0 ? run.knowledge_count : "—"}
                    </td>
                    <td className="px-4 py-2.5">
                      <span className={`text-xs font-medium ${cfg.color}`}>
                        {cfg.icon} {cfg.label}
                      </span>
                      {run.error_message && (
                        <div className="text-[10px] text-red-400 truncate max-w-[120px] mt-0.5">{run.error_message}</div>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
