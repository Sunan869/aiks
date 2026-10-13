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
  it("migrates remaining action buttons without retaining native button tags", () => {
    for (const page of pages) {
      expect(page).toContain("UiButton");
      expect(page).not.toMatch(/<button(?:\\s|>)/);
      expect(page).not.toContain("</button>");
    }
  });
  it("preserves task retry, manual-conflict and provider configuration actions", () => {
    expect(pages.join("\\n")).toContain("retryFailedAiTask");
    expect(pages.join("\\n")).toContain("人工核查冲突");
    expect(pages.join("\\n")).toContain("saveProviderSettings");
  });
});
