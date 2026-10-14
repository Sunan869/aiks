import { describe, expect, it } from "vitest";
import settings from "../pages/SettingsPage.tsx?raw";
import editor from "../components/KnowledgeEditor.tsx?raw";
import memory from "../pages/ProjectMemoryPage.tsx?raw";
describe("Radix Switch and Tabs migration", () => {
  it("preserves settings switch state through the Radix checked contract", () => {
    expect(settings).toContain("<Switch.Root checked={value} onCheckedChange={onChange}");
    expect(settings).toContain("<Switch.Thumb");
    expect(settings).toContain("ai_max_concurrent");
  });
  it("keeps editor content and preview switching controlled", () => {
    expect(editor).toContain('<Tabs.Root value={preview ? "preview" : "edit"}');
    expect(editor).toContain('setPreview(value === "preview")');
    expect(editor).toContain('Tabs.Trigger value="edit"');
    expect(editor).toContain('Tabs.Trigger value="preview"');
    expect(editor).toContain("createKnowledge(");
  });
  it("retains project memory workspace content and tabs", () => {
    expect(memory).toContain("<Tabs.Root value={workspaceTab}");
    expect(memory).toContain('aria-label="项目记忆工作区"');
    expect(memory).toContain("<Tabs.Trigger value={key}");
    expect(memory).toContain('workspaceTab === "tools"');
  });
});
