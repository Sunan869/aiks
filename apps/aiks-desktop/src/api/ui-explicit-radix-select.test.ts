import { describe, expect, it } from "vitest";
import sessions from "../pages/SessionsPage.tsx?raw";
import settings from "../pages/SettingsPage.tsx?raw";
import knowledge from "../pages/KnowledgeBasePageV3.tsx?raw";
import workbench from "../pages/KnowledgeWorkbenchPage.tsx?raw";
describe("explicit Radix Select business bindings", () => {
  it("removes fake DOM events from major filter and settings screens", () => {
    for (const source of [sessions, settings, knowledge, workbench]) {
      expect(source).toContain("<RadixSelectField");
      expect(source).not.toContain("<UiSelect");
    }
  });
  it("keeps resets and numeric configuration conversion", () => {
    expect(sessions).toContain("setSource(value); setPage(0)");
    expect(knowledge).toContain("setCategory(value); setPage(0)");
    expect(workbench).toContain("setCategory(value); setPage(0)");
    expect(settings).toContain('Number(value) * 60');
    expect(settings).toContain('update("ai_max_concurrent", Number(value))');
  });
});