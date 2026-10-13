import { describe, expect, it } from "vitest";
import sources from "../pages/SourcesPage.tsx?raw";
import processing from "../pages/ProcessingPage.tsx?raw";

describe("source and processing overview usability", () => {
  it("keeps source health separate from user filters", () => {
    expect(sources).toContain("const configurableSources");
    expect(sources).toContain('aria-label="数据源健康概览"');
    expect(sources).toContain("healthyCount");
    expect(sources).toContain("issueCount");
    expect(sources).toContain("visibleSources.length");
  });
  it("keeps independent accessible task and pipeline tables", () => {
    expect(processing).toContain('aria-label="近期任务记录"');
    expect(processing).toContain('aria-label="流水线阶段记录"');
    expect(processing).toContain("visibleTasks.length");
    expect(processing).toContain("filtered.length");
    expect(processing).toContain("retryFailedAiTask");
    expect(processing).toContain("cancelPendingAiTask");
  });
});
