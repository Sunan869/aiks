import { describe, expect, it } from "vitest";
import memorySource from "../pages/ProjectMemoryPage.tsx?raw";
import processingSource from "../pages/ProcessingPage.tsx?raw";
import settingsSource from "../pages/SettingsPage.tsx?raw";

describe("workspace layout contract", () => {
  it("separates knowledge reading, review, evolution and low-frequency tools", () => {
    expect(memorySource).toContain('aria-label="项目记忆工作区"');
    for (const name of ["knowledge", "review", "evolution", "tools"]) {
      expect(memorySource).toContain('workspaceTab === "' + name + '"');
    }
    expect(memorySource).toContain("人工修正项目归属");
    expect(memorySource).toContain("工作回顾与 Agent 上下文");
  });

  it("keeps processing filters and existing status actions", () => {
    expect(processingSource).toContain("setTaskFilter(value)");
    expect(processingSource).toContain("handleBackfill");
    expect(processingSource).toContain('aria-label="任务统计"');
  });

  it("retains settings controls within clearly grouped sections", () => {
    expect(settingsSource).toContain('<Section title="AI 智能整理">');
    expect(settingsSource).toContain('<Section title="语义搜索">');
    expect(settingsSource).toContain("rounded-2xl border border-slate-200");
  });
});
