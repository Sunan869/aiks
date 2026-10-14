import { describe, expect, it } from "vitest";
import overview from "../pages/OverviewPage.tsx?raw";
import app from "../App.tsx?raw";
describe("Overview snapshot consistency", () => {
  it("shows stored sessions as work records and distinguishes live discovery", () => {
    expect(overview).toContain('label: "本地工作记录"');
    expect(overview).toContain("fullStatus.db_total");
    expect(overview).toContain('label: "Provider 扫描发现"');
    expect(overview).toContain("fullStatus.scan_total");
    expect(app).toContain("fullStatus?.db_total");
    expect(app).toContain("fullStatus.db_total} 条本地工作记录");
  });
  it("keeps sync totals independently classified and reports unclassified entries", () => {
    expect(overview).toContain("status.db_synced + status.db_pending + status.db_conflict + status.db_failed");
    expect(overview).toContain("balance.unclassified > 0");
    expect(overview).toContain("balance.classified > fullStatus.db_total");
  });
  it("gets global task-center stats separately and handles errors", () => {
    expect(overview).toContain("getTaskCenterStats()");
    expect(overview).toContain("taskStats?.ai_issues");
    expect(overview).toContain("taskStats?.pending");
    expect(overview).toContain("setTaskError(true)");
    expect(overview).not.toContain("setSyncMsg(`错误：${e}`)");
  });
});
