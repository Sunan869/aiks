import { describe, expect, it } from "vitest";
import overview from "./pages/OverviewPage.tsx?raw";
import memory from "./pages/ProjectMemoryPage.tsx?raw";
import processing from "./pages/ProcessingPage.tsx?raw";
import sources from "./pages/SourcesPage.tsx?raw";
import settings from "./pages/SettingsPage.tsx?raw";
import diagnostics from "./pages/DiagnosticsPage.tsx?raw";
import en from "./locales/en-US.json";
describe("Dynamic dashboard and tab translation coverage", () => {
  it("localizes mapped dashboard label values", () => {
    for(const page of [overview,processing,sources]) expect(page).toContain("tr(label)");
    expect(sources).toContain("tr(description)");
    expect(diagnostics).toContain("tr(bridgeStatus)");
    expect(diagnostics).toContain("tr(String(label))");
  });
  it("localizes project memory tabs and topic group labels", () => {
    expect(memory).toContain("tr(label)");
    expect(memory).toContain("tr(topic.title)");
    expect(memory).toContain("translateUiText(item.reason)");
    for(const key of ["知识概览","待复核","知识演进","项目工具","架构与决策"]) expect(en[key as keyof typeof en]).toBeTruthy();
  });
  it("localizes numeric settings choices without changing persistent IDs", () => {
    expect(settings).toContain('tr("{{count}} 分钟"');
    expect(settings).toContain('tr("{{count}} 路"');
    expect(settings).toContain('value: "openai_responses"');
    expect(en["{{count}} 分钟"]).toContain("minutes");
  });
});
