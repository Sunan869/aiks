import { describe, expect, it } from "vitest";
import processing from "../pages/ProcessingDetailPage.tsx?raw";
import sessions from "../pages/SessionDetailPage.tsx?raw";
import knowledge from "../pages/KnowledgeDetailPage.tsx?raw";
import search from "../pages/SearchPage.tsx?raw";

describe("consistent desktop detail surfaces", () => {
  it("keeps processing retry and stage trace in a responsive layout", () => {
    expect(processing).toContain("aiks-page-shell");
    expect(processing).toContain("handleRetry");
    expect(processing).toContain("detail.stage_runs.map(");
    expect(processing).toContain("sm:grid-cols-3");
  });
  it("keeps read-only conversation, chunks, knowledge and rerun actions", () => {
    expect(sessions).toContain("xl:grid-cols-[320px_minmax(0,1fr)]");
    expect(sessions).toContain("LLM 切片");
    expect(sessions).toContain("原始对话");
    expect(sessions).toContain("handleTriggerPipeline");
  });
  it("keeps manual review and multi-source selection in knowledge detail", () => {
    expect(knowledge).toContain("aiks-page-shell");
    expect(knowledge).toContain('<UiSelect aria-label="关联类型"');
    expect(knowledge).toContain('<UiSelect aria-label="反馈类型"');
    expect(knowledge).toContain("<UiSelect multiple size={4}");
    expect(knowledge).toContain("发布到 SiYuan");
  });
  it("keeps searching from the same API and uses shared page shell", () => {
    expect(search).toContain("aiks-page-shell");
    expect(search).toContain("搜索");
  });
});
