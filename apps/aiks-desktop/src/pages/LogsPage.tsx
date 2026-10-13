import { UiEmptyState, UiStatusBadge } from "../components/ui/workspace";
import { PageHeader } from "../components/ui/PageHeader";
import { UiNotice } from "../components/ui/surfaces";
import { appendLogEntries, classifyLogLevel, filterLogEntries, formatLogExport, type LogEntry, type LogLevel } from "../components/ui/log-utils";
import { UiButton, UiInput } from "../components/ui/fields";
import { useState, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

export default function LogsPage() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [query, setQuery] = useState("");
  const [level, setLevel] = useState<"all" | LogLevel>("all");
  const [paused, setPaused] = useState(false);
  const pausedRef = useRef(false);
  const [dropped, setDropped] = useState(0);
  const [notice, setNotice] = useState("");
  const visibleLogs = filterLogEntries(logs, level, query);

  const setPauseState = (next: boolean) => {
    pausedRef.current = next;
    setPaused(next);
  };

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<{ message: string }>("startup-progress", event => {
      if (!active) return;
      if (pausedRef.current) {
        setDropped(count => count + 1);
        return;
      }
      const message = event.payload.message;
      setLogs(previous => appendLogEntries(previous, {
        time: new Date().toLocaleTimeString("zh-CN"),
        level: classifyLogLevel(message),
        message,
      }));
    }).then(stop => {
      if (active) unlisten = stop;
      else stop();
    }).catch(error => { if (active) setNotice("订阅日志失败：" + String(error)); });
    return () => { active = false; unlisten?.(); };
  }, []);

  const copyVisibleLogs = async () => {
    if (!visibleLogs.length) return;
    try {
      await navigator.clipboard.writeText(formatLogExport(visibleLogs));
      setNotice(`已复制 ${visibleLogs.length} 条日志`);
    } catch (error) {
      setNotice("复制失败：" + String(error));
    }
  };

  const levelColor = (logLevel: LogLevel) => {
    switch (logLevel) {
      case "ERROR": return "text-red-400";
      case "WARN": return "text-yellow-400";
      default: return "text-green-400";
    }
  };

  return (
    <div className="aiks-page-shell flex h-full min-h-0 min-w-0 flex-col">
      <PageHeader title="日志" description="本次运行的实时日志（最多保留 500 条）" actions={<>
        <UiButton variant={paused ? "primary" : "outline"} size="sm" aria-pressed={paused} onClick={() => setPauseState(!paused)}>{paused ? "继续接收" : "暂停接收"}</UiButton>
        <UiButton variant="outline" size="sm" onClick={() => void copyVisibleLogs()} disabled={visibleLogs.length === 0}>复制筛选结果</UiButton>
        <UiButton variant="outline" size="sm"
          onClick={() => { setLogs([]); setDropped(0); setNotice(""); }}
          className="text-xs text-gray-400 hover:text-gray-600 px-2 py-1 rounded border"
        >
          清空
        </UiButton></>} />

      {notice && <UiNotice tone={notice.startsWith("复制失败") || notice.startsWith("订阅日志失败") ? "danger" : "success"} className="mb-3">{notice}</UiNotice>}
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <UiInput aria-label="筛选日志" value={query} onChange={event => setQuery(event.target.value)} placeholder="搜索日志内容…" className="min-w-0 flex-1" />
        <div role="group" aria-label="日志等级筛选" className="flex flex-wrap gap-1">
          {(["all", "INFO", "WARN", "ERROR"] as const).map(value => <UiButton key={value} variant={level === value ? "primary" : "ghost"} size="sm" aria-pressed={level === value} onClick={() => setLevel(value)}>{value === "all" ? "全部" : value}</UiButton>)}
        </div>
      </div>
      <p role="status" className="mb-2 text-xs text-slate-500">当前显示 {visibleLogs.length} / {logs.length} 条日志{paused ? ` · 已暂停，忽略 ${dropped} 条` : ""}</p>
      <div role="region" aria-label="运行日志内容" tabIndex={0} className="min-h-[180px] flex-1 overflow-auto rounded-xl border border-slate-800 bg-slate-950 p-4 font-mono text-xs shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sky-500">
        {visibleLogs.length === 0 ? (
          <UiEmptyState title={logs.length === 0 ? "暂无日志" : "没有符合筛选条件的日志"} description={logs.length === 0 ? "运行期间的日志会显示在这里。" : "请调整等级或关键词筛选。"} />
        ) : (
          visibleLogs.map((log, i) => (
            <div key={i} className="mb-1 flex min-w-0 gap-2">
              <span className="text-gray-500">{log.time}</span>
              <span className={levelColor(log.level)}><UiStatusBadge tone={log.level === "ERROR" ? "danger" : log.level === "WARN" ? "warning" : "success"}>{log.level}</UiStatusBadge></span>
              <span className="min-w-0 break-words text-gray-200">{log.message}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
