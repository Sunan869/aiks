export type LogLevel = "INFO" | "WARN" | "ERROR";
export interface LogEntry { time: string; level: LogLevel; message: string; }

export function classifyLogLevel(message: string): LogLevel {
  if (/\b(ERROR|FATAL|PANIC|FAILED)\b|错误|失败|异常/i.test(message)) return "ERROR";
  if (/\b(WARN|WARNING|RETRY)\b|警告|重试/i.test(message)) return "WARN";
  return "INFO";
}

export function appendLogEntries(previous: LogEntry[], entry: LogEntry, capacity = 500): LogEntry[] {
  return [...previous, entry].slice(-capacity);
}

export function filterLogEntries(entries: LogEntry[], level: "all" | LogLevel, query: string): LogEntry[] {
  const needle = query.trim().toLocaleLowerCase();
  return entries.filter(log =>
    (level === "all" || log.level === level) &&
    (!needle || `${log.time} ${log.level} ${log.message}`.toLocaleLowerCase().includes(needle))
  );
}

export function formatLogExport(entries: LogEntry[]): string {
  return entries.map(log => `[${log.time}] [${log.level}] ${log.message}`).join("\n");
}
