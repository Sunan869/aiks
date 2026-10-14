import { describe, expect, it } from "vitest";
import page0 from "../pages/ProcessingPage.tsx?raw";
import page1 from "../pages/KnowledgeWorkbenchPage.tsx?raw";
import page2 from "../pages/SettingsPage.tsx?raw";
import page3 from "../pages/ProjectMemoryPage.tsx?raw";

describe("major workspaces share one header contract", () => {
  it("uses the common header across processing, knowledge, settings and memory", () => {
    for (const page of [page0,page1,page2,page3]) expect(page).toContain("<PageHeader");
  });
  it("retains original business entry points", () => {
    expect(page0).toContain("backfillExtractions");
    expect(page0).toContain("retryFailedAiTask");
    expect(page1).toContain("getKnowledge(filters)");
    expect(page1).toContain("setCreating(true)");
    expect(page2).toContain("save(true)");
    expect(page3).toContain("项目记忆工作区");
    expect(page3).toContain("SearchSelect");
  });
});
