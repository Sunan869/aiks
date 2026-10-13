import { describe, expect, it } from "vitest";
import { appendLogEntries, classifyLogLevel, filterLogEntries, formatLogExport, type LogEntry } from "../components/ui/log-utils";

const entry = (message: string, level: LogEntry["level"] = "INFO"): LogEntry => ({ time: "12:30:00", level, message });
describe("live desktop log center", () => {
  it("classifies diagnostic warnings and failures", () => {
    expect(classifyLogLevel("ERROR: provider failed")).toBe("ERROR");
    expect(classifyLogLevel("重试 LLM 请求")).toBe("WARN");
    expect(classifyLogLevel("同步完成")).toBe("INFO");
  });
  it("caps the live buffer without changing earlier state", () => {
    const original = [entry("first"), entry("second")];
    expect(appendLogEntries(original, entry("third"), 2).map(x => x.message)).toEqual(["second", "third"]);
    expect(original).toHaveLength(2);
  });
  it("filters by level, message and case-insensitive text", () => {
    const items = [entry("healthy"), entry("Connection FAILED", "ERROR"), entry("retry", "WARN")];
    expect(filterLogEntries(items, "ERROR", "connection")).toEqual([items[1]]);
    expect(filterLogEntries(items, "all", "  RETRY ")).toEqual([items[2]]);
    expect(filterLogEntries(items, "WARN", "healthy")).toEqual([]);
  });
  it("formats exactly the visible entries for support copying", () => {
    expect(formatLogExport([entry("READY"), entry("failed to connect", "ERROR")]))
      .toBe("[12:30:00] [INFO] READY\\n[12:30:00] [ERROR] failed to connect".replace("\\n", "\n"));
  });
});
