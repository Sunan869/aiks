import { PageHeader } from "../components/ui/PageHeader";
import { UiNotice, UiSection, UiMetric } from "../components/ui/surfaces";
import { useState, useEffect, useCallback, useRef } from "react";
import { getApi } from "../api/client";
import { createSafeTaskDiagnosticJson } from "../task-diagnostics";
import { collectProcessingPanels, isFreshProcessingRead } from "../processing-refresh";
import type { PipelineSummary, PipelineStats, TaskCenterEntry, TaskCenterStats } from "../api/types";
import { useSourceName } from "../ProviderCatalog";
import { UiButton, UiInput } from "../components/ui/fields";

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

interface Props { onViewDetail?: (runId: string) => void; onViewSession?: (sessionId: number) => void; }

export default function ProcessingPage({ onViewDetail, onViewSession }: Props) {
  const formatSourceName = useSourceName();
  const [runs, setRuns] = useState<PipelineSummary[]>([]);
  const [stats, setStats] = useState<PipelineStats | null>(null);
  const [tasks, setTasks] = useState<TaskCenterEntry[]>([]);
  const [taskStats, setTaskStats] = useState<TaskCenterStats | null>(null);
  const [showDiagnosticPreview, setShowDiagnosticPreview] = useState(false);
  const [diagnosticNotice, setDiagnosticNotice] = useState("");
  const [diagnosticJson, setDiagnosticJson] = useState<string | null>(null);

  // Snapshot data at the exact moment the user requests a preview. The live
  // task polling must never silently replace an already inspected export.
  const refreshDiagnosticPreview = () => {
    if (!taskPanelFresh || !taskStats) return;
    setDiagnosticJson(createSafeTaskDiagnosticJson(taskStats, tasks));
    setDiagnosticNotice("已生成新的脱敏任务状态快照；请检查内容后再选择复制或保存。");
    setShowDiagnosticPreview(true);
  };

  const toggleDiagnosticPreview = () => {
    if (showDiagnosticPreview) {
      setShowDiagnosticPreview(false);
      setDiagnosticJson(null);
      setDiagnosticNotice("");
    } else {
      refreshDiagnosticPreview();
    }
  };

  const saveDiagnostic = () => {
    if (!diagnosticJson) return;
    const blob = new Blob([diagnosticJson], { type: "application/json;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "aiks-task-diagnostics-v2.json";
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 0);
    setDiagnosticNotice("已请求保存审核后的脱敏任务诊断快照。");
  };
  const [retryingTask, setRetryingTask] = useState<number | null>(null);
  const [expandedSession, setExpandedSession] = useState<number | null>(null);
  const [deletionPreflight, setDeletionPreflight] = useState<string | null>(null);
  const [retryNotice, setRetryNotice] = useState("");
  const [taskLoadError, setTaskLoadError] = useState("");
  const [pipelineLoadError, setPipelineLoadError] = useState("");
  const [lastTaskRefresh, setLastTaskRefresh] = useState<string | null>(null);
  const [lastPipelineRefresh, setLastPipelineRefresh] = useState<string | null>(null);
  const [taskPanelFresh, setTaskPanelFresh] = useState(false);
  const [pipelinePanelFresh, setPipelinePanelFresh] = useState(false);
  const [taskFilter, setTaskFilter] = useState<"all" | "sync_failed" | "ai_failed" | "active">("all");
  const [filter, setFilter] = useState<string>("all");
  const [runQuery, setRunQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [backfilling, setBackfilling] = useState(false);
  const [backfillMsg, setBackfillMsg] = useState("");
  const inFlightRef = useRef(false);
  const mountedRef = useRef(true);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const autoRefreshRef = useRef(true);
  const [taskRefreshFailures, setTaskRefreshFailures] = useState(0);
  const [pipelineRefreshFailures, setPipelineRefreshFailures] = useState(0);
  // A poll overlapping a mutation must not re-enable actions on an old snapshot.
  const mutationGeneration = useRef(0);
  const mutationInFlight = useRef(false);
  const setPolling = (enabled: boolean) => { autoRefreshRef.current = enabled; setAutoRefresh(enabled); };

  const load = useCallback(async (showLoading = false) => {
    if (inFlightRef.current) return;
    inFlightRef.current = true;
    const startedGeneration = mutationGeneration.current;
    if (showLoading) setLoading(true);
    try {
      const result = await collectProcessingPanels(getApi());
      if (!mountedRef.current) return;
      const refreshedAt = new Date().toLocaleTimeString("zh-CN");
      if (result.tasks.status === "ok" &&
          isFreshProcessingRead(startedGeneration, mutationGeneration.current, mutationInFlight.current)) {
        setTasks(result.tasks.first);
        setTaskStats(result.tasks.second);
        setTaskPanelFresh(true);
        setTaskLoadError("");
        setTaskRefreshFailures(0);
        setLastTaskRefresh(refreshedAt);
      } else if (result.tasks.status === "error") {
        setTaskPanelFresh(false);
        setTaskRefreshFailures(previous => previous + 1);
        setTaskLoadError("任务列表或全库统计读取失败，已保留旧数据；刷新恢复前禁止任务写操作。");
      } else {
        setTaskPanelFresh(false);
      }
      if (result.pipeline.status === "ok") {
        setRuns(result.pipeline.first);
        setStats(result.pipeline.second);
        setPipelinePanelFresh(true);
        setPipelineLoadError("");
        setPipelineRefreshFailures(0);
        setLastPipelineRefresh(refreshedAt);
      } else {
        setPipelinePanelFresh(false);
        setPipelineRefreshFailures(previous => previous + 1);
        setPipelineLoadError("流水线记录或统计读取失败，已保留旧数据；可独立查看任务列表。");
      }
    } catch {
      if (!mountedRef.current) return;
      setTaskPanelFresh(false);
      setPipelinePanelFresh(false);
      setTaskLoadError("任务刷新未完成，请检查本地服务后重试。");
      setPipelineLoadError("流水线刷新未完成，请检查本地服务后重试。");
    } finally {
      inFlightRef.current = false;
      if (showLoading && mountedRef.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    void load(true);
    const poll = () => {
      if (autoRefreshRef.current && document.visibilityState === "visible") void load(false);
    };
    const interval = window.setInterval(poll, 2000);
    const onVisibilityChange = () => {
      if (autoRefreshRef.current && document.visibilityState === "visible") void load(false);
    };
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      mountedRef.current = false;
      window.clearInterval(interval);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, [load]);

  const handleBackfill = useCallback(async () => {
    if (backfilling || retryingTask !== null || !taskPanelFresh) return;
    mutationGeneration.current += 1;
    mutationInFlight.current = true;
    setTaskPanelFresh(false);
    setBackfilling(true);
    setBackfillMsg("");
    try {
      const { submitted } = await getApi().backfillExtractions();
      setBackfillMsg("已重新提交 " + submitted + " 个任务（失败 + 未处理队列）");
    } catch {
      // Transport errors can contain credentials, paths and Session text.
      setBackfillMsg("补跑提交失败，请检查本地任务服务后重试。");
    } finally {
      mutationGeneration.current += 1;
      mutationInFlight.current = false;
      setTaskPanelFresh(false);
      setBackfilling(false);
      await load(false);
    }
  }, [backfilling, retryingTask, taskPanelFresh, load]);

  const filteredByStatus = filter === "all" ? runs
    : filter === "processing" ? runs.filter(r => r.status === "PROCESSING")
    : filter === "failed" ? runs.filter(r => r.status === "FAILED")
    : runs.filter(r => r.status === "READY");
  const searchText = runQuery.trim().toLocaleLowerCase();
  const filtered = filteredByStatus.filter(run =>
    !searchText || [run.session_title, run.run_id, run.source, run.error_message]
      .some(value => String(value ?? "").toLocaleLowerCase().includes(searchText))
  );
  const visibleTasks = tasks.filter(task => {
    const matchesStatus = taskFilter === "all" ||
      (taskFilter === "sync_failed" && (task.sync_status?.startsWith("FAILED") || task.sync_status === "CONFLICT")) ||
      (taskFilter === "ai_failed" && (task.pipeline_status === "FAILED" || task.job_status === "FAILED")) ||
      (taskFilter === "active" && (task.sync_status === "PENDING" || task.pipeline_status === "PROCESSING" || task.job_status === "RUNNING"));
    const matchesText = !searchText ||
      [task.title, task.external_session_id, task.source, task.sync_error, task.pipeline_error, task.job_error]
        .some(value => String(value ?? "").toLocaleLowerCase().includes(searchText));
    return matchesStatus && matchesText;
  });

  return (
    <div className="aiks-page-shell min-w-0">
      <PageHeader title="处理中心" description="会话同步、AI 提炼与知识处理流水线状态" actions={<>
          {backfillMsg && <span className="text-xs text-blue-500">{backfillMsg}</span>}
          <UiButton variant="primary" size="sm"
            onClick={handleBackfill}
            disabled={backfilling || retryingTask !== null || !taskPanelFresh}
            className="text-xs px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded disabled:opacity-50"
          >
            {backfilling ? "提交中..." : "重试失败并补跑队列"}
          </UiButton>
          <UiButton variant={autoRefresh ? "outline" : "secondary"} size="sm" type="button" aria-pressed={!autoRefresh} onClick={() => setPolling(!autoRefresh)}>{autoRefresh ? "暂停自动刷新" : "恢复自动刷新"}</UiButton>
          <UiButton variant="outline" size="sm" onClick={() => void load(true)} className="text-xs px-3 py-1.5 border border-gray-200 rounded hover:bg-gray-50 dark:border-gray-700 dark:hover:bg-gray-700">
            刷新
          </UiButton></>} />

      <div className="mb-4 rounded-xl border border-slate-200 bg-white p-3 dark:border-slate-800 dark:bg-slate-900">
        <label className="block text-xs font-medium text-slate-500 dark:text-slate-400" htmlFor="processing-run-search">搜索任务与处理记录</label>
        <UiInput id="processing-run-search" value={runQuery} onChange={event => setRunQuery(event.target.value)}
          placeholder="查找会话标题、来源或错误内容…" className="mt-1 w-full rounded-lg border border-slate-200 px-3 py-2 text-sm dark:border-slate-700" />
      </div>

      {/* A task may fail sync independently from AI processing. */}
      {/* Raw synchronization status is separate from knowledge extraction. */}
      <section className="aiks-surface mb-5">
        <h2 className="text-sm font-semibold text-gray-800 dark:text-gray-100 mb-1">会话同步与 AI 任务状态</h2>
        <p className="text-xs text-gray-500 mb-3">下列状态分别来自同步记录与知识提炼流水线；会话同步成功不代表 AI 知识已提炼完成。</p>
        <p className="text-xs text-amber-700 dark:text-amber-400 mb-3">出现文档冲突时仅支持打开会话人工核查，AIKS 不会自动覆盖 SiYuan 中用户修改过的内容。</p>
        {retryNotice && <p role="status" className="text-xs text-blue-600 mb-2">{retryNotice}</p>}
        {deletionPreflight && <UiNotice tone="warning">{deletionPreflight}</UiNotice>}
        {taskLoadError && <UiNotice tone="warning" className="mb-2">{taskLoadError}</UiNotice>}
        {pipelineLoadError && <UiNotice tone="warning" className="mb-2">{pipelineLoadError}</UiNotice>}
        <p role="status" className="mb-2 text-xs text-slate-500">
          自动刷新：{autoRefresh ? "每 2 秒（窗口可见时）" : "已暂停"}
          {" · "}任务最近成功：{lastTaskRefresh ?? "尚无"}{taskRefreshFailures > 0 ? " · 失败 " + taskRefreshFailures + " 次" : ""}
          {" · "}流水线最近成功：{lastPipelineRefresh ?? "尚无"}{pipelineRefreshFailures > 0 ? " · 失败 " + pipelineRefreshFailures + " 次" : ""}
        </p>
        {!taskPanelFresh && tasks.length > 0 && (
          <UiNotice tone="warning" className="mb-3">以下是上次成功刷新时的任务快照，状态可能已变化，重试与取消暂不可用。</UiNotice>
        )}
        <div className="mb-4 grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-6" role="status" aria-label="任务统计">
          {([
            ["总会话", taskStats?.total_sessions],
            ["待处理", taskStats?.pending],
            ["运行中", taskStats?.running],
            ["已取消", taskStats?.cancelled],
            ["同步异常", taskStats?.sync_issues],
            ["AI 异常", taskStats?.ai_issues],
          ] as const).map(([label, value]) => (
            <div key={label} className="rounded-xl border border-slate-200 bg-slate-50/70 px-3 py-3 dark:border-slate-700 dark:bg-slate-800/70">
              <div className="text-xs font-medium text-slate-500 dark:text-slate-400">{label}</div>
              <div className="mt-1 text-xl font-semibold tabular-nums text-slate-900 dark:text-white">{value ?? "—"}</div>
            </div>
          ))}
        </div>
        <UiButton variant="outline" size="sm" type="button" className="text-xs text-blue-600 mb-2 hover:underline disabled:opacity-50"
          disabled={!taskPanelFresh && !showDiagnosticPreview}
          onClick={toggleDiagnosticPreview}>
          {showDiagnosticPreview ? "收起诊断摘要" : "预览脱敏任务诊断包"}
        </UiButton>
        {showDiagnosticPreview && diagnosticJson !== null && (
          <div className="mb-3 rounded border border-gray-200 dark:border-gray-700 p-3">
            <p className="text-xs text-gray-500 mb-2">
              此诊断包仅含全库统计及最近最多 200 条任务的匿名化聚合。
              不导出 Session/任务 ID、标题、文件路径、原文、错误文本或密钥；
              最新任务轮询不会静默改变当前快照。
            </p>
            <pre aria-label="脱敏任务诊断预览" className="text-xs whitespace-pre-wrap max-h-72 overflow-auto">{diagnosticJson}</pre>
            <div className="flex flex-wrap items-center gap-3 mt-2">
              <UiButton variant="outline" size="sm" type="button" onClick={refreshDiagnosticPreview}
                disabled={!taskPanelFresh} className="text-xs text-blue-600 hover:underline disabled:opacity-50">更新预览快照</UiButton>
              <UiButton variant="outline" size="sm" type="button" onClick={saveDiagnostic}
                className="text-xs text-blue-600 hover:underline">保存审核后的 JSON</UiButton>
              <UiButton variant="outline" size="sm" type="button"
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(diagnosticJson);
                    setDiagnosticNotice("已复制当前审核后的脱敏快照");
                  } catch {
                    setDiagnosticNotice("复制失败，请使用保存 JSON");
                  }
                }}
                className="text-xs text-blue-600 hover:underline">复制当前快照</UiButton>
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
            <UiButton variant="outline" size="sm"
              key={value}
              type="button"
              onClick={() => setTaskFilter(value)}
              aria-pressed={taskFilter === value}
              className={`rounded-lg border px-3 py-2 text-xs font-medium transition-colors focus-visible:ring-2 focus-visible:ring-sky-500 ${taskFilter === value
                ? "border-blue-400 bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300"
                : "border-gray-200 text-gray-500 dark:border-gray-700"}`}
            >{label}</UiButton>
          ))}
        </div>
        <p className="mb-2 text-xs text-slate-500" role="status">当前显示 {visibleTasks.length} / {tasks.length} 条近期任务；全库统计不受筛选影响。</p>
        {visibleTasks.length === 0 ? (
          <p className="text-xs text-gray-400">{searchText || taskFilter !== "all" ? "没有符合筛选条件的任务" : "暂无会话同步任务记录"}</p>
        ) : (
          <div className="aiks-data-table max-h-[440px] overflow-auto" role="region" aria-label="近期任务记录" tabIndex={0}>
            <table className="w-full min-w-[960px] text-xs">
              <thead className="sticky top-0 z-10 bg-slate-50 dark:bg-slate-900"><tr className="text-left text-gray-500 border-b border-gray-200 dark:border-gray-700">
                <th className="py-2 pr-3">会话</th><th className="py-2 pr-3">来源</th>
                <th className="py-2 pr-3">原始同步</th><th className="py-2 pr-3">AI 提炼</th>
                <th className="py-2 pr-3">任务</th><th className="py-2 pr-3">最近阶段</th><th className="py-2 pr-3">操作</th><th className="py-2">错误</th>
              </tr></thead>
              <tbody>
                {visibleTasks.map(task => (
                  <tr key={task.session_id} className="border-b border-gray-100 dark:border-gray-700/40">
                    <td className="py-2 pr-3 max-w-48 truncate" title={task.title || task.external_session_id}>{task.title || task.external_session_id}</td>
                    <td className="py-2 pr-3">{formatSourceName(task.source)}</td>
                    <td className="py-2 pr-3">{task.sync_excluded ? "已排除" : task.sync_status || "未同步"}</td>
                    <td className="py-2 pr-3">{task.pipeline_status || "未提炼"}{task.current_stage ? ` · ${task.current_stage}` : ""}</td>
                    <td className="py-2 pr-3">{task.job_status || "—"}{task.attempts ? ` (${task.attempts})` : ""}</td>
                    <td className="py-2 pr-3" title={task.last_task_update || ""}>
                      {task.stage_latency_ms !== null ? `${task.stage_latency_ms}ms` : "—"}
                    </td>
                    <td className="py-2 pr-3">
                      <div className="flex items-center gap-2">
                        <UiButton variant="outline" size="sm" type="button"
                          disabled={!onViewSession}
                          title="查看已入库的会话详情，不改变同步或 AI 排队状态"
                          onClick={() => onViewSession?.(task.session_id)}>预览</UiButton>
                        <UiButton variant="outline" size="sm" type="button"
                          onClick={() => setExpandedSession(current => current === task.session_id ? null : task.session_id)}>
                          {expandedSession === task.session_id ? "收起阶段" : "处理阶段"}
                        </UiButton>
                        <UiButton variant="outline" size="sm" type="button"
                          disabled={!taskPanelFresh}
                          onClick={async () => {
                            try {
                              const impact = await getApi().inspectSessionDeletion(task.session_id);
                              setDeletionPreflight(`会话 #${impact.session_id}：待处理 ${impact.pending_jobs}，运行中 ${impact.running_jobs}，知识条目 ${impact.knowledge_items}、知识切片 ${impact.knowledge_chunks}、向量记录 ${impact.embedding_records}；SiYuan 会话文档 ${impact.siyuan_documents}、知识文档 ${impact.knowledge_siyuan_documents}。删除尚未开放，需先验证远端归属及修改冲突。`);
                            } catch {
                              setDeletionPreflight("无法获取删除影响清单，请刷新重试");
                            }
                          }}>删除影响预检</UiButton>
                        <UiButton variant="outline" size="sm" type="button"
                          disabled={!taskPanelFresh || retryingTask !== null || task.job_status === "RUNNING"}
                          onClick={async () => {
                            const next = !task.sync_excluded;
                            if (next && !window.confirm("确认排除此会话的后续同步和 AI 排队？既有 SiYuan 文档和知识不会删除。")) return;
                            mutationGeneration.current += 1;
                            mutationInFlight.current = true;
                            setTaskPanelFresh(false);
                            setRetryingTask(task.session_id);
                            try {
                              await getApi().setSessionSyncExcluded(task.session_id, next);
                              setRetryNotice(next ? "已排除后续同步" : "已恢复同步资格");
                            } catch {
                              setRetryNotice("操作未完成，请刷新后重试");
                            } finally {
                              mutationGeneration.current += 1;
                              mutationInFlight.current = false;
                              setRetryingTask(null);
                              await load(false);
                            }
                          }}>{task.sync_excluded ? "恢复同步" : "不同步"}</UiButton>
                      {(task.sync_status === "SYNCED" || task.sync_status === "UNCHANGED") &&
                      (task.pipeline_status === "FAILED" || task.job_status === "FAILED") &&
                      task.job_status !== "RUNNING" && task.job_status !== "PENDING" ? (
                        <UiButton variant="outline" size="sm" type="button" disabled={retryingTask !== null || backfilling || !taskPanelFresh}
                          className="text-blue-600 hover:underline disabled:opacity-50"
                          onClick={async () => {
                            mutationGeneration.current += 1;
                            mutationInFlight.current = true;
                            setTaskPanelFresh(false);
                            setRetryingTask(task.session_id);
                            setRetryNotice("");
                            try {
                              await getApi().retryFailedAiTask(task.session_id);
                              setRetryNotice("已提交单条 AI 任务重试");
                              await load(false);
                            } catch {
                              setRetryNotice("重试未提交，请检查本地任务服务后刷新。");
                            } finally {
                              mutationGeneration.current += 1;
                              mutationInFlight.current = false;
                              setTaskPanelFresh(false);
                              setRetryingTask(null);
                              await load(false);
                            }
                          }}
                        >{retryingTask === task.session_id ? "提交中…" : "重试 AI"}</UiButton>
                      ) : task.sync_status === "CONFLICT" ? (
                        <UiButton variant="outline" size="sm" type="button" className="text-amber-600 hover:underline"
                          onClick={() => onViewSession?.(task.session_id)}
                          disabled={!onViewSession}
                          title="打开会话核查来源和目标，保留 SiYuan 用户改动；不会自动覆盖">
                          人工核查冲突
                        </UiButton>
                      ) : task.job_status === "PENDING" ? (
                        <UiButton variant="outline" size="sm" type="button" disabled={retryingTask !== null || backfilling || !taskPanelFresh}
                          className="text-orange-600 hover:underline disabled:opacity-50"
                          onClick={async () => {
                            mutationGeneration.current += 1;
                            mutationInFlight.current = true;
                            setTaskPanelFresh(false);
                            setRetryingTask(task.session_id);
                            setRetryNotice("");
                            try {
                              const cancelled = await getApi().cancelPendingAiTask(task.session_id);
                              setRetryNotice(cancelled ? "已取消排队中的 AI 任务" : "任务已开始运行或不再排队，未取消");
                              await load(false);
                            } catch {
                              setRetryNotice("取消未完成，请刷新任务状态后重试。");
                            } finally {
                              mutationGeneration.current += 1;
                              mutationInFlight.current = false;
                              setTaskPanelFresh(false);
                              setRetryingTask(null);
                              await load(false);
                            }
                          }}
                        >{retryingTask === task.session_id ? "处理中…" : "取消排队"}</UiButton>
                      ) : null}
                      </div>
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
            {expandedSession !== null && (() => {
              const run = runs.find(item => item.session_id === expandedSession);
              return <div className="border-t p-4 text-xs space-y-3" role="region" aria-label="所选会话处理阶段">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span>会话 #{expandedSession} · {run ? (STATUS_CONFIG[run.status]?.label ?? run.status) : "暂无流水线记录"}</span>
                  {run && onViewDetail && <UiButton variant="outline" size="sm" onClick={() => onViewDetail(run.run_id)}>查看完整流水线</UiButton>}
                </div>
                {run && <div className="flex flex-wrap gap-3">{STAGE_ORDER.map(stage => {
                  const item = run.stage_runs.find(entry => entry.stage === stage);
                  return <span key={stage}>{STAGE_LABELS[stage] ?? stage}：{item?.status ?? "未开始"}</span>;
                })}</div>}
              </div>;
            })()}
          </div>
        )}
      </section>

      <UiSection title="处理结果汇总" description="会话状态与流水线执行次数采用不同口径，不进行相加">
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <UiMetric label="流水线运行" value={stats?.total ?? "—"} />
          <UiMetric label="运行失败" value={stats?.failed ?? "—"} />
          <UiMetric label="知识条目" value={stats?.knowledge_items ?? "—"} />
          <UiMetric label="向量索引" value={stats?.embeddings ?? "—"} />
        </div>
        {!pipelinePanelFresh && <UiNotice tone="warning">流水线统计暂不可用。</UiNotice>}
      </UiSection>

    </div>
  );
}
