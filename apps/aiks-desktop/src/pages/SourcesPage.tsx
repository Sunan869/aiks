import { useEffect, useState } from "react";
import { RefreshCw, Settings2 } from "lucide-react";
import { getApi } from "../api/client";
import type { FullStatus } from "../api/types";
import { useSourceCatalog } from "../ProviderCatalog";
import { saveProviderSettings } from "../api/provider-catalog";
import { sourceStatusPresentation, syncCatalogSource, type SourceDescriptor, type SourceStatusTone } from "../api/provider-catalog-model";

interface Props { fullStatus: FullStatus | null; }

const STATUS_TONE_CLASS: Record<SourceStatusTone, string> = {
  success: "text-green-600 dark:text-green-400",
  neutral: "text-gray-500 dark:text-gray-400",
  warning: "text-amber-600 dark:text-amber-400",
  danger: "text-red-600 dark:text-red-400",
};

const ISSUE_TONE_CLASS: Record<SourceStatusTone, string> = {
  success: "",
  neutral: "border-gray-200 bg-gray-50 text-gray-600 dark:border-gray-700 dark:bg-gray-900/40 dark:text-gray-300",
  warning: "border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/25 dark:text-amber-200",
  danger: "border-red-200 bg-red-50 text-red-700 dark:border-red-900/60 dark:bg-red-950/25 dark:text-red-200",
};

function SourceCard({ source, count, diagnostics, refresh }: { source: SourceDescriptor; count: number; diagnostics: string[]; refresh: () => Promise<void> }) {
  const [editing, setEditing] = useState(false);
  const [enabled, setEnabled] = useState(source.enabled);
  const [paths, setPaths] = useState(source.paths.join("\n"));
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  useEffect(() => { if (!editing) { setEnabled(source.enabled); setPaths(source.paths.join("\n")); } }, [source.enabled, source.paths, editing]);
  const sync = async () => {
    setBusy(true); setMessage("");
    try {
      const result = await syncCatalogSource(getApi(), source);
      setMessage(`扫描完成：+${result.new_count} 新增，${result.updated_count} 更新，${result.failed_count} 失败`);
      await refresh();
    } catch (e) { setMessage(`同步未完成：${String(e)}`); }
    finally { setBusy(false); }
  };
  const save = async () => {
    setBusy(true); setMessage("");
    try {
      await saveProviderSettings(source.key, enabled, paths.split(/\r?\n/).map(p => p.trim()).filter(Boolean));
      setMessage("已保存。请重启 AIKS 后应用；当前进程仍使用旧配置。");
      setEditing(false); await refresh();
    } catch (e) { setMessage(`保存失败：${String(e)}`); }
    finally { setBusy(false); }
  };
  const readable = source.enabled && source.status === "ok" && !source.restart_required;
  const status = sourceStatusPresentation(source);
  const hasDetails = Boolean(source.message?.trim()) || diagnostics.length > 0;
  return (
    <section className="rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold">{source.display_name}</h2>
        <span className={`text-xs font-medium ${STATUS_TONE_CLASS[status.tone]}`}>{status.label}</span>
      </div>
      <div className="mt-2 break-all text-xs text-gray-500">
        {source.paths.length
          ? <>检测目录：{source.paths.join(" · ")}</>
          : source.key === "aider"
            ? "需要配置项目根目录；不会自动扫描整个磁盘。"
            : "自动探测该工具的本地会话目录"}
      </div>

      {status.title && (
        <div className={`mt-3 rounded-md border px-3 py-2 text-xs ${ISSUE_TONE_CLASS[status.tone]}`}>
          <div className="font-medium">{status.title}</div>
          {status.description && <div className="mt-1 leading-5 opacity-80">{status.description}</div>}
        </div>
      )}

      {hasDetails && (
        <details className="mt-2 text-[11px] text-gray-400">
          <summary className="cursor-pointer select-none hover:text-gray-600 dark:hover:text-gray-300">查看详情</summary>
          <div className="mt-1.5 space-y-1 rounded-md bg-gray-50 px-3 py-2 font-mono leading-5 dark:bg-gray-900/50">
            {source.message?.trim() && <div>原始信息：{source.message}</div>}
            {diagnostics.length > 0 && <div>诊断：{diagnostics.join("、")}</div>}
          </div>
        </details>
      )}
      <div className="mt-3 flex items-center justify-between gap-2">
        <span className="text-xs text-gray-500">最近扫描发现 <strong className="text-blue-600">{count}</strong> 条会话</span>
        <div className="flex gap-2">
          <button type="button" onClick={() => setEditing(v => !v)} disabled={busy} className="flex items-center gap-1 rounded border border-gray-200 px-2.5 py-1 text-xs dark:border-gray-600"><Settings2 className="h-3 w-3" />目录与开关</button>
          <button type="button" onClick={() => void sync()} disabled={busy || !readable} className="flex items-center gap-1 rounded border border-blue-300 px-2.5 py-1 text-xs text-blue-600 disabled:opacity-40 dark:border-blue-700 dark:text-blue-400"><RefreshCw className={`h-3 w-3 ${busy ? "animate-spin" : ""}`} />立即同步</button>
        </div>
      </div>
      {editing && <div className="mt-4 border-t border-gray-100 pt-3 dark:border-gray-700">
        <label className="flex items-center gap-2 text-xs"><input type="checkbox" checked={enabled} onChange={e => setEnabled(e.target.checked)} disabled={busy} />启用 {source.display_name}</label>
        <label className="mt-3 block text-xs text-gray-500">数据根目录（每行一个；固定目录工具留空可自动探测）
          <textarea value={paths} onChange={e => setPaths(e.target.value)} rows={3} disabled={busy} spellCheck={false} className="mt-1 w-full rounded border border-gray-200 bg-transparent p-2 font-mono text-xs dark:border-gray-600" placeholder="填写包含会话存储的绝对目录，而不是安装目录" />
        </label>
        <p className="mt-1 text-[11px] text-gray-400">只读取会话和必要元数据。保存只修改当前数据源设置，重启生效；关闭来源不会删除已导入记录。</p>
        <div className="mt-2 flex gap-2"><button type="button" onClick={() => void save()} disabled={busy} className="rounded bg-blue-600 px-3 py-1.5 text-xs text-white disabled:opacity-40">保存配置</button><button type="button" onClick={() => setEditing(false)} disabled={busy} className="px-2 text-xs text-gray-500">取消</button></div>
      </div>}
      {message && <p role="status" className="mt-3 text-xs text-gray-600 dark:text-gray-300">{message}</p>}
    </section>
  );
}

export default function SourcesPage({ fullStatus }: Props) {
  const { sources, loading, error, refresh } = useSourceCatalog();
  return <div className="w-full min-w-0 p-6">
    <div className="mb-6 flex items-start justify-between gap-3"><div><h1 className="text-xl font-semibold">数据源</h1><p className="mt-1 text-xs text-gray-500">统一管理 AI 工具本地会话来源；健康状态不由会话数量推断。</p></div><button type="button" onClick={() => void refresh()} disabled={loading} className="rounded border border-gray-200 px-3 py-1.5 text-xs dark:border-gray-600">刷新状态</button></div>
    {error && <p role="alert" className="mb-4 text-sm text-red-600">读取数据源目录失败：{error}</p>}
    {loading && sources.length === 0 && <p className="text-sm text-gray-500">正在读取数据源目录…</p>}
    <div className="space-y-3">{sources.filter(source => source.configurable !== false).map(source => <SourceCard key={source.key} source={source} count={fullStatus?.scan_by_source[source.display_name] ?? 0} diagnostics={fullStatus?.provider_diagnostics?.[source.key] ?? []} refresh={refresh} />)}</div>
  </div>;
}
