import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

interface LogEntry { time: string; level: string; message: string; }

export default function LogsPage() {
  const [logs, setLogs] = useState<LogEntry[]>([]);

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
    <div className="p-6 h-full flex flex-col">
      <div className="mb-4 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">日志</h1>
          <p className="text-gray-500 text-sm mt-1">运行日志</p>
        </div>
        <button
          onClick={() => setLogs([])}
          className="text-xs text-gray-400 hover:text-gray-600 px-2 py-1 rounded border"
        >
          清空
        </button>
      </div>

      <div className="flex-1 bg-gray-950 rounded-lg p-4 font-mono text-xs overflow-auto">
        {logs.length === 0 ? (
          <div className="text-gray-500">暂无日志</div>
        ) : (
          logs.map((log, i) => (
            <div key={i} className="flex gap-2 mb-0.5">
              <span className="text-gray-500">{log.time}</span>
              <span className={levelColor(log.level)}>{log.level}</span>
              <span className="text-gray-200">{log.message}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
