import { describe, expect, it } from "vitest";
import processing from "../pages/ProcessingPage.tsx?raw";
describe("processing workspace polling and task controls", () => {
  it("does not issue automatic refresh while paused or hidden", () => {
    expect(processing).toContain("autoRefreshRef.current && document.visibilityState");
    expect(processing).toContain('autoRefresh ? "每 2 秒（窗口可见时）" : "已暂停"');
    expect(processing).toContain("setPolling(!autoRefresh)");
    expect(processing).toContain("void load(true)");
  });
  it("retains task actions, safe unmount and failure visibility", () => {
    expect(processing).toContain("mountedRef.current = false");
    expect(processing).toContain("if (!mountedRef.current) return");
    expect(processing).toContain("setConsecutiveFailures(previous => previous + 1)");
    expect(processing).toContain("retryFailedAiTask");
    expect(processing).toContain("cancelPendingAiTask");
    expect(processing).toContain("人工核查冲突");
    expect(processing).toContain("createSafeTaskDiagnosticJson");
  });
});
