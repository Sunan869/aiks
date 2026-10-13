import { describe, expect, it } from "vitest";
import surfaces from "../components/ui/surfaces.tsx?raw";
import overview from "../pages/OverviewPage.tsx?raw";
import sync from "../pages/SyncPage.tsx?raw";
import sources from "../pages/SourcesPage.tsx?raw";
import diagnostics from "../pages/DiagnosticsPage.tsx?raw";
import processing from "../pages/ProcessingPage.tsx?raw";

describe("shared UI workspace surfaces", () => {
  it("defines semantic reusable sections, metrics and notices", () => {
    expect(surfaces).toContain("function UiSection");
    expect(surfaces).toContain("function UiMetric");
    expect(surfaces).toContain("function UiNotice");
    expect(surfaces).toContain('tone === "danger" ? "alert" : "status"');
  });
  it("retains core actions and shared status presentation", () => {
    expect(overview).toContain("<UiMetric");
    expect(sync).toContain('<UiSection title="最近同步"');
    expect(sync).toContain('"sync_and_extract"');
    expect(sources).toContain('UiNotice tone="danger"');
    expect(diagnostics).toContain("getV41Diagnostics");
    expect(processing).toContain("retryFailedAiTask");
  });
});
