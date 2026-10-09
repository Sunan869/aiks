import { useState, useEffect, useCallback, useRef } from "react";
import { getApi } from "../api/client";
import type { PipelineSummary, PipelineStats, TaskCenterEntry, TaskCenterStats } from "../api/types";
import { useSourceName } from "../ProviderCatalog";

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
  const formatSourceName = useSourceName();
  const [runs, setRuns] = useState<PipelineSummary[]>([]);
  const [stats, setStats] = useState<PipelineStats | null>(null);
  const [tasks, setTasks] = useState<TaskCenterEntry[]>([]);
  const [taskStats, setTaskStats] = useState<TaskCenterStats | null>(null);
  const [showDiagnosticPreview, setShowDiagnosticPreview] = useState(false);
  const [diagnosticNotice, setDiagnosticNotice] = useState("");
  const diagnosticJson = JSON.stringify({
    schema: "aiks-task-diagnostics-v1",
    counts: taskStats,
  }, null, 2);
  const saveDiagnostic = () => {
    if (!taskStats) return;
    const blob = new Blob([diagnosticJson], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "aiks-task-diagnostics.json";
    link.click();
    URL.revokeObjectURL(url);
    setDiagnosticNotice("已生成仅包含统计信息的诊断摘要");
  };
  const [retryingTask, setRetryingTask] = useState<number | null>(null);
  const [retryNotice, setRetryNotice] = useState("");
  const [loadError, setLoadError] = useState("");
  const [lastSuccessfulRefresh, setLastSuccessfulRefresh] = useState<string | null>(null);
  const [taskFilter, setTaskFilter] = useState<"all" | "sync_failed" | "ai_failed" | "active">("all");
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
      const [r, s, t, totals] = await Promise.all([
        getApi().getPipelineRuns(300),
        getApi().getPipelineStats(),
        getApi().getTaskCenterEntries(200),
        getApi().getTaskCenterStats(),
      ]);
      setRuns(r);
      setStats(s);
      setTasks(t);
      setTaskStats(totals);
      setLoadError("");
      setLastSuccessfulRefresh(new Date().toLocaleTimeString());
    } catch (error) {
      setLoadError("无法刷新任务状态：" + String(error));
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

      {/* A task may fail sync independently from AI processing. */}
      {/* Raw synchronization status is separate from knowledge extraction. */}
      <section className="mb-5 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded p-4">
        <h2 className="text-sm font-semibold text-gray-800 dark:text-gray-100 mb-1">会话同步与 AI 任务状态</h2>
        <p className="text-xs text-gray-500 mb-3">下列状态分别来自同步记录与知识提炼流水线；会话同步成功不代表 AI 知识已提炼完成。</p>
        {retryNotice && <p role="status" className="text-xs text-blue-600 mb-2">{retryNotice}</p>}
        {loadError && <p role="alert" className="text-xs text-red-600 mb-2">{loadError}</p>}
        {lastSuccessfulRefresh && <p className="text-xs text-gray-400 mb-2">最近刷新：{lastSuccessfulRefresh}</p>}
        <div className="flex flex-wrap gap-3 mb-3 text-xs text-gray-500" role="status">
          <span>总会话 {taskStats?.total_sessions ?? "—"}</span>
          <span>待处理 {taskStats?.pending ?? "—"}</span>
          <span>运行中 {taskStats?.running ?? "—"}</span>
          <span>已取消 {taskStats?.cancelled ?? "—"}</span>
          <span>同步异常 {taskStats?.sync_issues ?? "—"}</span>
          <span>AI 异常 {taskStats?.ai_issues ?? "—"}</span>
        </div>
        <button type="button" className="text-xs text-blue-600 mb-2 hover:underline"
          onClick={() => setShowDiagnosticPreview(value => !value)}>
          {showDiagnosticPreview ? "收起诊断摘要" : "预览安全诊断摘要"}
        </button>
        {showDiagnosticPreview && (
          <div className="mb-3 rounded border border-gray-200 dark:border-gray-700 p-3">
            <p className="text-xs text-gray-500 mb-2">仅显示统计数据，不含 Session 正文、路径、错误内容或密钥。</p>
            <pre className="text-xs whitespace-pre-wrap">{diagnosticJson}</pre>
            <div className="flex items-center gap-3 mt-2">
              <button type="button" disabled={!taskStats} onClick={saveDiagnostic}
                className="text-xs text-blue-600 hover:underline disabled:opacity-50">导出 JSON 摘要</button>
              <button type="button" disabled={!taskStats}
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(diagnosticJson);
                    setDiagnosticNotice("诊断摘要已复制");
                  } catch {
                    setDiagnosticNotice("复制失败，请使用导出 JSON");
                  }
                }}
                className="text-xs text-blue-600 hover:underline disabled:opacity-50">复制摘要</button>
            </div>
            {diagnosticNotice && <p role="status" className="text-xs text-gray-500 mt-2">{diagnosticNotice}</p>}
          </div>
        )}
        <div className="flex flex-wrap gap-2 mb-3" aria-label="任务状态筛选">
          {([
            ["all", "全部"],
            ["sync_failed", "同步失败"],
            ["ai_failed", "AI 失败"],
            ["active", "处理中"],
          ] as const).map(([value, label]) => (
            <button
              key={value}
              type="button"
              onClick={() => setTaskFilter(value)}
              aria-pressed={taskFilter === value}
              className={`text-xs px-2.5 py-1 rounded border ${taskFilter === value
                ? "border-blue-400 bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300"
                : "border-gray-200 text-gray-500 dark:border-gray-700"}`}
            >{label}</button>
          ))}
        </div>
        {tasks.filter(task =>
          taskFilter === "all" ||
          (taskFilter === "sync_failed" && (task.sync_status?.startsWith("FAILED") || task.sync_status === "CONFLICT")) ||
          (taskFilter === "ai_failed" && (task.pipeline_status === "FAILED" || task.job_status === "FAILED")) ||
          (taskFilter === "active" && (task.sync_status === "PENDING" || task.pipeline_status === "PROCESSING" || task.job_status === "RUNNING"))
        ).length === 0 ? (
          <p className="text-xs text-gray-400">暂无会话同步任务记录</p>
        ) : (
          <div className="overflow-x-auto max-h-72 overflow-y-auto">
            <table className="w-full text-xs">
              <thead><tr className="text-left text-gray-500 border-b border-gray-200 dark:border-gray-700">
                <th className="py-2 pr-3">会话</th><th className="py-2 pr-3">来源</th>
                <th className="py-2 pr-3">原始同步</th><th className="py-2 pr-3">AI 提炼</th>
                <th className="py-2 pr-3">任务</th><th className="py-2 pr-3">最近阶段</th><th className="py-2 pr-3">操作</th><th className="py-2">错误</th>
              </tr></thead>
              <tbody>
                {tasks.filter(task =>
                  taskFilter === "all" ||
                  (taskFilter === "sync_failed" && (task.sync_status?.startsWith("FAILED") || task.sync_status === "CONFLICT")) ||
                  (taskFilter === "ai_failed" && (task.pipeline_status === "FAILED" || task.job_status === "FAILED")) ||
                  (taskFilter === "active" && (task.sync_status === "PENDING" || task.pipeline_status === "PROCESSING" || task.job_status === "RUNNING"))
                ).map(task => (
                  <tr key={task.session_id} className="border-b border-gray-100 dark:border-gray-700/40">
                    <td className="py-2 pr-3 max-w-48 truncate" title={task.title || task.external_session_id}>{task.title || task.external_session_id}</td>
                    <td className="py-2 pr-3">{formatSourceName(task.source)}</td>
                    <td className="py-2 pr-3">{task.sync_status || "未同步"}</td>
                    <td className="py-2 pr-3">{task.pipeline_status || "未提炼"}{task.current_stage ? ` · ${task.current_stage}` : ""}</td>
                    <td className="py-2 pr-3">{task.job_status || "—"}{task.attempts ? ` (${task.attempts})` : ""}</td>
                    <td className="py-2 pr-3" title={task.last_task_update || ""}>
                      {task.stage_latency_ms !== null ? `${task.stage_latency_ms}ms` : "—"}
                    </td>
                    <td className="py-2 pr-3">
                      {(task.sync_status === "SYNCED" || task.sync_status === "UNCHANGED") &&
                      (task.pipeline_status === "FAILED" || task.job_status === "FAILED") &&
                      task.job_status !== "RUNNING" && task.job_status !== "PENDING" ? (
                        <button type="button" disabled={retryingTask !== null}
                          className="text-blue-600 hover:underline disabled:opacity-50"
                          onClick={async () => {
                            setRetryingTask(task.session_id);
                            setRetryNotice("");
                            try {
                              await getApi().retryFailedAiTask(task.session_id);
                              setRetryNotice("已提交单条 AI 任务重试");
                              await load(false);
                            } catch (error) {
                              setRetryNotice(`重试未提交：${String(error)}`);
                            } finally {
                              setRetryingTask(null);
                            }
                          }}
                        >{retryingTask === task.session_id ? "提交中…" : "重试 AI"}</button>
                      ) : task.job_status === "PENDING" ? (
                        <button type="button" disabled={retryingTask !== null}
                          className="text-orange-600 hover:underline disabled:opacity-50"
                          onClick={async () => {
                            setRetryingTask(task.session_id);
                            setRetryNotice("");
                            try {
                              const cancelled = await getApi().cancelPendingAiTask(task.session_id);
                              setRetryNotice(cancelled ? "已取消排队中的 AI 任务" : "任务已开始运行或不再排队，未取消");
                              await load(false);
                            } catch (error) {
                              setRetryNotice("取消失败：" + String(error));
                            } finally {
                              setRetryingTask(null);
                            }
                          }}
                        >{retryingTask === task.session_id ? "处理中…" : "取消排队"}</button>
                      ) : "—"}
                    </td>
                    <td className="py-2 max-w-64 truncate text-red-500" title={task.sync_error || task.pipeline_error || task.job_error || ""}>
                      <details className="group max-w-64">
                        <summary className="cursor-pointer truncate list-none" title="展开错误诊断">{task.sync_error || task.pipeline_error || task.job_error || "—"}</summary>
                        <div className="mt-2 max-w-sm whitespace-pre-wrap break-all text-gray-600 dark:text-gray-300 space-y-1">
                          <div>原始同步：{task.sync_status || "未开始"}</div>
                          <div>AI 提炼：{task.pipeline_status || "未开始"}</div>
                          <div>持久任务：{task.job_status || "无"}</div>
                          {task.sync_error && <div className="text-red-500">同步错误：{task.sync_error}</div>}
                          {task.pipeline_error && <div className="text-red-500">提炼错误：{task.pipeline_error}</div>}
                          {task.job_error && <div className="text-red-500">队列错误：{task.job_error}</div>}
                        </div>
                      </details>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

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
                    <td className="px-3 py-2.5 text-gray-500 text-xs">{formatSourceName(run.source)}</td>
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
