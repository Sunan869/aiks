import { describe, expect, it } from "vitest";
import page from "../pages/DiagnosticsPage.tsx?raw";

describe("Diagnostics page safe partial failure contract", () => {
  it("uses independent probes and retains successful results", () => {
    expect(page).toContain("collectDiagnostics({");
    expect(page).toContain("result.doctor.status === \"ok\"");
    expect(page).toContain("result.ai.status === \"ok\"");
    expect(page).toContain("result.workbench.status === \"ok\"");
    expect(page).toContain("setProbeFailures(result.failures)");
  });

  it("does not export raw rejected transport errors or stale success states", () => {
    expect(page).not.toContain("String(e)");
    expect(page).toContain("setDoctor(null)");
    expect(page).toContain("setAiOk(null)");
    expect(page).toContain("setV41(null)");
    expect(page).toContain("最近全部检测请求成功");
    expect(page).toContain("为保护敏感配置，不显示原始错误详情");
  });
});
