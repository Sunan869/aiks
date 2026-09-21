import { describe, expect, it } from "vitest";
import sourcesPageSource from "../pages/SourcesPage.tsx?raw";
import apiTypesSource from "./types.ts?raw";
import engineSource from "../../../../crates/aiks-core/src/engine/mod.rs?raw";

describe("WorkBuddy desktop source contract", () => {
  it("lists WorkBuddy and maps it to the stable workbuddy sync key", () => {
    expect(sourcesPageSource).toContain('"WorkBuddy"');
    expect(sourcesPageSource).toMatch(/"WorkBuddy"\s*:\s*"workbuddy"/);
  });

  it("uses provider health instead of session count as the detection signal", () => {
    expect(apiTypesSource).toContain("provider_health: Record<string, boolean>");
    expect(engineSource).toContain("pub provider_health:");
    expect(engineSource).toContain("health_check_all().await");
    expect(sourcesPageSource).toContain("fullStatus?.provider_health?.[src]");
    expect(sourcesPageSource).not.toContain("const detected = count > 0");
  });
});
