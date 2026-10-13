import { describe, expect, it } from "vitest";
import overview from "../pages/OverviewPage.tsx?raw";
import sync from "../pages/SyncPage.tsx?raw";
import diagnostics from "../pages/DiagnosticsPage.tsx?raw";
import logs from "../pages/LogsPage.tsx?raw";
import search from "../pages/SearchPage.tsx?raw";
import uiSurfaces from "../components/ui/surfaces.tsx?raw";
import workspace from "../components/ui/workspace.tsx?raw";

describe("monitoring and search workspace workflows", () => {
  it("preserves sync actions with in-flight guards and responsive metrics", () => {
    expect(overview).toContain("syncAndExtract()");
    expect(overview).toContain("if (syncing || syncInProgress) return;");
    expect(sync).toContain('"sync_and_extract"');
    expect(sync).toContain('<UiNotice tone="danger"');
    expect(sync).toContain("<UiStatGrid>");
    expect(workspace).toContain('4: "sm:grid-cols-4"');
  });
  it("makes health failures visible and retains native diagnostics", () => {
    expect(diagnostics).toContain("setDiagnosticError");
    expect(diagnostics).toContain('<UiNotice tone="danger"');
    expect(uiSurfaces).toContain('tone === "danger" ? "alert" : "status"');
    expect(diagnostics).toContain("getV41Diagnostics");
  });
  it("supports log filtering and accessible history", () => {
    expect(logs).toContain("const visibleLogs =");
    expect(logs).toContain('aria-label="日志等级筛选"');
    expect(logs).toContain('"startup-progress"');
    expect(logs).toContain("setLogs([])");
  });
  it("ignores stale search responses and permits keyboard result navigation", () => {
    expect(search).toContain("++requestId.current");
    expect(search).toContain("currentId === requestId.current");
    expect(search).toContain("onViewKnowledge(item.id)");
    expect(search).toContain('role="search"');
  });
});
