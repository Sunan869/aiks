import { describe, expect, it } from "vitest";
import header from "../components/ui/PageHeader.tsx?raw";
import page0 from "../pages/OverviewPage.tsx?raw";
import page1 from "../pages/SyncPage.tsx?raw";
import page2 from "../pages/DiagnosticsPage.tsx?raw";
import page3 from "../pages/LogsPage.tsx?raw";
import page4 from "../pages/SourcesPage.tsx?raw";
import page5 from "../pages/SessionsPage.tsx?raw";
import page6 from "../pages/KnowledgeBasePageV3.tsx?raw";
import page7 from "../pages/SearchPage.tsx?raw";

describe("shared desktop workspace page headers", () => {
  it("renders a typed responsive common header", () => {
    expect(header).toContain("interface PageHeaderProps");
    expect(header).toContain("flex-wrap");
    expect(header).toContain("dark:text-slate-50");
  });
  it("applies the header across eight workspaces and preserves critical actions", () => {
    for (const page of [page0, page1, page2, page3, page4, page5, page6, page7]) expect(page).toContain("<PageHeader");
    expect(page0).toContain("syncAndExtract()");
    expect(page1).toContain('"sync_and_extract"');
    expect(page2).toContain("getV41Diagnostics");
    expect(page3).toContain("setLogs([])");
    expect(page4).toContain("syncCatalogSource");
    expect(page5).toContain("setShowImport(true)");
    expect(page6).toContain("syncKnowledgeToSiyuan");
    expect(page7).toContain("searchKnowledge(");
  });
});
