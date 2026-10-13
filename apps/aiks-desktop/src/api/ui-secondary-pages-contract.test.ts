import { describe, expect, it } from "vitest";
import overview from "../pages/OverviewPage.tsx?raw";
import sync from "../pages/SyncPage.tsx?raw";
import diagnostics from "../pages/DiagnosticsPage.tsx?raw";
import logs from "../pages/LogsPage.tsx?raw";
import knowledge from "../pages/KnowledgeBasePageV3.tsx?raw";

describe("shared secondary page layouts", () => {
  it("preserves overview synchronization actions and responsive metrics", () => {
    expect(overview).toContain("handleSync");
    expect(overview).toContain("lg:grid-cols-4");
    expect(overview).toContain("aiks-page-shell");
  });
  it("keeps synchronization status and retry invocation", () => {
    expect(sync).toContain('"sync_and_extract"');
    expect(sync).toContain("aiks-page-shell");
  });
  it("retains health diagnostics and workbench checks", () => {
    expect(diagnostics).toContain('"get_doctor"');
    expect(diagnostics).toContain("getV41Diagnostics");
    expect(diagnostics).toContain("aiks-page-shell");
  });
  it("retains local event log and clear behavior", () => {
    expect(logs).toContain('"startup-progress"');
    expect(logs).toContain("setLogs([])");
  });
  it("preserves knowledge cards and unified visual hierarchy", () => {
    expect(knowledge).toContain("KnowledgeCard");
    expect(knowledge).toContain("hover:shadow-md");
  });
});
