import { describe, expect, it } from "vitest";
import sourcesPageSource from "../pages/SourcesPage.tsx?raw";
import { mockSourceDescriptors } from "./provider-catalog.mock";
import { sourceStateLabel } from "./provider-catalog-model";
import apiTypesSource from "./types.ts?raw";
import engineSource from "../../../../crates/aiks-core/src/engine/mod.rs?raw";

describe("WorkBuddy desktop source contract", () => {
  it("lists WorkBuddy and maps it to the stable workbuddy sync key", () => {
    expect(mockSourceDescriptors().find(s => s.key === "workbuddy")?.display_name).toBe("WorkBuddy");
    expect(sourcesPageSource).toContain("syncCatalogSource");
  });

  it("uses provider health instead of session count as the detection signal", () => {
    expect(apiTypesSource).toContain("provider_health: Record<string, boolean>");
    expect(engineSource).toContain("pub provider_health:");
    expect(engineSource).toContain("health_check_all().await");
    expect(sourceStateLabel({ ...mockSourceDescriptors()[0], status: "ok" })).toBe("可读取");
    expect(sourcesPageSource).not.toContain("const detected = count > 0");
  });
});
