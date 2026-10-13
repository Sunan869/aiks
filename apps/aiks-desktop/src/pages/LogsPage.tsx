import { UiButton, UiInput } from "../components/ui/fields";
import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

interface LogEntry { time: string; level: string; message: string; }

export default function LogsPage() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [query, setQuery] = useState("");
  const [level, setLevel] = useState<"all" | "INFO" | "WARN" | "ERROR">("all");
  const visibleLogs = logs.filter(log =>
    (level === "all" || log.level === level) &&
    (!query.trim() || `${log.time} ${log.level} ${log.message}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))
  );

  useEffect(() => {
    // Listen for log events from Rust backend
    const unlisten = listen<{ message: string }>("startup-progress", (e) => {
      setLogs((prev) => [
        ...prev,
        {
          time: new Date().toLocaleTimeString("zh-CN"),
          level: "INFO",
          message: e.payload.message,
        },
      ].slice(-200));
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  const levelColor = (level: string) => {
    switch (level) {
      case "ERROR": return "text-red-500";
      case "WARN": return "text-yellow-500";
      default: return "text-green-400";
    }
  };

  return (
    <div className="aiks-page-shell flex h-full min-h-0 min-w-0 flex-col">
      <div className="mb-4 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">日志</h1>
          <p className="text-gray-500 text-sm mt-1">运行日志</p>
        </div>
        <UiButton variant="outline" size="sm"
          onClick={() => setLogs([])}
          className="text-xs text-gray-400 hover:text-gray-600 px-2 py-1 rounded border"
        >
          清空
        </UiButton>
      </div>

      <div className="mb-3 flex flex-wrap items-center gap-2">
        <UiInput aria-label="筛选日志" value={query} onChange={event => setQuery(event.target.value)} placeholder="搜索日志内容…" className="min-w-0 flex-1" />
        <div role="group" aria-label="日志等级筛选" className="flex flex-wrap gap-1">
          {(["all", "INFO", "WARN", "ERROR"] as const).map(value => <UiButton key={value} variant={level === value ? "primary" : "ghost"} size="sm" aria-pressed={level === value} onClick={() => setLevel(value)}>{value === "all" ? "全部" : value}</UiButton>)}
        </div>
      </div>
      <p role="status" className="mb-2 text-xs text-slate-500">当前显示 {visibleLogs.length} / {logs.length} 条本次运行日志</p>
      <div role="region" aria-label="运行日志内容" tabIndex={0} className="min-h-[180px] flex-1 overflow-auto rounded-xl border border-slate-800 bg-slate-950 p-4 font-mono text-xs shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sky-500">
        {visibleLogs.length === 0 ? (
          <div className="text-gray-500">{logs.length === 0 ? "暂无日志" : "没有符合筛选条件的日志"}</div>
        ) : (
          visibleLogs.map((log, i) => (
            <div key={i} className="flex gap-2 mb-0.5">
              <span className="text-gray-500">{log.time}</span>
              <span className={levelColor(log.level)}>{log.level}</span>
              <span className="min-w-0 break-words text-gray-200">{log.message}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
