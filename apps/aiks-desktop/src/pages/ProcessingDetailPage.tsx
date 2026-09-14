import { useState, useEffect, useCallback } from "react";
import { getApi } from "../api/client";
import type { PipelineSummary, StageRun } from "../api/types";

const STAGE_LABELS: Record<string, string> = {
  PARSED: "解析", CLEANED: "清洗", LLM_CHUNKED: "LLM切片",
  AI_EXTRACTED: "AI提炼", KNOWLEDGE_SPLIT: "知识拆分",
  EMBED_CHUNKED: "向量切片", EMBEDDED: "向量化",
  INDEXED: "索引", READY: "完成"
};

const STATUS_CFG: Record<string, { icon: string; color: string; label: string }> = {
  SUCCESS: { icon: "✓", color: "text-green-500", label: "成功" },
  RUNNING: { icon: "…", color: "text-blue-500 animate-pulse", label: "运行中" },
  FAILED: { icon: "✕", color: "text-red-500", label: "失败" },
  SKIPPED: { icon: "—", color: "text-gray-400", label: "已跳过" },
  PENDING: { icon: "·", color: "text-gray-300", label: "等待中" },
};

function StageRow({ stage }: { stage: StageRun }) {
  const [expanded, setExpanded] = useState(false);
  const cfg = STATUS_CFG[stage.status] ?? STATUS_CFG.PENDING;

  return (
    <div className="border border-gray-100 dark:border-gray-700 rounded mb-2">
      <div
        className="flex items-center justify-between px-4 py-3 cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-700/40"
        onClick={() => setExpanded(e => !e)}
      >
        <div className="flex items-center gap-3">
          <span className={`text-sm font-medium w-5 ${cfg.color}`}>{cfg.icon}</span>
          <span className="text-sm font-medium text-gray-800 dark:text-gray-200">
            {STAGE_LABELS[stage.stage] ?? stage.stage}
          </span>
          <span className="text-xs text-gray-400">{cfg.label}</span>
        </div>
        <div className="flex items-center gap-4 text-xs text-gray-400">
          {stage.input_count != null && <span>输入 {stage.input_count}</span>}
          {stage.output_count != null && <span>→ 输出 {stage.output_count}</span>}
          {stage.latency_ms != null && <span>{stage.latency_ms}ms</span>}
          <span className="text-gray-400">{expanded ? "▲" : "▼"}</span>
        </div>
      </div>
      {expanded && (
        <div className="px-4 pb-3 border-t border-gray-100 dark:border-gray-700">
          {stage.error_message && (
            <div className="mt-2 px-3 py-2 bg-red-50 dark:bg-red-900/20 rounded text-xs text-red-600 dark:text-red-400 font-mono">
              {stage.error_message}
            </div>
          )}
          {stage.detail !== null && stage.detail !== undefined && (
            <pre className="mt-2 text-xs text-gray-500 bg-gray-50 dark:bg-gray-900/50 p-2 rounded overflow-auto max-h-32">
              {JSON.stringify(stage.detail as Record<string, unknown>, null, 2)}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

interface Props {
  runId: string;
  onBack: () => void;
}

export default function ProcessingDetailPage({ runId, onBack }: Props) {
  const [detail, setDetail] = useState<PipelineSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [retrying, setRetrying] = useState(false);
  const [retryMsg, setRetryMsg] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const d = await getApi().getPipelineDetail(runId);
      setDetail(d);
    } finally {
      setLoading(false);
    }
  }, [runId]);

  useEffect(() => { load(); }, [load]);

  const handleRetry = async () => {
    if (!detail) return;
    setRetrying(true);
    setRetryMsg(null);
    try {
      // Trigger re-run via run_pipeline_for_session
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("run_pipeline_for_session", { sessionId: detail.session_id });
      setRetryMsg("已重新提交处理任务");
      setTimeout(load, 2000);
    } catch (e) {
      setRetryMsg(`错误：${e}`);
    } finally {
      setRetrying(false);
    }
  };

  if (loading) return (
    <div className="p-6 flex items-center justify-center h-48 text-gray-400 text-sm">加载中...</div>
  );

  if (!detail) return (
    <div className="p-6 text-gray-400 text-sm">未找到处理记录: {runId}</div>
  );

  const statusColor = {
    READY: "text-green-600", PROCESSING: "text-blue-600",
    FAILED: "text-red-500", RAW_ONLY: "text-gray-500"
  }[detail.status] ?? "text-gray-600";

  return (
    <div className="p-6">
      <div className="flex items-center gap-2 mb-5">
        <button onClick={onBack} className="text-sm text-blue-500 hover:text-blue-700">← 返回</button>
        <span className="text-gray-300">/</span>
        <span className="text-sm text-gray-600 dark:text-gray-400">处理详情</span>
      </div>

      <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 p-5 mb-5">
        <div className="flex items-start justify-between">
          <div>
            <h1 className="text-base font-semibold text-gray-900 dark:text-gray-100">
              {detail.session_title || detail.run_id}
            </h1>
            <div className="flex items-center gap-3 mt-1 text-xs text-gray-500">
              <span>{detail.source}</span>
              <span>·</span>
              <span>{detail.pipeline_version}</span>
              {detail.started_at && (
                <>
                  <span>·</span>
                  <span>{new Date(detail.started_at).toLocaleString("zh-CN")}</span>
                </>
              )}
            </div>
          </div>
          <div className="flex items-center gap-3">
            <span className={`text-sm font-medium ${statusColor}`}>{detail.status}</span>
            {(detail.status === "FAILED" || detail.status === "RAW_ONLY") && (
              <button
                onClick={handleRetry}
                disabled={retrying}
                className="text-xs px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded disabled:opacity-50"
              >
                {retrying ? "提交中..." : "重新处理"}
              </button>
            )}
          </div>
        </div>
        {retryMsg && <div className="mt-2 text-xs text-blue-600">{retryMsg}</div>}
        {detail.error_message && (
          <div className="mt-3 px-3 py-2 bg-red-50 dark:bg-red-900/20 rounded text-xs text-red-600">
            {detail.error_stage}: {detail.error_message}
          </div>
        )}
      </div>

      {/* Stats */}
      <div className="grid grid-cols-3 gap-3 mb-5 text-center text-sm">
        <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 p-3">
          <div className="font-semibold text-gray-900 dark:text-gray-100">{detail.knowledge_count}</div>
          <div className="text-xs text-gray-400">知识条目</div>
        </div>
        <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 p-3">
          <div className="font-semibold text-gray-900 dark:text-gray-100">{detail.stage_runs.length}</div>
          <div className="text-xs text-gray-400">阶段完成</div>
        </div>
        <div className="bg-white dark:bg-gray-800 rounded border border-gray-200 dark:border-gray-700 p-3">
          <div className="font-semibold text-gray-900 dark:text-gray-100">
            {detail.stage_runs.reduce((sum, s) => sum + (s.latency_ms ?? 0), 0)}ms
          </div>
          <div className="text-xs text-gray-400">总耗时</div>
        </div>
      </div>

      {/* Stage trace */}
      <h2 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-3">处理阶段</h2>
      {detail.stage_runs.length === 0 ? (
        <div className="text-sm text-gray-400 text-center py-8">暂无阶段记录</div>
      ) : (
        detail.stage_runs.map((stage, i) => <StageRow key={i} stage={stage} />)
      )}
    </div>
  );
}
