import { describe, expect, it } from "vitest";
import page0 from "../pages/KnowledgeBasePageV3.tsx?raw";
import page1 from "../pages/KnowledgeDetailPage.tsx?raw";
import page2 from "../pages/KnowledgePage.tsx?raw";
import page3 from "../pages/KnowledgeWorkbenchPage.tsx?raw";
import page4 from "../pages/KnowledgeWorkspacePage.tsx?raw";
import page5 from "../pages/ProcessingDetailPage.tsx?raw";
import page6 from "../pages/ProcessingPage.tsx?raw";
import page7 from "../pages/ProjectMemoryPage.tsx?raw";
import page8 from "../pages/SessionDetailPage.tsx?raw";
import page9 from "../pages/SettingsPage.tsx?raw";
import page10 from "../pages/SourcesPage.tsx?raw";

const pages = [page0, page1, page2, page3, page4, page5, page6, page7, page8, page9, page10];
describe("unified desktop action controls", () => {
  it("preserves shared actions and accessible native detail/disclosure controls", () => {
    for (const page of pages) expect(page).toContain("UiButton");
    expect(page0).toContain("查看知识：");
    expect(page2).toContain("打开知识库查看会话");
    expect(page3).toContain('aria-label={`查看知识：${item.title}`}');
    expect(page5).toContain("aria-expanded={expanded}");
    // An accessible native <button> is legitimate. Do not enforce a brittle
    // exact tag count as more keyboard-accessible actions are introduced.
  });
  it("preserves task retry, manual-conflict and provider configuration actions", () => {
    expect(pages.join("\\n")).toContain("retryFailedAiTask");
    expect(pages.join("\\n")).toContain("人工核查冲突");
    expect(pages.join("\\n")).toContain("saveProviderSettings");
  });
});
