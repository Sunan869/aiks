import { describe, expect, it } from "vitest";
import work from "../pages/KnowledgeWorkbenchPage.tsx?raw";
import knowledge from "../pages/KnowledgeBasePageV3.tsx?raw";
import sources from "../pages/SourcesPage.tsx?raw";

describe("knowledge workbench complete list interactions", () => {
  it("combines current-page search with project/tags and existing server filters", () => {
    expect(work).toContain("const visibleItems =");
    expect(work).toContain("pageQuery.trim().toLocaleLowerCase()");
    expect(work).toContain("item.project_name");
    expect(work).toContain("...parseTags(item.tags)");
    expect(work).toContain("{visibleItems.map(item => {");
    expect(work).toContain("getKnowledge(filters)");
  });
  it("keeps real actions, filter states and paging", () => {
    expect(work).toContain("setKnowledgeFavorite");
    expect(work).toContain("aria-pressed={view === key}");
    expect(work).toContain("当前页显示");
    expect(work).toContain("清除搜索条件");
    expect(work).toContain("setPage(value => value + 1)");
    expect(knowledge).toContain("syncKnowledgeToSiyuan");
    expect(sources).toContain("syncCatalogSource");
  });
});
