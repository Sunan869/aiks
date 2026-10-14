import { UiStatGrid } from "../components/ui/workspace";
import { PageHeader } from "../components/ui/PageHeader";
import { UiNotice, UiSection, UiMetric } from "../components/ui/surfaces";
import { UiButton } from "../components/ui/fields";
import { useCallback, useEffect, useState } from "react";
import { RefreshCw, Cpu, Search, AlertTriangle } from "lucide-react";
import { getApi } from "../api/client";
import type { FullStatus, AiStatus, TaskCenterStats, AiUsageStats } from "../api/types";

interface Props {
  fullStatus: FullStatus | null;
  aiStatus: AiStatus | null;
  syncInProgress: boolean;
  onRefresh: () => void;
}

/** Never equate Provider discoveries with distinct persisted sessions. */
export function overviewSyncBalance(status: FullStatus) {
  const classified = status.db_synced + status.db_pending + status.db_conflict + status.db_failed;
  return { classified, unclassified: Math.max(0, status.db_total - classified) };
}

export default function OverviewPage({ fullStatus, aiStatus, syncInProgress, onRefresh }: Props) {
  const [syncMsg, setSyncMsg] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [taskStats, setTaskStats] = useState<TaskCenterStats | null>(null);
  const [usage, setUsage] = useState<AiUsageStats | null>(null);
  const [usageError, setUsageError] = useState(false);
  const [taskError, setTaskError] = useState(false);
  const [taskLoading, setTaskLoading] = useState(true);
  const [aiKnowledgeCount, setAiKnowledgeCount] = useState<number | null>(null);
  const [aiKnowledgeError, setAiKnowledgeError] = useState(false);

  const loadTaskStats = useCallback(async () => {
    setTaskLoading(true);
    try {
      const stats = await getApi().getTaskCenterStats();
      setTaskStats(stats);
      setTaskError(false);
    } catch {
      setTaskStats(null);
      setTaskError(true);
    } finally {
      setTaskLoading(false);
    }
  }, []);

  const loadUsage = useCallback(async () => {
    try {
      const value = await getApi().getAiUsageStats();
      setUsage(value);
      setUsageError(false);
    } catch {
      setUsage(null);
      setUsageError(true);
    }
  }, []);

  const loadAiKnowledgeCount = useCallback(async () => {
    try {
      const page = await getApi().getKnowledge({ status: "active", sourceType: "conversation", limit: 1, offset: 0 });
      setAiKnowledgeCount(page.total);
      setAiKnowledgeError(false);
    } catch {
      setAiKnowledgeCount(null);
      setAiKnowledgeError(true);
    }
  }, []);

  useEffect(() => { void loadTaskStats(); void loadAiKnowledgeCount(); void loadUsage(); }, [loadTaskStats, loadAiKnowledgeCount, loadUsage]);

  const handleRefresh = () => { onRefresh(); void loadTaskStats(); void loadAiKnowledgeCount(); };
  const handleSync = async () => {
    if (syncing || syncInProgress) return;
    setSyncing(true);
    setSyncMsg(null);
    try {
      const r = await getApi().syncAndExtract();
      setSyncMsg(`同步提交完成：+${r.new_count} 新增，${r.updated_count} 更新`);
      onRefresh();
      void loadTaskStats();
      void loadAiKnowledgeCount();
    } catch {
      // Transport exceptions may contain local paths, provider payloads, or credentials.
      setSyncMsg("同步未完成，请到处理中心检查任务状态并重试。");
    } finally {
      setSyncing(false);
    }
  };

  const format = (n: number) => n.toLocaleString("zh-CN");
  const hasStatus = fullStatus !== null;
  const balance = fullStatus ? overviewSyncBalance(fullStatus) : null;
  const sources = Object.entries(fullStatus?.scan_by_source ?? {})
    .filter(([, count]) => count > 0)
    .sort((a, b) => b[1] - a[1]);
  const maxSourceCount = Math.max(1, ...sources.map(([, count]) => count));
  const isSyncing = syncing || syncInProgress;

  const metrics = [
    { label: "本地工作记录", value: hasStatus ? format(fullStatus.db_total) : "—", detail: "已入库的不同会话" },
    { label: "Provider 扫描发现", value: hasStatus ? format(fullStatus.scan_total) : "—", detail: "实时来源发现量，非入库数" },
    { label: "AI 知识条目", value: aiKnowledgeCount === null ? "—" : format(aiKnowledgeCount), detail: "知识库中当前有效的会话来源知识条目" },
    { label: "活跃数据源", value: hasStatus ? format(sources.length) : "—", detail: "扫描发现至少一条会话的来源" },
  ];

  return (
    <div className="aiks-page-shell min-w-0 space-y-5">
      <PageHeader title="概览" description="本地工作记录、来源发现、同步状态与 AI 处理进度（分别统计）"
        actions={<div className="flex flex-wrap items-center gap-2">
          <UiButton variant="outline" size="sm" onClick={handleRefresh} disabled={isSyncing}>
            <RefreshCw className="h-4 w-4" />刷新状态
          </UiButton>
          <UiButton variant="primary" size="sm" onClick={() => void handleSync()} disabled={isSyncing}>
            <RefreshCw className={`h-4 w-4 ${isSyncing ? "animate-spin" : ""}`} />
            {isSyncing ? "同步中…" : "扫描并同步"}
          </UiButton>
        </div>} />

      <UiStatGrid>
        {metrics.map(item => <UiMetric key={item.label} label={item.label} value={item.value} detail={item.detail} />)}
      </UiStatGrid>

      {!hasStatus && <UiNotice tone="warning">状态尚未加载。这里不会把未知的统计值显示成 0。</UiNotice>}
      {aiKnowledgeError && <UiNotice tone="warning">AI 知识条目统计读取失败，请刷新重试；此处不再使用旧提炼表的成功数替代。</UiNotice>}
      {syncMsg && <UiNotice tone={syncMsg.startsWith("同步提交完成") ? "success" : "danger"}>{syncMsg}</UiNotice>}

      <div className="grid gap-4 xl:grid-cols-2">
        <UiSection title="本地同步状态" description="以下项目来自同一批已入库会话的 SiYuan 同步状态">
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
            {([
              ["已同步", fullStatus?.db_synced, "text-emerald-600 dark:text-emerald-300"],
              ["待同步", fullStatus?.db_pending, "text-sky-600 dark:text-sky-300"],
              ["冲突", fullStatus?.db_conflict, "text-amber-600 dark:text-amber-300"],
              ["同步失败", fullStatus?.db_failed, "text-red-600 dark:text-red-300"],
            ] as const).map(([label, count, color]) => (
              <div key={label} className="rounded-lg border border-slate-200 bg-slate-50 p-3 text-center dark:border-slate-700 dark:bg-slate-800">
                <div className={`text-xl font-semibold tabular-nums ${color}`}>{count === undefined ? "—" : format(count)}</div>
                <div className="mt-1 text-xs text-slate-500 dark:text-slate-400">{label}</div>
              </div>
            ))}
          </div>
          {fullStatus && balance && (
            <div className="mt-3 space-y-2 text-xs text-slate-500 dark:text-slate-400">
              <div>本地总会话 {format(fullStatus.db_total)} · 已归类同步状态 {format(balance.classified)}</div>
              {balance.unclassified > 0 && <UiNotice tone="warning">另有 {format(balance.unclassified)} 条会话未归入上述四种同步状态，请检查同步目标记录；不代表这些会话已丢失。</UiNotice>}
              {balance.classified > fullStatus.db_total && <UiNotice tone="warning">同步状态合计超过本地总会话，可能存在重复目标记录，需要进一步检查。</UiNotice>}
              {fullStatus.last_sync_at && <div>最近同步：{new Date(fullStatus.last_sync_at).toLocaleString("zh-CN")} · 本次新入库 {format(fullStatus.last_sync_new)} · 更新 {format(fullStatus.last_sync_updated)} · 失败 {format(fullStatus.last_sync_failed)}</div>}
            </div>
          )}
        </UiSection>

        <UiSection title="AI 任务与处理状态" description="队列状态独立于“待同步”；任务数量不应与同步状态相加">
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            {([
              ["待处理", taskStats?.pending],
              ["运行中", taskStats?.running],
              ["已取消", taskStats?.cancelled],
              ["同步异常", taskStats?.sync_issues],
              ["AI 异常", taskStats?.ai_issues],
              ["提炼失败", fullStatus?.extraction_failed],
            ] as const).map(([label, count]) => (
              <div key={label} className="rounded-lg bg-slate-50 p-3 dark:bg-slate-800">
                <div className="text-lg font-semibold tabular-nums text-slate-900 dark:text-slate-100">{count == null ? "—" : format(count)}</div>
                <div className="text-xs text-slate-500 dark:text-slate-400">{label}</div>
              </div>
            ))}
          </div>
          <div className="mt-3 flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
            <Cpu className="h-4 w-4 shrink-0" />
            <span>AI 服务：{aiStatus ? (aiStatus.healthy ? "正常" : "暂时不可用") : "未知"}{aiStatus?.display_name ? ` · ${aiStatus.display_name}` : ""}</span>
          </div>
          {taskLoading && <p className="mt-2 text-xs text-slate-500">正在读取全库任务统计…</p>}
          {taskError && <UiNotice tone="warning" className="mt-2">任务统计获取失败，当前显示为未知；请刷新重试。</UiNotice>}
        </UiSection>
      </div>

      <UiSection title="AI 用量" description="来自本地模型请求日志的累计用量；未返回 usage 的请求不做估算">
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          {([
            ["输入 Token", usage?.input_tokens],
            ["输出 Token", usage?.output_tokens],
            ["已统计请求", usage?.measured_requests],
            ["总请求", usage?.requests],
          ] as const).map(([label, count]) => (
            <div key={label} className="rounded-lg bg-slate-50 p-3 dark:bg-slate-800">
              <div className="text-lg font-semibold tabular-nums">{count == null ? "—" : format(count)}</div>
              <div className="mt-1 text-xs text-slate-500 dark:text-slate-400">{label}</div>
            </div>
          ))}
        </div>
        {usage && <p className="mt-3 text-xs text-slate-500 dark:text-slate-400">累计失败请求 {format(usage.failed_requests)} · 完整返回输入/输出 Token 的请求 {format(usage.measured_requests)} / {format(usage.requests)}。Token 合计仅包含实际返回的数值。</p>}
        {usage && usage.by_model.length > 0 && <div className="mt-3 space-y-2">
          {usage.by_model.map(item => <div key={item.model} className="flex flex-wrap justify-between gap-2 text-xs text-slate-600 dark:text-slate-300">
            <span className="min-w-0 truncate" title={item.model}>{item.model}</span>
            <span className="tabular-nums">请求 {format(item.requests)} · 输入 {item.input_tokens == null ? "—" : format(item.input_tokens)} / 输出 {item.output_tokens == null ? "—" : format(item.output_tokens)}</span>
          </div>)}
        </div>}
        {usageError && <UiNotice tone="warning" className="mt-3">AI 用量读取失败，请刷新重试。</UiNotice>}
      </UiSection>

      <UiSection title="AI 提炼结果" description="基于提炼记录的状态统计；不等于有效知识条目数，也不代表模型 Token 消耗">
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          {([
            ["提炼成功", fullStatus?.extraction_success],
            ["已跳过", fullStatus?.extraction_skipped],
            ["待提炼", fullStatus?.extraction_pending],
            ["提炼失败", fullStatus?.extraction_failed],
          ] as const).map(([label, count]) => (
            <div key={label} className="rounded-lg bg-slate-50 p-3 dark:bg-slate-800">
              <div className="text-lg font-semibold tabular-nums text-slate-900 dark:text-slate-100">{count == null ? "—" : format(count)}</div>
              <div className="mt-1 text-xs text-slate-500 dark:text-slate-400">{label}</div>
            </div>
          ))}
        </div>
        <p className="mt-3 text-xs text-slate-500 dark:text-slate-400">模型用量需要独立采集真实的输入、输出 Token，暂不使用知识分块的 token_count 代替。</p>
      </UiSection>

      <div className="grid gap-4 xl:grid-cols-2">
        <UiSection title="来源发现分布" description="按 Provider 实时扫描数量排列；不代表已经同步入库的数量">
          {sources.length === 0 ? <p className="text-sm text-slate-500">暂无可显示的扫描来源数据。</p> : (
            <div className="space-y-3">
              {sources.map(([source, count]) => (
                <div key={source} className="grid grid-cols-[minmax(0,8rem)_minmax(0,1fr)_auto] items-center gap-3 text-xs">
                  <span className="truncate text-slate-700 dark:text-slate-200" title={source}>{source}</span>
                  <div className="h-2 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800" aria-label={`${source}：${count} 条扫描发现`}>
                    <div className="h-full rounded-full bg-sky-500" style={{ width: `${(count / maxSourceCount) * 100}%` }} />
                  </div>
                  <span className="tabular-nums text-slate-600 dark:text-slate-300">{format(count)}</span>
                </div>
              ))}
            </div>
          )}
          <div className="mt-4 flex items-start gap-2 text-xs text-slate-500 dark:text-slate-400">
            <Search className="mt-0.5 h-4 w-4 shrink-0" />
            <span>扫描发现与本地会话可能因去重、过滤及来源变化存在差异。两者的差额不能直接认定为漏同步；需要按来源及会话 ID 对账。</span>
          </div>
        </UiSection>

        <UiSection title="需要关注" description="仅列出已有统计可以证实的问题，不推测未知故障原因">
          <div className="space-y-3">
            {fullStatus && (fullStatus.db_failed > 0 || fullStatus.db_conflict > 0) &&
              <UiNotice tone="warning"><AlertTriangle className="mr-1 inline h-4 w-4" />同步失败 {format(fullStatus.db_failed)} · 冲突 {format(fullStatus.db_conflict)}，请前往处理中心检查。</UiNotice>}
            {taskStats && (taskStats.ai_issues > 0 || taskStats.sync_issues > 0) &&
              <UiNotice tone="warning">处理中心显示 AI 异常 {format(taskStats.ai_issues)}、同步异常 {format(taskStats.sync_issues)}；两项统计可能涉及相同会话。</UiNotice>}
            {fullStatus && fullStatus.extraction_pending > 0 &&
              <UiNotice>提炼待处理记录 {format(fullStatus.extraction_pending)}。此指标与任务队列 PENDING 的统计方式可能不同。</UiNotice>}
            {fullStatus && fullStatus.db_failed === 0 && fullStatus.db_conflict === 0 &&
              !taskError && taskStats && taskStats.ai_issues === 0 && taskStats.sync_issues === 0 &&
              <UiNotice tone="success">当前统计未发现同步冲突或异常任务；这不代表所有会话已经完成 AI 处理。</UiNotice>}
            {!fullStatus && <UiNotice tone="warning">概览状态读取中或不可用。</UiNotice>}
            {fullStatus && !taskStats && !taskLoading && <UiNotice tone="warning">任务统计不可用，请检查处理中心。</UiNotice>}
          </div>
        </UiSection>
      </div>
    </div>
  );
}
