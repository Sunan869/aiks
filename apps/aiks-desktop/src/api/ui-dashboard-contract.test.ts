import { describe, expect, it } from "vitest";
import sources from "../pages/SourcesPage.tsx?raw";
import processing from "../pages/ProcessingPage.tsx?raw";
import combo from "../components/ui/SearchSelect.tsx?raw";
import memory from "../pages/ProjectMemoryPage.tsx?raw";
describe("unified worklist UX", () => {
  it("filters source cards without mutating the provider scan or configuration", () => {
    expect(sources).toContain('aria-label="搜索数据源"');
    expect(sources).toContain("sourceStatusPresentation(source).tone");
    expect(sources).toContain("visibleSources.map(");
    expect(sources).toContain("saveProviderSettings");
  });
  it("supports local pipeline searching while keeping task retries", () => {
    expect(processing).toContain('id="processing-run-search"');
    expect(processing).toContain("filteredByStatus.filter(");
    expect(processing).toContain("handleBackfill");
  });
  it("uses stable combobox popup ids, not label-derived ids", () => {
    expect(combo).toContain("const listId = useId()");
    expect(combo).toContain("aria-controls={open ? listId : undefined}");
    expect(memory).toContain("项目记忆工作区");
  });
});
