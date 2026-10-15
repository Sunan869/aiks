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
  it("combines session and stage records in one accessible worklist", () => {
    expect(processing).toContain('aria-label="近期任务记录"');
    expect(processing).toContain('aria-label="所选会话处理阶段"');
    expect(processing).toContain("visibleTasks.length");
    expect(processing).toContain("expandedSession");
    expect(processing).toContain("retryFailedAiTask");
    expect(processing).toContain("cancelPendingAiTask");
  });
});
