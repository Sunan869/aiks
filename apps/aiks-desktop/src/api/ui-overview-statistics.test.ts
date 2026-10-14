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
    expect(overview).toContain('label: "AI 知识条目"');
    expect(overview).toContain('getKnowledge({ status: "active", sourceType: "conversation", limit: 1, offset: 0 })');
    expect(overview).not.toContain("format(fullStatus.extraction_success)");
    expect(app).toContain("sessionCount={sessionCount}");
    expect(app).not.toContain("fullStatus.db_total} 条本地工作记录");
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
    expect(overview).toContain("setTaskStats(null)");
    expect(overview).toContain("setTaskError(true)");
    expect(overview).not.toContain("setSyncMsg(`错误：${e}`)");
  });
  it("shows only observed AI request token usage", () => {
    expect(overview).toContain("getAiUsageStats()");
    expect(overview).toContain('title="AI 用量"');
    expect(overview).toContain("usage?.input_tokens");
    expect(overview).toContain("usage?.output_tokens");
    expect(overview).toContain("usage?.measured_requests");
    expect(overview).toContain("setUsage(null)");
  });
  it("separates extraction outcomes from token usage and effective knowledge", () => {
    expect(overview).toContain('title="AI 提炼结果"');
    expect(overview).toContain("fullStatus?.extraction_success");
    expect(overview).toContain("fullStatus?.extraction_skipped");
    expect(overview).toContain("fullStatus?.extraction_pending");
    expect(overview).toContain("fullStatus?.extraction_failed");
    expect(overview).toContain("暂不使用知识分块的 token_count");
  });
});
