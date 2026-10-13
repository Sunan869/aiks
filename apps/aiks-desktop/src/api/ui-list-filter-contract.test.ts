import { describe, expect, it } from "vitest";
import sessions from "../pages/SessionsPage.tsx?raw";
import knowledge from "../pages/KnowledgeBasePageV3.tsx?raw";
import sources from "../pages/SourcesPage.tsx?raw";
import processing from "../pages/ProcessingPage.tsx?raw";

describe("consistent business list filtering", () => {
  it("searches the loaded session page without changing backend paging", () => {
    expect(sessions).toContain('aria-label="搜索当前页工作记录"');
    expect(sessions).toContain("currentPageItems.map(");
    expect(sessions).toContain("setPage(0)");
    expect(sessions).toContain("data?.total");
  });
  it("searches loaded knowledge and retains category filtering and publish command", () => {
    expect(knowledge).toContain('aria-label="搜索当前页知识"');
    expect(knowledge).toContain("currentPageItems.map(");
    expect(knowledge).toContain('<UiSelect aria-label="知识分类"');
    expect(knowledge).toContain("handleSyncToSiyuan");
  });
  it("supports resetting source diagnostics filters", () => {
    expect(sources).toContain('重置筛选');
    expect(sources).toContain("setOnlyIssues(false)");
  });
  it("keeps pipeline status filtering and search together", () => {
    expect(processing).toContain("filteredByStatus.filter(");
    expect(processing).toContain("runQuery");
  });
});
